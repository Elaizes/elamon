package io.github.elaizes.elamon

import android.app.Application
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.SystemClock
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
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ControlInfo
import io.github.elaizes.elamon.data.History
import io.github.elaizes.elamon.data.Info
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.Listener
import io.github.elaizes.elamon.data.LogEntry
import io.github.elaizes.elamon.data.Proc
import io.github.elaizes.elamon.data.Sample
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.OwnKeys
import io.github.elaizes.elamon.data.LinkDef
import io.github.elaizes.elamon.data.StreamMessage
import io.github.elaizes.elamon.data.UnitInfo
import io.github.elaizes.elamon.data.reachPaired

enum class Conn { Connecting, Live, Reconnecting, Offline, Unauthorized, CertificateChanged, Refused }

data class MachineState(
	val conn: Conn = Conn.Connecting,
	val host: String? = null,
	val info: Info? = null,
	val sample: Sample? = null,
	/** The same buffers throughout; [histVersion] tells their changes apart. */
	val hist: Hist = Hist(),
	val histVersion: Long = 0,
	val processes: List<Proc>? = null,
	val processCount: Int = 0,
	/** When the last sample arrived, for extrapolating media positions. */
	val receivedAt: Long = 0,
	/** Why the computer turned the stream away, with [Conn.Refused]. */
	val refused: Message? = null,
	/** With [Conn.Unauthorized]: this phone can't read its own token any more, rather than the computer refusing it. */
	val keyLost: Boolean = false,
)

/** What a list of processes needs from the service: the order, and how many of the heaviest. */
data class ProcRequest(val sort: String, val count: Int) {
	companion object {
		/** A detail page's top list, the Processes tab's, and every process for searching or grouping. */
		const val TOP = Limits.PROCESSES_TOP
		const val LIST = Limits.PROCESSES_LIST
		const val ALL = Limits.PROCESSES_ALL
	}
}

/** What the stream is opened with; a change reopens it on the same address. */
private data class StreamParams(val intervalMs: Int, val procs: Int, val sort: String, val media: Boolean)

/** A line for the snackbar: one of the app's strings, or what the computer said, as it said it. */
sealed interface Message {
	data class Res(@StringRes val id: Int, val args: List<String> = emptyList()) : Message
	data class Text(val text: String) : Message
}

/** The actions panel: the one running, and what each printed last time. */
data class ActionsState(
	val running: String? = null,
	val results: Map<String, ActionResult> = emptyMap(),
	/** The action whose output is on screen. */
	val shown: String? = null,
)

/** The activity log as last read: null until it is; [error] when reading it failed. */
data class ActivityState(
	val entries: List<LogEntry>? = null,
	val loading: Boolean = false,
	val error: Message? = null,
)

/** The first value at once, then each one that stays for [ms]: quick to start, calm while settings change. */
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

class MachineViewModel(app: Application, private val machineId: String) : AndroidViewModel(app) {
	private val store = MachineStore.get(app)
	private val ownKeys = OwnKeys.get(app)
	private val settings = Settings.get(app)
	private val hist = Hist()
	private val _state = MutableStateFlow(MachineState(hist = hist))
	val state: StateFlow<MachineState> = _state.asStateFlow()

	private val _control = MutableStateFlow<ControlInfo?>(null)
	val control: StateFlow<ControlInfo?> = _control.asStateFlow()

	private val _permissions = MutableStateFlow(Permissions.before(store.get(machineId)?.role))

	/** What this phone may do here: as the control panel says once read, as its last known role allows before. */
	val permissions: StateFlow<Permissions> = _permissions.asStateFlow()

	private val _controlError = MutableStateFlow<Message?>(null)

	/** Why the control panel couldn't be read, until it can. */
	val controlError: StateFlow<Message?> = _controlError.asStateFlow()

	private val _actions = MutableStateFlow(ActionsState())
	val actions: StateFlow<ActionsState> = _actions.asStateFlow()

	private val _activity = MutableStateFlow(ActivityState())

	/** What phones asked the computer to do, newest first, once the activity page has asked for it. */
	val activity: StateFlow<ActivityState> = _activity.asStateFlow()

	private val _compatDismissed = MutableStateFlow(false)

	/** The banner about versions, closed for as long as this screen lives. */
	val compatDismissed: StateFlow<Boolean> = _compatDismissed.asStateFlow()

