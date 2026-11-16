package io.github.elaizes.elamon

import android.app.Application
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.annotation.StringRes
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonObject
import io.github.elaizes.elamon.data.APP_API
import io.github.elaizes.elamon.data.Action
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.ART_PATH
import io.github.elaizes.elamon.data.DEFAULT_LOG_LEVEL
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.Hello
import io.github.elaizes.elamon.data.LogFilter
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.StreamOpen
import io.github.elaizes.elamon.data.Subscribe
import io.github.elaizes.elamon.data.TimelineHistory
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiChanged
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.cleanName
import io.github.elaizes.elamon.data.reachPaired
import io.github.elaizes.elamon.data.takeCodePoints
import io.github.elaizes.elamon.data.text

enum class Conn { Connecting, Live, Reconnecting, Offline, Unauthorized, CertificateChanged, Refused, Incompatible }

data class MachineState(
	val conn: Conn = Conn.Connecting,
	val host: String? = null,
	/** What the computer shows this phone: as last kept until the computer is reached, then its own. */
	val ui: UiDoc? = null,
	/** [ui] is the one kept from before, not yet confirmed by the computer. */
	val uiKept: Boolean = true,
	/** The last data line of each dashboard, by its id. */
	val data: Map<String, DataLine> = emptyMap(),
	/** Counts changes to the graphs' points, which live in the view model, for a redraw. */
	val histVersion: Long = 0,
	/** When the last data line arrived (wall clock), for moving a track's position on between lines. */
	val receivedAt: Long = 0,
	/** Why the computer turned the stream away, with [Conn.Refused]. */
	val refused: Message? = null,
	/** With [Conn.Unauthorized]: this phone can't read its own token any more, rather than the computer refusing it. */
	val keyLost: Boolean = false,
	/** What the computer said it is, when it was reached; with [Conn.Incompatible], why there are no dashboards. */
	val hello: Hello? = null,
	val compat: Compat = Compat.Ok,
	/** With [Conn.Offline]: the addresses and names the computer was looked for at last, in vain. */
	val tried: List<String> = emptyList(),
	/** The graphs and timelines shown over a long range instead of live, by widget id. */
	val ranges: Map<String, RangeView> = emptyMap(),
	/** The lines of the logs of the dashboard the stream is on, by widget id; none for one before its first state. */
	val logs: Map<String, LogLines> = emptyMap(),
)

/** A graph or timeline shown over a long range: which one, and how asking the computer for it went. */
data class RangeView(
	val range: HistoryRange,
	/** Asked for, not answered yet. */
	val loading: Boolean = true,
	/** The computer couldn't give it: the live view stays, saying so. */
	val failed: Boolean = false,
	/** A graph's buckets: how long each is; a stream's point joins them only once one has passed. */
	val stepMs: Long = 0,
	/** A timeline's segments over the range. */
	val timeline: TimelineHistory? = null,
)

/**
 * What the stream is asked for: the dashboard on screen, how often, the tables asked for another
 * order than their own, and the logs for another filter than their own.
 */
data class Want(
	val dashboard: String?,
	val intervalMs: Int,
	val sorts: Map<String, String> = emptyMap(),
	val logs: Map<String, LogFilter> = emptyMap(),
) {
	/** With [table] in the order of [field]: one of its `sorts`, or its own order back. */
	fun sorted(table: Widget, field: String): Want =
		copy(sorts = if (field == table.sort?.field) sorts - table.id else sorts + (table.id to field))

	/** With [log] filtered by [level] and [search], each as it is when null: its own filter back when that is its level and no search. */
	fun filtered(log: Widget, level: String? = null, search: String? = null): Want {
		val had = logs[log.id]
		val own = log.level ?: DEFAULT_LOG_LEVEL
		val l = level ?: had?.level ?: own
		val q = (search ?: had?.search.orEmpty()).trim().takeCodePoints(Limits.LOG_SEARCH_MAX)
		return copy(logs = if (l == own && q.isEmpty()) logs - log.id else logs + (log.id to LogFilter(l, q)))
	}

	/** What the stream sends once it took [change]: this, with the logs' filters as the service has them now. */
	fun told(change: Subscribe, had: Map<String, LogFilter>): Want {
		val now = had.toMutableMap()
		for ((id, f) in change.log.orEmpty()) if (f == null) now -= id else now[id] = f
		return copy(logs = now)
	}
}

/** What an open stream needs, to send what is wanted: see [tell]. */
sealed interface Tell {
	/** It sends that already. */
	data object Already : Tell

