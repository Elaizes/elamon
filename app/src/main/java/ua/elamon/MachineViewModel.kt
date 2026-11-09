package ua.elamon

import android.app.Application
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import ua.elamon.data.ActionResult
import ua.elamon.data.ElamondClient
import ua.elamon.data.ElamondException
import ua.elamon.data.ControlInfo
import ua.elamon.data.History
import ua.elamon.data.Info
import ua.elamon.data.Listener
import ua.elamon.data.Proc
import ua.elamon.data.Sample
import ua.elamon.data.ServerStore
import ua.elamon.data.ServiceDef
import ua.elamon.data.StreamMessage
import ua.elamon.data.UnitInfo

enum class Conn { Connecting, Live, Reconnecting, Offline, Unauthorized, CertificateChanged }

/** The main series for charts, oldest first; NaN where a value was missing. */
data class Hist(
	val cpu: List<Float> = emptyList(),
	val cpuTemp: List<Float> = emptyList(),
	val mem: List<Float> = emptyList(),
	val swap: List<Float> = emptyList(),
	val rx: List<Float> = emptyList(),
	val tx: List<Float> = emptyList(),
	val read: List<Float> = emptyList(),
	val write: List<Float> = emptyList(),
	val batteryPower: List<Float> = emptyList(),
	val gpuUsage: Map<String, List<Float>> = emptyMap(),
	val gpuTemp: Map<String, List<Float>> = emptyMap(),
) {
	val size get() = cpu.size

	fun append(s: Sample, cap: Int): Hist {
		fun <T> List<T>.add(v: T) = (if (size >= cap) drop(size - cap + 1) else this) + v
		val realNet = s.net.filter { it.kind == "wired" || it.kind == "wireless" }
		val n = size
		fun gpuSeries(old: Map<String, List<Float>>, pick: (ua.elamon.data.Gpu) -> Float?): Map<String, List<Float>> {
			val ids = old.keys + s.gpus.map { it.id }
			return ids.associateWith { id ->
				val prev = old[id] ?: List(n) { Float.NaN }
				prev.add(s.gpus.find { it.id == id }?.let(pick) ?: Float.NaN)
			}
		}
		return Hist(
			cpu = cpu.add(s.cpu.usage),
			cpuTemp = cpuTemp.add(s.cpu.temp ?: Float.NaN),
			mem = mem.add(s.memory.used.toFloat()),
			swap = swap.add(s.memory.swapUsed.toFloat()),
			rx = rx.add(realNet.sumOf { it.rx }.toFloat()),
			tx = tx.add(realNet.sumOf { it.tx }.toFloat()),
			read = read.add(s.diskIo.sumOf { it.read }.toFloat()),
			write = write.add(s.diskIo.sumOf { it.write }.toFloat()),
			batteryPower = batteryPower.add(s.battery?.power ?: Float.NaN),
			gpuUsage = gpuSeries(gpuUsage) { it.usage },
			gpuTemp = gpuSeries(gpuTemp) { it.temp },
		)
	}

	companion object {
		fun from(h: History, cap: Int): Hist {
			fun List<Float?>.f() = map { it ?: Float.NaN }.takeLast(cap)
			return Hist(
				cpu = h.cpu.takeLast(cap),
				cpuTemp = h.cpuTemp.f(),
				mem = h.mem.map { it.toFloat() }.takeLast(cap),
				swap = h.swap.map { it.toFloat() }.takeLast(cap),
				rx = h.rx.map { it.toFloat() }.takeLast(cap),
				tx = h.tx.map { it.toFloat() }.takeLast(cap),
				read = h.read.map { it.toFloat() }.takeLast(cap),
				write = h.write.map { it.toFloat() }.takeLast(cap),
				batteryPower = h.batteryPower.f(),
				gpuUsage = h.gpus.associate { it.id to it.usage.f() },
				gpuTemp = h.gpus.associate { it.id to it.temp.f() },
			)
		}
	}
}

data class MachineState(
	val conn: Conn = Conn.Connecting,
	val host: String? = null,
	val info: Info? = null,
	val sample: Sample? = null,
	val hist: Hist = Hist(),
	val processes: List<Proc>? = null,
	val processCount: Int = 0,
	/** When the last sample arrived, for extrapolating media positions. */
	val receivedAt: Long = 0,
)

/** What the process list needs from the service. */
data class ProcRequest(val sort: String, val all: Boolean)