	// Kept until the screen collects them, even across a rotation.
	private val _messages = Channel<Message>(capacity = Limits.MESSAGES_KEPT, onBufferOverflow = BufferOverflow.DROP_OLDEST)

	/** One-line results and errors for a snackbar. */
	val messages: Flow<Message> = _messages.receiveAsFlow()

	private var procRequest: ProcRequest? = null
	private var wantMedia = false
	private val wanted = MutableStateFlow(params())
	private var current = wanted.value

	private var job: Job? = null
	private var stopping: Job? = null
	private var visible = false

	/** Reads the control panel again now and then while the screen is shown. */
	private var controlTicker: Job? = null
	private var controlReading: Job? = null

	/** When a sample last made the control panel be read again (elapsed realtime). */
	private var lastRoleHint: Long? = null

	/** The address the stream last worked on: a new set of parameters goes straight there. */
	private var workingHost: String? = null
	private var attempt = 0

	init {
		// A new update interval takes a new stream, on the same address.
		viewModelScope.launch { settings.state.collect { wanted.value = params() } }
		// Paired again while this screen stays open (the pairing screen was left before the computer
		// answered): the stream starts over with the new token instead of staying turned away.
		viewModelScope.launch {
			store.machines
				.map { list -> list.find { it.id == machineId }?.let { Triple(it.token, it.tokenSealed, it.fingerprint) } }
				.distinctUntilChanged()
				.drop(1)
				.collect { if (it != null) credentialsChanged() }
		}
	}

	/** Shown, the stream starts over now; not shown, it does when the screen is back. */
	private fun credentialsChanged() {
		job?.cancel()
		job = null
		// A new pairing may come with another role; the panel is read again once the stream is live.
		_control.value = null
		_permissions.value = Permissions.before(store.get(machineId)?.role)
		if (visible) start()
	}

	private fun params() = StreamParams(
		intervalMs = settings.intervalMs,
		procs = procRequest?.count ?: 0,
		sort = procRequest?.sort ?: "cpu",
		media = wantMedia,
	)

	/**
	 * The screen is shown: stream while it is. When it stops, the stream stays for a few seconds,
	 * so turning the phone doesn't cost a reconnection; then it ends until the screen is back.
	 */
	fun setVisible(v: Boolean) {
		visible = v
		if (v) {
			stopping?.cancel()
			stopping = null
			if (job?.isActive != true) start()
			if (controlTicker?.isActive != true) controlTicker = viewModelScope.launch { rereadControl() }
		} else {
			controlTicker?.cancel()
			controlTicker = null
			if (job?.isActive == true && stopping == null) {
				stopping = viewModelScope.launch {
					delay(Limits.STOP_GRACE_MS)
					job?.cancel()
					job = null
					stopping = null
					// What's on screen when it comes back is old until the stream is.
					_state.update { if (it.conn == Conn.Live) it.copy(conn = Conn.Reconnecting) else it }
				}
			}
		}
	}

	/** Reads the control panel every so often until cancelled: a role or control changed on the computer shows without reopening. */
	private suspend fun rereadControl(): Nothing {
		while (true) {
			delay(Limits.CONTROL_REFRESH_MS)
			refreshControl()
		}
	}

	/** Null when no list of processes is on screen, so the service doesn't collect it. */
	fun setProcesses(request: ProcRequest?) {
		if (procRequest == request) return
		procRequest = request
		if (request == null) _state.update { it.copy(processes = null) }
		wanted.value = params()
	}