	/** `POST /api/v1/stream/{id}` with this. */
	data class Change(val change: Subscribe) : Tell

	/** Only a stream opened anew does. */
	data object Reopen : Tell
}

/**
 * What a stream opened to wait [readTimeoutMs] for a line, and told [sent] since, needs to send
 * [want]; [ui] has the tables and logs. Opened anew when a line every `want.intervalMs` needs the
 * connection to wait another time ([Limits.streamReadTimeoutMs]), and when a table goes back to
 * its own order where that can't be asked for by name: a stream is told another order (largest
 * first), never to drop one, so a table's own order that is smallest first and also one of its
 * `sorts` comes back only with a new stream. A log's filter is told only with its dashboard on the
 * stream, as the service takes it: the others wait until theirs is, and are told again with it.
 */
fun tell(sent: Want, want: Want, ui: UiDoc?, readTimeoutMs: Int): Tell {
	if (Limits.streamReadTimeoutMs(want.intervalMs) != readTimeoutMs) return Tell.Reopen
	val sorts = want.sorts.filter { (k, v) -> sent.sorts[k] != v }.toMutableMap()
	for ((k, asked) in sent.sorts) {
		if (k in want.sorts) continue
		val table = ui?.let { findWidget(it, k) } ?: continue
		val listed = table.sorts.orEmpty()
		// An order the table no longer lists, the service doesn't keep to either.
		if (asked !in listed) continue
		val own = table.sort
		// Asked for by name, its own order comes as it is: largest first, or not one of the others.
		if (own != null && (own.desc || own.field !in listed)) sorts[k] = own.field else return Tell.Reopen
	}
	// A stream moved to another dashboard is told its logs' filters with it, whatever it kept of them:
	// their lines start anew then all the same.
	val moved = want.dashboard != sent.dashboard
	val logs = logsOf(ui, want.dashboard).filter { want.logs[it] != sent.logs[it] || (moved && it in want.logs) }.associateWith { want.logs[it] }
	val change = Subscribe(
		dashboard = want.dashboard?.takeIf { it != sent.dashboard },
		interval = want.intervalMs.takeIf { it != sent.intervalMs },
		sort = sorts.ifEmpty { null },
		log = logs.ifEmpty { null },
	)
	return if (change == Subscribe()) Tell.Already else Tell.Change(change)
}

/** The ids of the logs a stream on [dashboard] sends: its own, and the header's. */
private fun logsOf(ui: UiDoc?, dashboard: String?): List<String> {
	ui ?: return emptyList()
	return listOfNotNull(dashboard, ui.header).distinct().flatMap { d -> allWidgets(ui.dashboard(d)?.widgets.orEmpty()) }.filter { it.type == "log" }.map { it.id }
}

/** A line for the snackbar: one of the app's strings, or what the computer said, as it said it. */
sealed interface Message {
	data class Res(@StringRes val id: Int, val args: List<String> = emptyList()) : Message
	data class Text(val text: String) : Message
}

/** Actions running now, by their key, and what each command printed last time. */
data class ActionsState(
	val running: Set<String> = emptySet(),
	val results: Map<String, ActionResult> = emptyMap(),
	/** The action whose output is on screen. */
	val shown: String? = null,
)

/** The first value at once, then each one that stays for [ms]: quick to start, calm while things change. */
@OptIn(FlowPreview::class)
fun <T> Flow<T>.settle(ms: Long): Flow<T> {
	var first = true
	return debounce {
		if (first) {
			first = false
			0L
		} else ms
	}
}

/**
 * The largest power of two to divide a [width] by [height] image by that still leaves both sides
 * at least [want] pixels: what BitmapFactory's inSampleSize takes.
 */
fun sampleSize(width: Int, height: Int, want: Int): Int {
	if (want <= 0) return 1
	var s = 1
	while (s < (1 shl 16) && width / (s * 2) >= want && height / (s * 2) >= want) s *= 2
	return s
}

/** An image's bytes as a bitmap about [sizePx] across, read at a fraction of its size when it is larger. */
private fun decodeArt(bytes: ByteArray, sizePx: Int): Bitmap? {
	val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
	BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
	if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
	val opts = BitmapFactory.Options().apply { inSampleSize = sampleSize(bounds.outWidth, bounds.outHeight, sizePx) }
	return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)
}

/** Every widget of [widgets], those in groups included, depth first. */
fun allWidgets(widgets: List<Widget>): List<Widget> = widgets.flatMap { listOf(it) + allWidgets(it.widgets.orEmpty()) }