class MachineViewModel(app: Application, private val serverId: String) : AndroidViewModel(app) {
	private val store = ServerStore.get(app)
	private val settings = Settings.get(app)
	private val _state = MutableStateFlow(MachineState())
	val state: StateFlow<MachineState> = _state.asStateFlow()

	private val _control = MutableStateFlow<ControlInfo?>(null)
	val control: StateFlow<ControlInfo?> = _control.asStateFlow()

	private val _messages = MutableSharedFlow<String>(extraBufferCapacity = 4, onBufferOverflow = BufferOverflow.DROP_OLDEST)
	/** One-line results and errors for a snackbar. */
	val messages: SharedFlow<String> = _messages.asSharedFlow()

	private var job: Job? = null
	private var visible = false
	private var procRequest: ProcRequest? = null
	private var wantMedia = false

	init {
		// A new update interval takes a new stream.
		viewModelScope.launch { settings.state.map { it.intervalMs }.distinctUntilChanged().drop(1).collect { restart() } }
	}

	/** The screen is shown: stream while it is, stop when the phone locks or the app goes away. */
	fun setVisible(v: Boolean) {
		if (visible == v) return
		visible = v
		restart()
	}

	/** Null when no list of processes is on screen, so the service doesn't collect it. */
	fun setProcesses(request: ProcRequest?) {
		if (procRequest == request) return
		procRequest = request
		if (request == null) _state.update { it.copy(processes = null) }
		// The service picks the heaviest processes by the chosen order, so a new order needs a new stream.
		restart()
	}

	/** Media players ride the stream only while the control panel shows them. */
	fun setMedia(want: Boolean) {
		if (wantMedia == want) return
		wantMedia = want
		restart()
	}

	fun retry() = restart()

	private fun restart() {
		job?.cancel()
		job = if (visible) viewModelScope.launch { run() } else null
	}

	private suspend fun run() {
		var attempt = 0
		while (true) {
			val server = store.get(serverId) ?: return
			val current = _state.value
			_state.update { it.copy(conn = if (it.sample == null) Conn.Connecting else Conn.Reconnecting) }
			val reached = try {
				ElamondClient.firstReachable(server.hostsToTry(), server.port, server.fingerprint, server.id)
			} catch (e: ElamondException) {
				_state.update { it.copy(conn = Conn.CertificateChanged) }
				return
			}
			if (reached == null) {
				_state.update { it.copy(conn = Conn.Offline) }
				delay(backoff(attempt++))
				continue
			}
			val host = reached.first.host
			if (host != server.lastHost) store.update(serverId) { it.copy(lastHost = host) }

			val client = ElamondClient(host, server.port, server.fingerprint, server.token)
			val req = procRequest
			try {
				client.stream(
					intervalMs = settings.intervalMs,
					procs = if (req == null) 0 else if (req.all) 10_000 else 200,
					sort = req?.sort ?: "cpu",
					// History once per screen: after a reconnect, the app's own record carries on.
					history = current.hist.size == 0,
					media = wantMedia,
				).collect { msg ->
					attempt = 0
					handle(msg, host)
				}
			} catch (e: ElamondException) {
				when (e.kind) {
					ElamondException.Kind.Unauthorized -> {
						_state.update { it.copy(conn = Conn.Unauthorized) }
						return
					}
					ElamondException.Kind.CertificateChanged -> {
						_state.update { it.copy(conn = Conn.CertificateChanged) }
						return
					}
					else -> {}
				}
			}
			_state.update { it.copy(conn = Conn.Reconnecting) }
			delay(backoff(attempt++))
		}
	}

	private fun handle(msg: StreamMessage, host: String) {
		when (msg) {
			is Info -> {
				_state.update { it.copy(info = msg) }
				if (msg.name.isNotEmpty() || msg.os.isNotEmpty()) {
					store.update(serverId) { it.copy(name = msg.name.ifEmpty { it.name }, os = msg.os) }
				}
			}
			is History -> _state.update { it.copy(hist = Hist.from(msg, HISTORY_CAP)) }
			is Sample -> _state.update {
				it.copy(
					conn = Conn.Live,
					host = host,
					sample = msg.copy(processes = null),
					hist = it.hist.append(msg, HISTORY_CAP),
					processes = if (procRequest != null) msg.processes ?: it.processes else null,
					processCount = msg.processCount ?: it.processCount,
					receivedAt = System.currentTimeMillis(),
				)
			}
		}
	}

	private fun backoff(attempt: Int): Long = (1000L shl attempt.coerceAtMost(3)).coerceAtMost(8000L)

	// ---- control --------------------------------------------------------------------

