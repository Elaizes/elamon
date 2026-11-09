package ua.elamon

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import ua.elamon.data.ElamondClient
import ua.elamon.data.ElamondException
import ua.elamon.data.History
import ua.elamon.data.Info
import ua.elamon.data.Proc
import ua.elamon.data.Sample
import ua.elamon.data.ServerStore

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
)

/** What the process list needs from the service. */
data class ProcRequest(val sort: String, val all: Boolean)

class MachineViewModel(app: Application, private val serverId: String) : AndroidViewModel(app) {
	private val store = ServerStore.get(app)
	private val settings = Settings.get(app)
	private val _state = MutableStateFlow(MachineState())
	val state: StateFlow<MachineState> = _state.asStateFlow()

	private var job: Job? = null
	private var visible = false
	private var procRequest: ProcRequest? = null

	init {
		// A new update interval takes a new stream.
		viewModelScope.launch { settings.intervalFlow.drop(1).collect { restart() } }
	}

	/** The screen is shown: stream while it is, stop when the phone locks or the app goes away. */
	fun setVisible(v: Boolean) {
		if (visible == v) return
		visible = v
		restart()
	}

	/** Null when the process list isn't on screen, so the service doesn't collect it. */
	fun setProcesses(request: ProcRequest?) {
		if (procRequest == request) return
		procRequest = request
		if (request == null) _state.update { it.copy(processes = null) }
		// The service picks the heaviest processes by the chosen order, so a new order needs a new stream.
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
			_state.update {
				it.copy(conn = if (it.sample == null) Conn.Connecting else Conn.Reconnecting)
			}
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

	private fun handle(msg: ua.elamon.data.StreamMessage, host: String) {
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
				)
			}
		}
	}

	private fun backoff(attempt: Int): Long = (1000L shl attempt.coerceAtMost(3)).coerceAtMost(8000L)

	class Factory(private val app: Application, private val id: String) : ViewModelProvider.Factory {
		@Suppress("UNCHECKED_CAST")
		override fun <T : ViewModel> create(modelClass: Class<T>): T = MachineViewModel(app, id) as T
	}

	companion object {
		/** Ten minutes at one sample a second. */
		const val HISTORY_CAP = 600
	}
}