fun findWidget(ui: UiDoc, id: String): Widget? = ui.dashboards.firstNotNullOfOrNull { d -> allWidgets(d.widgets).find { it.id == id } }

/**
 * One computer's screen: one stream for it, told which dashboard is on screen (`POST
 * /api/v1/stream/{id}`) instead of opened again; the `ui` document, fetched again when the stream
 * says it changed and kept on the phone ([UiCache]); the graphs' points; the actions run.
 */
class MachineViewModel(app: Application, private val machineId: String) : AndroidViewModel(app) {
	private val store = MachineStore.get(app)
	private val cache = UiCache.get(app)

	/** What this screen keeps of the computer is kept only while it isn't forgotten (or paired again under another id). */
	private val ticket = cache.ticket(machineId)
	private val settings = Settings.get(app)
	private val _state = MutableStateFlow(MachineState())
	val state: StateFlow<MachineState> = _state.asStateFlow()

	/** The graphs' and sparks' points, by widget id. */
	private val hists = HashMap<String, Hist>()

	/** The points of the graphs shown over a long range, by widget id; and what asks the computer for them. */
	private val longHists = HashMap<String, Hist>()
	private val fetches = HashMap<String, Job>()

	/** What asks the computer for a log's older lines, by widget id. */
	private val logFetches = HashMap<String, Job>()
	private var histCount = 0L

	private val _actions = MutableStateFlow(ActionsState())
	val actions: StateFlow<ActionsState> = _actions.asStateFlow()

	// Kept until the screen collects them, even across a rotation.
	private val _messages = Channel<Message>(capacity = Limits.MESSAGES_KEPT, onBufferOverflow = BufferOverflow.DROP_OLDEST)

	/** One-line results and errors for a snackbar. */
	val messages: Flow<Message> = _messages.receiveAsFlow()

	private val wanted = MutableStateFlow(Want(settings.lastTab(machineId), Intervals.DEFAULT_MS))

	/** The dashboard the screen shows: a tab, or a page a widget opened. */
	val dashboard: StateFlow<Want> = wanted.asStateFlow()

	private var job: Job? = null
	private var stopping: Job? = null
	private var visible = false

	/** The address the stream last worked on: a new stream goes straight there. */
	private var workingHost: String? = null
	private var attempt = 0

	/** [NetworkChanges.count] when the computer was last looked for: a change since, and it is looked for again at once. */
	private var lookedAt = NetworkChanges.count.value

	init {
		NetworkChanges.watch(app)
		// What was kept starts the screen at once; whatever the computer says replaces it.
		viewModelScope.launch {
			val ui = cache.ui(machineId)
			val data = cache.data(machineId)
			if (ui != null && _state.value.uiKept && _state.value.ui == null) applyUi(ui, fresh = false)
			if (data.isNotEmpty()) _state.update { it.copy(data = data + it.data) }
		}
		// Paired again while this screen stays open (the pairing screen was left before the computer
		// answered): the stream starts over with the new token instead of staying turned away.
		viewModelScope.launch {
			store.machines
				.map { list -> list.find { it.id == machineId }?.let { Pair(it.tokenSealed, it.fingerprint) } }
				.distinctUntilChanged()
				.drop(1)
				.collect { if (it != null) credentialsChanged() }
		}
	}

	/** Shown, the stream starts over now; not shown, it does when the screen is back. */
	private fun credentialsChanged() {
		job?.cancel()
		job = null
		if (visible) start()
	}

	/**
	 * The screen is shown: stream while it is. When it stops, the stream stays for a few seconds,
	 * so turning the phone doesn't cost a reconnection; then it ends until the screen is back, and
	 * the last readings are kept for next time.
	 */
	fun setVisible(v: Boolean) {
		visible = v
		if (v) {
			stopping?.cancel()
			stopping = null
			if (job?.isActive != true) {
				start()
				// Back on the screen: the long ranges are asked for again, to reach to now.
				for ((id, view) in _state.value.ranges) fetchRange(id, view.range)
			}
		} else if (job?.isActive == true && stopping == null) {
			stopping = viewModelScope.launch {
				delay(Limits.STOP_GRACE_MS)
				job?.cancel()
				job = null
				stopping = null
				keepData()
				// What's on screen when it comes back is old until the stream is.
				_state.update { if (it.conn == Conn.Live) it.copy(conn = Conn.Reconnecting) else it }
			}
		}
	}

	private fun keepData() {
		val data = _state.value.data
		// Written apart from this screen, which may be going away.
		if (data.isNotEmpty()) cache.keep(machineId, data, ticket)
	}