	/** A client for one-off requests, on the address the stream found. */
	private fun client(): ElamondClient? {
		val server = store.get(serverId) ?: return null
		val host = _state.value.host ?: server.lastHost ?: server.hosts.firstOrNull() ?: return null
		return ElamondClient(host, server.port, server.fingerprint, server.token)
	}

	fun refreshControl() {
		viewModelScope.launch {
			val c = client() ?: return@launch
			runCatching { c.get<ControlInfo>("/api/v1/control") }.onSuccess { _control.value = it }
		}
	}

	private fun say(e: Throwable) {
		_messages.tryEmit(
			when ((e as? ElamondException)?.kind) {
				ElamondException.Kind.ControlDisabled -> "Control is off on this computer"
				ElamondException.Kind.NotPermitted -> "The computer didn't allow that"
				ElamondException.Kind.NotFound -> "It's gone"
				ElamondException.Kind.Failed -> e.message ?: "It didn't work"
				ElamondException.Kind.Unreachable -> "Can't reach the computer"
				else -> e.message ?: "Something went wrong"
			}
		)
	}

	/** Runs a control request; [done] gets told on success. */
	private fun act(done: String? = null, refresh: Boolean = false, block: suspend (ElamondClient) -> Unit) {
		viewModelScope.launch {
			val c = client() ?: return@launch
			try {
				block(c)
				done?.let { _messages.tryEmit(it) }
				if (refresh) refreshControl()
			} catch (e: Exception) {
				say(e)
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
			"lock" -> "Locked"
			"poweroff" -> "Shutting down"
			"reboot" -> "Restarting"
			"suspend" -> "Going to sleep"
			else -> null
		},
	) { it.post<PowerReq, Ok>("/api/v1/control/power", PowerReq(action)) }

	fun setProfile(profile: String) = act(refresh = true) {
		it.post<ProfileReq, Ok>("/api/v1/control/profile", ProfileReq(profile))
	}

	fun signal(pid: Int, signal: String) = act(done = if (signal == "kill") "Killed" else "Asked to quit") {
		it.post<SignalReq, Ok>("/api/v1/control/signal", SignalReq(pid, signal))
	}

	/** Runs an action and hands back what it printed. */
	fun runAction(id: String, onResult: (ActionResult) -> Unit) {
		viewModelScope.launch {
			val c = client() ?: return@launch
			try {
				onResult(c.post<ActionReq, ActionResult>("/api/v1/control/action", ActionReq(id), timeoutMs = 3_700_000))
			} catch (e: Exception) {
				say(e)
			}
		}
	}

	fun media(player: String, op: String, value: Double? = null) = act {
		it.post<MediaReq, Ok>("/api/v1/media", MediaReq(player, op, value))
	}

	fun volume(level: Float? = null, muted: Boolean? = null) = act {
		it.post<VolumeReq, Ok>("/api/v1/media/volume", VolumeReq(level, muted))
	}

	suspend fun art(player: String): Bitmap? {
		val c = client() ?: return null
		return runCatching {
			val bytes = c.bytes("/api/v1/media/art?player=" + java.net.URLEncoder.encode(player, "UTF-8"))
			BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
		}.getOrNull()
	}

	fun saveService(def: ServiceDef) = act(done = "Saved") {
		it.post<ServiceDef, ServiceDef>("/api/v1/services", def)
	}

	fun deleteService(id: String) = act(done = "Removed") { it.delete("/api/v1/services/$id") }

	fun unit(serviceId: String, op: String) = act(
		done = when (op) {
			"start" -> "Started"
			"stop" -> "Stopped"
			else -> "Restarted"
		},
	) { it.post<UnitReq, Ok>("/api/v1/services/$serviceId/unit", UnitReq(op)) }

	suspend fun detected(): List<Listener> = client()?.let { runCatching { it.get<List<Listener>>("/api/v1/services/detected") }.getOrNull() }.orEmpty()

	suspend fun units(scope: String): List<UnitInfo> =
		client()?.let { runCatching { it.get<List<UnitInfo>>("/api/v1/units?scope=$scope") }.getOrNull() }.orEmpty()

	class Factory(private val app: Application, private val id: String) : ViewModelProvider.Factory {
		@Suppress("UNCHECKED_CAST")
		override fun <T : ViewModel> create(modelClass: Class<T>): T = MachineViewModel(app, id) as T
	}

	companion object {
		/** Ten minutes at one sample a second. */
		const val HISTORY_CAP = 600
	}
}
