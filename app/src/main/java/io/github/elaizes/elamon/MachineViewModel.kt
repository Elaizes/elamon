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
import io.github.elaizes.elamon.data.Proc
import io.github.elaizes.elamon.data.Sample
import io.github.elaizes.elamon.data.ServerStore
import io.github.elaizes.elamon.data.ServiceDef
import io.github.elaizes.elamon.data.StreamMessage
import io.github.elaizes.elamon.data.UnitInfo

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

class MachineViewModel(app: Application, private val serverId: String) : AndroidViewModel(app) {
	private val store = ServerStore.get(app)
	private val settings = Settings.get(app)
	private val hist = Hist()
	private val _state = MutableStateFlow(MachineState(hist = hist))
	val state: StateFlow<MachineState> = _state.asStateFlow()

	private val _control = MutableStateFlow<ControlInfo?>(null)
	val control: StateFlow<ControlInfo?> = _control.asStateFlow()

	private val _controlError = MutableStateFlow<Message?>(null)

	/** Why the control panel couldn't be read, until it can. */
	val controlError: StateFlow<Message?> = _controlError.asStateFlow()

	private val _actions = MutableStateFlow(ActionsState())
	val actions: StateFlow<ActionsState> = _actions.asStateFlow()

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

	/** The address the stream last worked on: a new set of parameters goes straight there. */
	private var workingHost: String? = null
	private var attempt = 0

	init {
		// A new update interval takes a new stream, on the same address.
		viewModelScope.launch { settings.state.collect { wanted.value = params() } }
		// Paired again while this screen stays open (the pairing screen was left before the computer
		// answered): the stream starts over with the new token instead of staying turned away.
		viewModelScope.launch {
			store.servers
				.map { list -> list.find { it.id == serverId }?.let { it.token to it.fingerprint } }
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
		} else if (job?.isActive == true && stopping == null) {
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
		_state.update { if (it.conn == Conn.Live) it else it.copy(conn = if (it.sample == null) Conn.Connecting else Conn.Reconnecting, refused = null) }
		job = viewModelScope.launch { loop() }
	}

	private suspend fun loop() {
		while (true) {
			val server = store.get(serverId) ?: return
			val host = workingHost ?: run {
				_state.update { it.copy(conn = if (it.sample == null) Conn.Connecting else Conn.Reconnecting) }
				val reached = try {
					ElamondClient.firstReachable(server.hostsToTry(), server.port, server.fingerprint, server.id)
				} catch (e: ElamondException) {
					_state.update { it.copy(conn = Conn.CertificateChanged) }
					return
				}
				if (reached == null) null else reached.first.host.also {
					workingHost = it
					if (it != server.lastHost) store.update(serverId) { s -> s.copy(lastHost = it) }
				}
			}
			if (host == null) {
				_state.update { it.copy(conn = Conn.Offline) }
				delay(Limits.backoffMs(attempt++))
				continue
			}

			val e = streamOn(host, ElamondClient(host, server.port, server.fingerprint, server.token))
			when {
				e.kind == ElamondException.Kind.Unauthorized -> {
					_state.update { it.copy(conn = Conn.Unauthorized) }
					return
				}
				e.kind == ElamondException.Kind.CertificateChanged -> {
					_state.update { it.copy(conn = Conn.CertificateChanged) }
					return
				}
				// Asking again won't change the answer: say why and wait for the user.
				e.status == 403 -> {
					_state.update { it.copy(conn = Conn.Refused, refused = messageFor(e)) }
					return
				}
			}
			// Lost: look for the computer on every address again.
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
				settings.noteServiceInterval(msg.intervalMs)
				_state.update { it.copy(info = msg, histVersion = hist.version) }
				// Every reopened stream starts with info; the store is written only when it says something new.
				val saved = store.get(serverId)
				if (saved != null && (msg.name.isNotEmpty() || msg.os.isNotEmpty())) {
					val name = msg.name.ifEmpty { saved.name }
					if (name != saved.name || msg.os != saved.os) store.update(serverId) { it.copy(name = name, os = msg.os) }
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
				if (!wasLive) refreshControl()
			}
		}
	}

	fun dismissCompat() {
		_compatDismissed.value = true
	}

	// ---- control --------------------------------------------------------------------

	/** A client for one-off requests, on the address the stream found. */
	private fun client(): ElamondClient? {
		val server = store.get(serverId) ?: return null
		val host = _state.value.host ?: workingHost ?: return null
		return ElamondClient(host, server.port, server.fingerprint, server.token)
	}

	/** Reads the control panel; before the stream is live it waits for the first sample, which calls it. */
	fun refreshControl() {
		if (_state.value.conn != Conn.Live) return
		viewModelScope.launch {
			val c = client() ?: return@launch
			try {
				_control.value = c.get<ControlInfo>("/api/v1/control")
				_controlError.value = null
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				_controlError.value = messageFor(e)
			}
		}
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
			}
		}
	}

	@Serializable private data class PowerReq(val action: String)
	@Serializable private data class ProfileReq(val profile: String)
	@Serializable private data class SignalReq(val pid: Int, val signal: String)
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

	fun signal(pid: Int, signal: String) = act(done = if (signal == "kill") R.string.done_killed else R.string.done_asked_to_quit) {
		it.post<SignalReq, Ok>("/api/v1/control/signal", SignalReq(pid, signal))
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
				// Ids come from what an action runs: one changed on the computer has a new id.
				if ((e as? ElamondException)?.code == "no_such_action") refreshControl()
			} finally {
				_actions.update { it.copy(running = null) }
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
			// Covers need control; turned off since the panel was read, it should say so instead of the player.
			if (e.code == "control_disabled") refreshControl()
			return null
		} catch (e: Exception) {
			return null
		}
		return withContext(Dispatchers.Default) { orNull { decodeArt(bytes, sizePx) } }
	}

	fun saveService(def: ServiceDef) = act(done = R.string.done_saved) {
		it.post<ServiceDef, ServiceDef>("/api/v1/services", def)
	}

	fun deleteService(id: String) = act(done = R.string.done_removed) { it.delete("/api/v1/services/$id") }

	fun unit(serviceId: String, op: String) = act(
		done = when (op) {
			"start" -> R.string.done_started
			"stop" -> R.string.done_stopped
			else -> R.string.done_restarted
		},
	) { it.post<UnitReq, Ok>("/api/v1/services/$serviceId/unit", UnitReq(op)) }

	suspend fun detected(): List<Listener> = client()?.let { orNull { it.get<List<Listener>>("/api/v1/services/detected") } }.orEmpty()

	suspend fun units(scope: String): List<UnitInfo> =
		client()?.let { orNull { it.get<List<UnitInfo>>("/api/v1/units?scope=$scope") } }.orEmpty()

	class Factory(private val app: Application, private val id: String) : ViewModelProvider.Factory {
		@Suppress("UNCHECKED_CAST")
		override fun <T : ViewModel> create(modelClass: Class<T>): T = MachineViewModel(app, id) as T
	}
}