	/** Shows [dashboard]: the stream is told, and sends its values from its next line. */
	fun show(dashboard: String) {
		wanted.update { it.copy(dashboard = dashboard) }
		if (_state.value.ui?.tabs?.contains(dashboard) == true) settings.setLastTab(machineId, dashboard)
	}

	/** [table] in the order of [field]: one of its `sorts`, largest first, or its own. */
	fun sort(table: Widget, field: String) {
		wanted.update { it.sorted(table, field) }
	}

	/** Shows the log [w]'s lines at [level] or worse that hold [search], each as it is when null: the stream is told, and sends them anew. */
	fun filterLog(w: Widget, level: String? = null, search: String? = null) {
		wanted.update { it.filtered(w, level, search) }
	}

	/**
	 * Asks the computer for the lines of the log [w] before those it shows, by the filter it shows
	 * them by. An answer to a log that started over since (its filter changed, the stream came back)
	 * is late, and left.
	 */
	fun loadOlder(w: Widget) {
		val had = _state.value.logs[w.id] ?: return
		if (had.loadingOlder || !had.canLoadOlder) return
		val filter = wanted.value.logs[w.id]
		val generation = had.generation
		changeLog(w.id, generation) { it.asking() }
		logFetches.remove(w.id)?.cancel()
		logFetches[w.id] = viewModelScope.launch {
			try {
				val c = client() ?: throw ElamondException(ElamondException.Kind.Unreachable)
				val page = c.log(w.id, had.before, filter?.level, filter?.search)
				changeLog(w.id, generation) { it.older(page) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				// A service before 0.8 has no older lines to give: no button then, rather than an error.
				val none = (e as? ElamondException)?.kind == ElamondException.Kind.NotFound
				changeLog(w.id, generation) { if (none) it.noOlder() else it.failed() }
			}
		}
	}

	/** Changes the lines of the log [id] while they are still those of [generation]. */
	private fun changeLog(id: String, generation: Int, change: (LogLines) -> LogLines) {
		_state.update { s ->
			val l = s.logs[id]?.takeIf { it.generation == generation } ?: return@update s
			s.copy(logs = s.logs + (id to change(l)))
		}
	}

	/** The points of the graph or spark [widget]; null until it has any. */
	fun hist(widget: String): Hist? = hists[widget]

	/** The points of the graph [widget] over the long range it shows; null while it shows none, or has none yet. */
	fun longHist(widget: String): Hist? = longHists[widget]

	/**
	 * Shows the graph or timeline [w] over [range], asked of the computer, the stream's points
	 * joining it as they come; null shows it live again.
	 */
	fun showRange(w: Widget, range: HistoryRange?) {
		fetches.remove(w.id)?.cancel()
		longHists.remove(w.id)
		if (range == null) {
			_state.update { it.copy(ranges = it.ranges - w.id) }
			return
		}
		_state.update { it.copy(ranges = it.ranges + (w.id to RangeView(range))) }
		fetchRange(w.id, range)
	}

	/**
	 * Asks the computer for [id] over [range], once the stream works. Asked again, what the screen
	 * has stays until the answer, and stays when there is none; asked the first time, no answer
	 * leaves the live view, saying so.
	 */
	private fun fetchRange(id: String, range: HistoryRange) {
		fetches.remove(id)?.cancel()
		fetches[id] = viewModelScope.launch {
			withTimeoutOrNull(Limits.REQUEST_TIMEOUT_MS.toLong()) { _state.first { it.conn == Conn.Live } }
			val w = _state.value.ui?.let { findWidget(it, id) }
			try {
				val c = client() ?: throw ElamondException(ElamondException.Kind.Unreachable)
				when (w?.type) {
					"graph" -> {
						val a = c.graphHistory(id, range.key)
						val count = w.series?.size?.coerceAtLeast(1) ?: 1
						// The range's buckets reach from its start to now.
						val h = Hist(count, (a.to - a.from).coerceAtLeast(a.stepMs)).apply {
							stepHintMs = a.stepMs
							replace(a.entry)
						}
						if (_state.value.ranges[id]?.range != range) return@launch
						longHists[id] = h
						histCount++
						settle(id, range) { it.copy(loading = false, failed = false, stepMs = a.stepMs) }
					}
					"timeline" -> {
						val a = c.timelineHistory(id, range.key)
						settle(id, range) { it.copy(loading = false, failed = false, timeline = a) }
					}
					else -> throw ElamondException(ElamondException.Kind.NotFound)
				}
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				val had = longHists[id] != null || _state.value.ranges[id]?.timeline != null
				settle(id, range) { it.copy(loading = false, failed = !had) }
			}
		}
	}

	/** Changes how [id] shows its long range while that is still [range]: an answer for one left since is late. */
	private fun settle(id: String, range: HistoryRange, change: (RangeView) -> RangeView) {
		_state.update { s ->
			val v = s.ranges[id]?.takeIf { it.range == range } ?: return@update s
			s.copy(ranges = s.ranges + (id to change(v)), histVersion = histCount)
		}
	}

	fun retry() {
		if (!visible) return
		job?.cancel()
		start()
	}

	/** A new session asks every address: the phone may be on another network by now. */
	private fun start() {
		attempt = 0
		workingHost = null
		_state.update {
			if (it.conn == Conn.Live) it
			else it.copy(conn = if (it.data.isEmpty()) Conn.Connecting else Conn.Reconnecting, refused = null, keyLost = false)
		}
		job = viewModelScope.launch { loop() }
	}

	private suspend fun loop() {
		while (true) {
			val machine = store.get(machineId) ?: return
			// Opened by the key store off the main thread, the first time; no token, nothing to ask the computer.
			val token = withContext(Dispatchers.Default) { store.token(machine) }
			if (token == null) {
				_state.update { it.copy(conn = Conn.Unauthorized, keyLost = true) }
				return
			}
			if (workingHost == null) {
				_state.update { it.copy(conn = if (it.data.isEmpty() && it.conn != Conn.Live) Conn.Connecting else Conn.Reconnecting) }
				lookedAt = NetworkChanges.count.value
				val reached = try {
					reachPaired(machine)
				} catch (e: ElamondException) {
					_state.update { it.copy(conn = Conn.CertificateChanged) }
					return
				}
				if (reached == null) {
					_state.update { it.copy(conn = Conn.Offline, tried = machine.hostsToTry()) }
					waitToLookAgain()
					continue
				}
				val hello = reached.second
				val compat = compatibility(BuildConfig.VERSION_NAME, hello.api, hello.version, hello.minApp)
				_state.update { it.copy(hello = hello, compat = compat) }
				// Only API 2 is read: with any other there is nothing to show but what to update.
				if (hello.api != APP_API) {
					_state.update { it.copy(conn = Conn.Incompatible) }
					return
				}
				workingHost = reached.first.host.also { if (it != machine.lastHost) store.update(machineId) { s -> s.copy(lastHost = it) } }
			}
			val host = workingHost ?: continue
			// Opened anew at once, on the same address, when only a new stream sends what is wanted.
			val e = streamOn(host, ElamondClient(host, machine.port, machine.fingerprint, token)) ?: continue
			when {
				e.kind == ElamondException.Kind.Unauthorized -> {
					_state.update { it.copy(conn = Conn.Unauthorized, keyLost = false) }
					return
				}
				// Asking again won't change the answer: say why and wait for the user.
				e.status == 403 -> {
					_state.update { it.copy(conn = Conn.Refused, refused = messageFor(e)) }
					return
				}
			}
			// Lost, or another certificate (which the next look tells apart from another computer):
			// look for the computer on every address again.
			workingHost = null
			_state.update { it.copy(conn = Conn.Reconnecting) }
			waitToLookAgain()
		}
	}

	/**
	 * Waits before looking for the computer again, the longer the more tries in a row came to
	 * nothing. The phone's network changing (Wi-Fi, mobile data, a VPN) since it was last looked for
	 * ends the wait at once, and the count starts over: every address is tried again, all at once,
	 * from where the phone is now.
	 */
	private suspend fun waitToLookAgain() {
		val changed = withTimeoutOrNull(Limits.backoffMs(attempt)) { NetworkChanges.count.first { it != lookedAt } }
		attempt = if (changed != null) 0 else attempt + 1
	}

	/**
	 * Streams from [host] until the connection fails, and returns why; null when it is to be opened
	 * anew at once ([Tell.Reopen]). What the screen shows changes by telling the stream, where it can.
	 */
	private suspend fun streamOn(host: String, client: ElamondClient): ElamondException? = try {
		coroutineScope {
			val opening = wanted.value
			// The connection waits for a line as long as the interval it opens with asks.
			val timeout = Limits.streamReadTimeoutMs(opening.intervalMs)
			// A new stream has its tables in their own order and its logs by their own filter: those
			// wanted otherwise are told once it is open.
			var sent = opening.copy(sorts = emptyMap(), logs = emptyMap())
			var streamId: String? = null
			// Changes to what is wanted, told to the open stream once it has said its id.
			val subscriber = launch(start = kotlinx.coroutines.CoroutineStart.LAZY) {
				wanted.settle(Limits.PARAMS_SETTLE_MS).collect { w ->
					val id = streamId ?: return@collect
					var target = w
					// Twice at most: what a refusal of one part leaves out is told once more.
					repeat(2) {
						val change = when (val t = tell(sent, target, _state.value.ui, timeout)) {
							Tell.Already -> return@collect
							Tell.Reopen -> throw ReopenStream()
							is Tell.Change -> t.change
						}
						try {
							client.subscribe(id, change)
							sent = target.told(change, sent.logs)
							return@collect
						} catch (e: ElamondException) {
							when (e.code) {
								// The dashboard went from this phone's ui on the computer: the ui is read again,
								// and the screen and the stream follow it.
								"no_such_dashboard" -> {
									freshUi(client)
									target = wanted.value
									// Still there as the computer says: not asked for again.
									if (target.dashboard == change.dashboard) sent = sent.copy(dashboard = target.dashboard)
								}
								// Orders the computer's tables don't have (its settings changed): dropped, the rest told again.
								"bad_sort" -> {
									val asked = change.sort.orEmpty().keys
									val bad = e.detail?.takeIf { it in asked }?.let(::setOf) ?: asked
									wanted.update { it.copy(sorts = it.sorts - bad) }
									sent = sent.copy(sorts = sent.sorts - bad)
									target = target.copy(sorts = target.sorts - bad)
									say(messageFor(e))
								}
								// Filters for logs the computer's dashboards don't have (its settings changed):
								// dropped, the rest told again.
								"bad_param" -> {
									val asked = change.log.orEmpty().keys
									if (e.detail != "log" || asked.isEmpty()) throw e
									wanted.update { it.copy(logs = it.logs - asked) }
									sent = sent.copy(logs = sent.logs - asked)
									target = target.copy(logs = target.logs - asked)
									say(messageFor(e))
								}
								// The stream ended, or something else: opened again.
								else -> throw e
							}
						}
					}
				}
			}
			client.stream(opening.dashboard, opening.intervalMs).collect { line ->
				attempt = 0
				when (line) {
					is StreamOpen -> {
						streamId = line.stream
						val ui = _state.value.ui
						if (ui == null || ui.version != line.ui || _state.value.uiKept) freshUi(client)
						// Opened without a dashboard, the stream sends the first tab's.
						if (opening.dashboard == null) sent = sent.copy(dashboard = _state.value.ui?.tabs?.firstOrNull())
						subscriber.start()
					}
					is DataLine -> handleData(line, host)
					is UiChanged -> {
						told(client, line.endpoints)
						if (line.ui != _state.value.ui?.version) freshUi(client)
					}
				}
			}
			throw ElamondException(ElamondException.Kind.Unreachable, "closed")
		}
	} catch (e: CancellationException) {
		throw e
	} catch (e: ReopenStream) {
		null
	} catch (e: ElamondException) {
		e
	} catch (e: Exception) {
		ElamondException(ElamondException.Kind.Protocol, e.message, e)
	}

	/** Ends a stream that only a new one replaces: see [Tell.Reopen]. */
	private class ReopenStream : Exception()

	/** Reads the `ui` from the computer over [client], and takes it, and where the computer says it is. */
	private suspend fun freshUi(client: ElamondClient) {
		val ui = client.ui()
		told(client, ui.service.endpoints)
		applyUi(ui, fresh = true)
	}

	/**
	 * Where the computer says it is now ([endpoints]), in an answer over [client]: pinned to its
	 * certificate and with its token, as every client here is. Kept among its addresses, so the next
	 * look tries them too.
	 */
	private fun told(client: ElamondClient, endpoints: List<String>) {
		val pin = client.pin ?: return
		if (endpoints.isNotEmpty()) store.takeEndpoints(machineId, pin, endpoints)
	}

	/**
	 * Takes [ui] as what the computer shows this phone: kept on the phone when [fresh] from the
	 * computer, its interval and tabs what the stream is asked for.
	 */
	private fun applyUi(ui: UiDoc, fresh: Boolean) {
		// The computer's name is shown and kept: one that can't pass for another, or reorder what follows it.
		val clean = ui.copy(service = ui.service.copy(name = cleanName(ui.service.name)))
		_state.update { it.copy(ui = clean, uiKept = !fresh) }
		val interval = Intervals.effective(clean.profile.intervalMs, clean.service.intervalMs)
		wanted.update { w ->
			val d = w.dashboard?.takeIf { clean.dashboard(it) != null } ?: clean.tabs.firstOrNull()
			// A table's order asked for stays only while the table has it among its sorts.
			val sorts = w.sorts.filter { (id, f) -> clean.dashboards.any { dash -> allWidgets(dash.widgets).any { it.id == id && f in it.sorts.orEmpty() } } }
			// And a log's filter while it is a log.
			val logs = w.logs.filterKeys { id -> findWidget(clean, id)?.type == "log" }
			w.copy(dashboard = d, intervalMs = interval, sorts = sorts, logs = logs)
		}
		for (h in hists.values) h.stepHintMs = interval.toLong()
		// A long range stays only while the widget still offers it.
		val gone = _state.value.ranges.filter { (id, v) -> findWidget(clean, id)?.ranges?.contains(v.range.key) != true }.keys
		if (gone.isNotEmpty()) {
			for (id in gone) {
				fetches.remove(id)?.cancel()
				longHists.remove(id)
			}
			_state.update { it.copy(ranges = it.ranges - gone) }
		}
		if (!fresh) return
		viewModelScope.launch { cache.putUi(machineId, clean, ticket) }
		val saved = store.get(machineId) ?: return
		val name = clean.service.name.ifEmpty { saved.name }
		if (name != saved.name) {
			store.update(machineId) { it.copy(name = name) }
		}
	}

	private fun handleData(line: DataLine, host: String) {
		val ui = _state.value.ui
		val dash = ui?.dashboard(line.dashboard)
		if (ui != null && dash != null) {
			for (w in allWidgets(dash.widgets)) {
				val graph = w.type == "graph"
				if (!graph && !(w.type == "value" && w.spark == true)) continue
				val count = if (graph) w.series?.size?.coerceAtLeast(1) ?: 1 else 1
				// A spark reaches as far back as the service keeps.
				val window = (if (graph) w.range else null)?.takeIf { it > 0 }?.let { it * MS_PER_SECOND } ?: ui.service.historySeconds * MS_PER_SECOND
				val h = hists[w.id]?.takeIf { it.seriesCount == count } ?: Hist(count, window).also { hists[w.id] = it }
				h.windowMs = window
				h.stepHintMs = wanted.value.intervalMs.toLong()
				val s = line.widgets[w.id]
				val values = if (graph) s?.numbers.orEmpty() else listOf(s?.number)
				h.take(line.t, values, line.history?.get(w.id))
				// Over a long range, a point to a bucket.
				val step = _state.value.ranges[w.id]?.stepMs
				if (step != null) longHists[w.id]?.appendEvery(line.t, values, step)
			}
		}
		histCount++
		// The header's pills come with every line, whatever dashboard it is for.
		val header = ui?.let { headerData(it, line) }?.let { it.dashboard to it }
		// A log's state is what came since the line before, not a reading: its lines are kept here, those
		// of the line's dashboard alone.
		val logs = dash?.let { allWidgets(it.widgets) }.orEmpty().filter { it.type == "log" }
		val logIds = logs.map { it.id }.toSet()
		_state.update {
			it.copy(
				conn = Conn.Live,
				host = host,
				data = it.data + (line.dashboard to line.copy(history = null, widgets = line.widgets - logIds)) + listOfNotNull(header),
				logs = logs.mapNotNull { w ->
					val had = it.logs[w.id]
					val s = line.widgets[w.id] ?: return@mapNotNull had?.let { l -> w.id to l }
					w.id to (had ?: LogLines()).take(s, w.lines ?: Limits.LOG_LINES_DEFAULT)
				}.toMap(),
				histVersion = histCount,
				receivedAt = System.currentTimeMillis(),
				refused = null,
			)
		}
	}

	/** This phone's id in the computer's list of paired phones, for the commands that change its role or profile; null while it isn't known. */
	val clientId: String? get() = store.get(machineId)?.clientId

	// ---- actions --------------------------------------------------------------------

	/** A client for one-off requests, on the address the stream found. */
	private fun client(): ElamondClient? {
		val machine = store.get(machineId) ?: return null
		val host = _state.value.host ?: workingHost ?: return null
		return ElamondClient(host, machine.port, machine.fingerprint, store.token(machine) ?: return null)
	}

	private fun say(m: Message) {
		_messages.trySend(m)
	}

	/** Reads the `ui` again: the role, control or the actions changed on the computer since it was read. */
	private fun refreshUi() {
		viewModelScope.launch {
			val c = client() ?: return@launch
			try {
				freshUi(c)
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				// The stream says `ui` when it changes; this was only a shortcut.
			}
		}
	}

	/**
	 * Runs [action] with [params], once the screen has asked what it needs to. [key] tells this run
	 * apart from others of the same action (a row's), for what shows it running.
	 */
	fun run(action: Action, params: JsonObject, key: String = action.id, confirmText: String? = null) {
		if (key in _actions.value.running) return
		_actions.update { it.copy(running = it.running + key) }
		viewModelScope.launch {
			try {
				val c = client() ?: throw ElamondException(ElamondException.Kind.Unreachable)
				val r = c.action(action.id, params, confirmText)
				// A command of the settings' own: how it ended, and its output, open once it's done. A
				// station's says only that the station did it ("done").
				if (action.kind != "nut" && (r.exitCode != null || r.output != null || r.timedOut)) {
					_actions.update { it.copy(results = it.results + (action.id to r), shown = action.id) }
				} else {
					done(action, params)?.let(::say)
				}
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				say(messageFor(e))
				// The role, control or the action changed on the computer since the ui was read.
				if ((e as? ElamondException)?.code in REREAD) refreshUi()
			} finally {
				_actions.update { it.copy(running = it.running - key) }
			}
		}
	}

	/** Shows the output an action printed last, or with null closes it. */
	fun showActionResult(id: String?) {
		_actions.update { it.copy(shown = id) }
	}

	/** A player's cover, decoded off the main thread at about [sizePx] square; null when there's none to show. */
	suspend fun art(player: String, sizePx: Int): Bitmap? {
		val c = client() ?: return null
		val bytes = try {
			c.bytes("$ART_PATH?player=" + ElamondClient.query(player))
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			return null
		}
		return withContext(Dispatchers.Default) { orNull { decodeArt(bytes, sizePx) } }
	}

	override fun onCleared() {
		keepData()
	}

	class Factory(private val app: Application, private val id: String) : ViewModelProvider.Factory {
		@Suppress("UNCHECKED_CAST")
		override fun <T : ViewModel> create(modelClass: Class<T>): T = MachineViewModel(app, id) as T
	}

	private companion object {
		const val MS_PER_SECOND = 1000L

		/** Refusals after which the ui is read again: what this phone may do changed on the computer. */
		val REREAD = setOf("not_allowed", "control_disabled", "no_such_action", "confirm_required")
	}
}

/** What to say once a built-in action is done, as 0.5 said it; null for those whose effect shows by itself. */
fun done(action: Action, params: JsonObject): Message? {
	val id = when (action.id) {
		"power.lock" -> R.string.done_locked
		"power.poweroff" -> R.string.done_shutting_down
		"power.reboot" -> R.string.done_restarting
		"power.suspend" -> R.string.done_going_to_sleep
		"process.signal" -> if (params["signal"].text() == "kill") R.string.done_killed else R.string.done_asked_to_quit
		"unit.start" -> R.string.done_started
		"unit.stop" -> R.string.done_stopped
		"unit.restart" -> R.string.done_restarted
		// Playback, the volume and the power profile show by themselves; a station takes a while to answer.
		else -> return if (action.kind == "actions" || action.kind == "nut") Message.Res(R.string.done_ran, listOf(action.name)) else null
	}
	return Message.Res(id)
}

/**
 * The states of the header's widgets (the pills under the computer's name) that [line] carries
 * beside those of its own dashboard, as a line of the header dashboard; null when there are none.
 */
fun headerData(ui: UiDoc, line: DataLine): DataLine? {
	val id = ui.header?.takeIf { it != line.dashboard } ?: return null
	val ids = ui.dashboard(id)?.widgets?.let(::allWidgets)?.map { it.id }?.toSet().orEmpty()
	val states = line.widgets.filterKeys { it in ids }
	return if (states.isEmpty()) null else DataLine(line.t, id, states)
}

/**
 * Whether the readings of [dashboard] are as live as the stream: its last line no more than
 * [Limits.LIVE_LINES] lines behind the newest of any dashboard. Kept from an earlier look, or from
 * before the app was closed, they aren't until the stream sends the dashboard's own.
 */
fun readingsLive(data: Map<String, DataLine>, dashboard: String, intervalMs: Int): Boolean {
	val line = data[dashboard] ?: return false
	val newest = data.values.maxOf { it.t }
	return newest - line.t <= Limits.LIVE_LINES * intervalMs.toLong()
}