	/** Media players ride the stream only while the control panel shows them. */
	fun setMedia(want: Boolean) {
		if (wantMedia == want) return
		wantMedia = want
		wanted.value = params()
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
		_state.update { if (it.conn == Conn.Live) it else it.copy(conn = if (it.sample == null) Conn.Connecting else Conn.Reconnecting, refused = null, keyLost = false) }
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
			val host = workingHost ?: run {
				_state.update { it.copy(conn = if (it.sample == null) Conn.Connecting else Conn.Reconnecting) }
				val reached = try {
					reachPaired(machine) { fp -> renewed(fp) }
				} catch (e: ElamondException) {
					_state.update { it.copy(conn = Conn.CertificateChanged) }
					return
				}
				if (reached == null) null else reached.first.host.also {
					workingHost = it
					if (it != machine.lastHost) store.update(machineId) { s -> s.copy(lastHost = it) }
				}
			}
			if (host == null) {
				_state.update { it.copy(conn = Conn.Offline) }
				delay(Limits.backoffMs(attempt++))
				continue
			}
			// Imported from another phone: the computer answers, so it can give this phone a key of its own.
			val pin = (store.get(machineId) ?: machine).fingerprint
			val own = ownKeys.renew(machineId, deviceName(getApplication())) { ElamondClient(host, machine.port, pin, token).rekey(it) }
			// The stream opens with the new key, so revoking the old phone doesn't end it.
			if (own != null) continue

			val e = streamOn(host, ElamondClient(host, machine.port, machine.fingerprint, token))
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
			// Lost, or another certificate (one the computer may have renewed, which the next look follows):
			// look for the computer on every address again.
			workingHost = null
			_state.update { it.copy(conn = Conn.Reconnecting) }
			delay(Limits.backoffMs(attempt++))
		}
	}

	/**
	 * Streams from [host] until the connection fails, and returns why. Changed parameters reopen
	 * the stream there at once, without asking every address and without leaving Live.
	 */
	private suspend fun streamOn(host: String, client: ElamondClient): ElamondException = try {
		// A change undone within the settling time (a tab left and come back to) reopens nothing.
		wanted.settle(Limits.PARAMS_SETTLE_MS).distinctUntilChanged().collectLatest { p ->
			current = p
			hist.stepHintMs = Intervals.effective(p.intervalMs, _state.value.info?.intervalMs).toLong()
			// History on every connection: it fills whatever the app missed meanwhile.
			client.stream(p.intervalMs, p.procs, p.sort, history = true, media = p.media).collect { msg ->
				attempt = 0
				handle(msg, host)
			}
			// Replaced by newer parameters, this block is cancelled: that is no reason to look for the computer again.
			currentCoroutineContext().ensureActive()
			throw ElamondException(ElamondException.Kind.Unreachable, "closed")
		}
		ElamondException(ElamondException.Kind.Unreachable, "closed")
	} catch (e: CancellationException) {
		throw e
	} catch (e: ElamondException) {
		e
	} catch (e: Exception) {
		ElamondException(ElamondException.Kind.Protocol, e.message, e)
	}

	private fun handle(msg: StreamMessage, host: String) {
		when (msg) {
			is Info -> {
				hist.windowMs = msg.historySeconds * 1000L
				hist.stepHintMs = Intervals.effective(current.intervalMs, msg.intervalMs).toLong()
				settings.noteElamondInterval(msg.intervalMs)
				_state.update { it.copy(info = msg, histVersion = hist.version) }
				// Every reopened stream starts with info; the store is written only when it says something new.
				val saved = store.get(machineId)
				if (saved != null && (msg.name.isNotEmpty() || msg.os.isNotEmpty())) {
					val name = msg.name.ifEmpty { saved.name }
					if (name != saved.name || msg.os != saved.os) store.update(machineId) { it.copy(name = name, os = msg.os) }
				}
			}
			is History -> {
				hist.replace(msg)
				_state.update { it.copy(histVersion = hist.version) }
			}
			is Sample -> {
				hist.append(msg)
				val wasLive = _state.value.conn == Conn.Live
				_state.update {
					it.copy(
						conn = Conn.Live,
						host = host,
						sample = msg.copy(processes = null),
						histVersion = hist.version,
						processes = if (procRequest != null) msg.processes ?: it.processes else null,
						processCount = msg.processCount ?: it.processCount,
						receivedAt = System.currentTimeMillis(),
						refused = null,
					)
				}
				// Only now is there an address known to work for everything else.
				if (!wasLive) refreshControl() else if (_permissions.value.outgrownBy(msg)) roleWentUp()
			}
		}
	}

	/** This phone's id in the computer's list of paired phones, for the commands that change its role; unknown for older pairings. */
	val clientId: String? get() = store.get(machineId)?.clientId

	fun dismissCompat() {
		_compatDismissed.value = true
	}

	// ---- control --------------------------------------------------------------------

	/** A client for one-off requests, on the address the stream found. */
	private fun client(): ElamondClient? {
		val machine = store.get(machineId) ?: return null
		val host = _state.value.host ?: workingHost ?: return null
		return ElamondClient(host, machine.port, machine.fingerprint, store.token(machine) ?: return null)
	}

	/**
	 * Reads the control panel, and with it what this phone may do; before the stream is live it waits
	 * for the first sample, which calls it. A newer reading replaces one still on its way.
	 */
	fun refreshControl() {
		if (_state.value.conn != Conn.Live) return
		controlReading?.cancel()
		controlReading = viewModelScope.launch {
			val c = client() ?: return@launch
			try {
				val info = c.get<ControlInfo>("/api/v1/control")
				_control.value = info
				_permissions.value = Permissions.of(info)
				_controlError.value = null
				// Kept, so the screen opens with the right tabs next time.
				if (info.role != null && store.get(machineId)?.role != info.role) store.update(machineId) { it.copy(role = info.role) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				_controlError.value = messageFor(e)
			}
		}
	}

	/** A sample held what this phone wasn't known to see: its role went up, so read what it may do now, now and then at most. */
	private fun roleWentUp() {
		val now = SystemClock.elapsedRealtime()
		val last = lastRoleHint
		if (last != null && now - last < Limits.ROLE_HINT_MIN_MS) return
		lastRoleHint = now
		refreshControl()
	}

	/** Refused for this phone's role, which changed on the computer since the panel was read: read it again. */
	private fun noteRefusal(e: Throwable) {
		if ((e as? ElamondException)?.notAllowed == true) refreshControl()
	}

	/**
	 * The computer renewed its certificate (`elamond rotate-cert`) and the steps from the one pinned
	 * check out: pinned from now on, which starts the stream over with it, and said once.
	 */
	private fun renewed(fingerprint: String) {
		store.update(machineId) { it.copy(fingerprint = fingerprint) }
		say(Message.Res(R.string.machine_cert_renewed))
	}

	private fun say(m: Message) {
		_messages.trySend(m)
	}

	/** Runs a control request; [done] gets told on success. */
	private fun act(@StringRes done: Int? = null, refresh: Boolean = false, block: suspend (ElamondClient) -> Unit) {
		viewModelScope.launch {
			val c = client() ?: return@launch say(Message.Res(R.string.err_unreachable))
			try {
				block(c)
				done?.let { say(Message.Res(it)) }
				if (refresh) refreshControl()
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				say(messageFor(e))
				noteRefusal(e)
				if ((e as? ElamondException)?.code == "process_changed") refreshProcesses()
			}
		}
	}

	@Serializable private data class PowerReq(val action: String)
	@Serializable private data class ProfileReq(val profile: String)
	@Serializable private data class SignalReq(val pid: Int, val signal: String, val start: Long? = null)
	@Serializable private data class ActionReq(val id: String)
	@Serializable private data class MediaReq(val player: String, val op: String, val value: Double? = null)
	@Serializable private data class VolumeReq(val level: Float? = null, val muted: Boolean? = null)
	@Serializable private data class UnitReq(val op: String)
	@Serializable private data class Ok(val ok: Boolean = true)

	fun power(action: String) = act(
		done = when (action) {
			"lock" -> R.string.done_locked
			"poweroff" -> R.string.done_shutting_down
			"reboot" -> R.string.done_restarting
			"suspend" -> R.string.done_going_to_sleep
			else -> null
		},
	) { it.post<PowerReq, Ok>("/api/v1/control/power", PowerReq(action)) }

	fun setProfile(profile: String) = act(refresh = true) {
		it.post<ProfileReq, Ok>("/api/v1/control/profile", ProfileReq(profile))
	}

	/**
	 * Sends [signal] to the process [pid] that started at [start] (Unix s, as samples give it): the
	 * service refuses it, process_changed, when the pid has gone to another process since. Without a
	 * start (0) whatever has the pid gets it.
	 */
	fun signal(pid: Int, start: Long, signal: String) = act(done = if (signal == "kill") R.string.done_killed else R.string.done_asked_to_quit) {
		it.post<SignalReq, Ok>("/api/v1/control/signal", SignalReq(pid, signal, start.takeIf { s -> s > 0 }))
	}

	/** The list of processes on screen read at once, rather than at the next sample, which may be a while. */
	private fun refreshProcesses() {
		val r = procRequest ?: return
		viewModelScope.launch {
			val s = client()?.let { c -> orNull { c.get<Sample>("/api/v1/snapshot?procs=${r.count}&sort=${r.sort}") } } ?: return@launch
			val procs = s.processes ?: return@launch
			// Only if the same list is still wanted.
			if (procRequest == r) _state.update { it.copy(processes = procs, processCount = s.processCount ?: it.processCount) }
		}
	}

	/** Runs an action; its output opens when it's done, here or whenever the panel is next shown. */
	fun runAction(id: String) {
		if (_actions.value.running != null) return
		_actions.update { it.copy(running = id) }
		viewModelScope.launch {
			try {
				val c = client() ?: throw ElamondException(ElamondException.Kind.Unreachable)
				// The service kills an action whose request goes away: leaving the screen lets it finish.
				val r = c.post<ActionReq, ActionResult>("/api/v1/control/action", ActionReq(id), timeoutMs = Limits.ACTION_TIMEOUT_MS, finishOnCancel = true)
				_actions.update { it.copy(results = it.results + (id to r), shown = id) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				say(messageFor(e))
				noteRefusal(e)
				// Ids come from what an action runs: one changed on the computer has a new id.
				if ((e as? ElamondException)?.code == "no_such_action") refreshControl()
			} finally {
				_actions.update { it.copy(running = null) }
			}
		}
	}

	/** Reads the activity log again; what was read before stays on screen meanwhile. */
	fun refreshActivity() {
		if (_activity.value.loading) return
		_activity.update { it.copy(loading = true) }
		viewModelScope.launch {
			try {
				val c = client() ?: throw ElamondException(ElamondException.Kind.Unreachable)
				val entries = c.get<List<LogEntry>>("/api/v1/control/log?limit=${Limits.ACTIVITY_SHOWN}")
				_activity.update { it.copy(entries = entries, error = null) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				_activity.update { it.copy(error = messageFor(e)) }
				noteRefusal(e)
			} finally {
				_activity.update { it.copy(loading = false) }
			}
		}
	}

	/** Shows the output an action printed last, or with null closes it. */
	fun showActionResult(id: String?) {
		_actions.update { it.copy(shown = id) }
	}

	fun media(player: String, op: String, value: Double? = null) = act {
		it.post<MediaReq, Ok>("/api/v1/media", MediaReq(player, op, value))
	}

	fun volume(level: Float? = null, muted: Boolean? = null) = act {
		it.post<VolumeReq, Ok>("/api/v1/media/volume", VolumeReq(level, muted))
	}

	/** A player's cover, decoded off the main thread at about [sizePx] square; null when there's none to show. */
	suspend fun art(player: String, sizePx: Int): Bitmap? {
		val c = client() ?: return null
		val bytes = try {
			c.bytes("/api/v1/media/art?player=" + java.net.URLEncoder.encode(player, "UTF-8"))
		} catch (e: CancellationException) {
			throw e
		} catch (e: ElamondException) {
			// Covers needed control before 0.5, and need the role since: either changed since the panel was read.
			if (e.code == "control_disabled" || e.notAllowed) refreshControl()
			return null
		} catch (e: Exception) {
			return null
		}
		return withContext(Dispatchers.Default) { orNull { decodeArt(bytes, sizePx) } }
	}

	fun saveLink(def: LinkDef) = act(done = R.string.done_saved) {
		it.onLinks { path -> post<LinkDef, LinkDef>(path, def) }
	}

	fun deleteLink(id: String) = act(done = R.string.done_removed) { it.onLinks("/$id") { path -> delete(path) } }

	fun unit(linkId: String, op: String) = act(
		done = when (op) {
			"start" -> R.string.done_started
			"stop" -> R.string.done_stopped
			else -> R.string.done_restarted
		},
	) { it.onLinks("/$linkId/unit") { path -> post<UnitReq, Ok>(path, UnitReq(op)) } }

	suspend fun detected(): List<Listener> = quietly { it.onLinks("/detected") { path -> get<List<Listener>>(path) } }.orEmpty()

	suspend fun units(scope: String): List<UnitInfo> = quietly { it.get<List<UnitInfo>>("/api/v1/units?scope=$scope") }.orEmpty()

	/** A suggestion read without a word on failure; null then. A refusal for the role still reads the panel again. */
	private suspend fun <T> quietly(block: suspend (ElamondClient) -> T): T? {
		val c = client() ?: return null
		return try {
			block(c)
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			noteRefusal(e)
			null
		}
	}

	class Factory(private val app: Application, private val id: String) : ViewModelProvider.Factory {
		@Suppress("UNCHECKED_CAST")
		override fun <T : ViewModel> create(modelClass: Class<T>): T = MachineViewModel(app, id) as T
	}
}
