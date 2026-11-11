@file:OptIn(ExperimentalSerializationApi::class)

package io.github.elaizes.elamon.data

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNames
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * What elamond sends; mirrors elamond/src/model.rs. Unknown fields are ignored so newer versions
 * still work. Ids and times have no default: a message without them is broken, not zero. A field
 * renamed in 0.5 is read by its new name or its old one (JsonNames): elamond 0.4 sends only the old.
 */
val ProtocolJson = Json {
	ignoreUnknownKeys = true
	classDiscriminator = "type"
	explicitNulls = false
	coerceInputValues = true
}

/** The API this app speaks; a higher one from the service means the app is out of date. */
const val APP_API = 1

/** What elamond before 0.4 runs at without saying so. */
const val DEFAULT_INTERVAL_MS = 1000
const val DEFAULT_HISTORY_SECONDS = 600

@Serializable
sealed interface StreamMessage

private val STREAM_TYPES = setOf("info", "history", "sample")
private val TYPE_PREFIX = Regex("""^\s*\{\s*"type"\s*:\s*"([^"]*)"""")

/**
 * One line of the stream. Null for a kind of message this app doesn't know, which a newer elamond
 * may send; a known kind that doesn't decode throws, so a broken message isn't read as zeros.
 */
fun decodeStreamLine(line: String, json: Json = ProtocolJson): StreamMessage? {
	// elamond puts the type first; reading it there saves parsing a whole sample twice.
	val type = TYPE_PREFIX.find(line)?.groupValues?.get(1)
		?: json.parseToJsonElement(line).jsonObject["type"]?.jsonPrimitive?.content
	if (type !in STREAM_TYPES) return null
	return json.decodeFromString(StreamMessage.serializer(), line)
}

@Serializable
data class Hello(
	val app: String,
	val api: Int,
	val id: String,
	val name: String,
	val version: String,
	/** The oldest app version the service works with; elamond before 0.4 doesn't say. */
	@SerialName("min_app") val minApp: String? = null,
)

@Serializable
data class PairResponse(
	val token: String,
	@SerialName("client_id") val clientId: String = "",
	val id: String = "",
	val name: String = "",
	/** What the phone may do there, "glance" to "admin"; elamond before 0.5 has no roles and doesn't say. */
	val role: String? = null,
)

@Serializable
@SerialName("info")
data class Info(
	val id: String,
	val name: String = "",
	/** "service_version" before 0.5, and still sent beside it for apps 0.4. */
	@SerialName("service_version") @JsonNames("service_version") val elamondVersion: String,
	val api: Int,
	@SerialName("min_app") val minApp: String? = null,
	/** The service's own tick: no stream comes faster. */
	@SerialName("interval_ms") val intervalMs: Int = DEFAULT_INTERVAL_MS,
	/** How far back history reaches. */
	@SerialName("history_seconds") val historySeconds: Int = DEFAULT_HISTORY_SECONDS,
	val os: String = "",
	val kernel: String = "",
	val arch: String = "",
	@SerialName("cpu_model") val cpuModel: String = "",
	@SerialName("cpu_physical_cores") val cpuPhysicalCores: Int? = null,
	@SerialName("cpu_logical_cores") val cpuLogicalCores: Int,
	@SerialName("memory_total") val memoryTotal: Long,
	@SerialName("boot_time") val bootTime: Long,
	val gpus: List<GpuInfo> = emptyList(),
	val capabilities: List<String> = emptyList(),
) : StreamMessage

@Serializable
data class GpuInfo(
	val id: String,
	val vendor: String = "",
	val name: String = "",
	val driver: String = "",
	@SerialName("boot_display") val bootDisplay: Boolean = false,
)

@Serializable
@SerialName("sample")
data class Sample(
	/** Unix milliseconds on the service's clock. */
	val t: Long,
	val uptime: Long,
	val cpu: Cpu = Cpu(),
	val memory: Memory = Memory(),
	val gpus: List<Gpu> = emptyList(),
	val temps: List<Temp> = emptyList(),
	val fans: List<Fan> = emptyList(),
	val battery: Battery? = null,
	val power: Power = Power(),
	val disks: List<Disk> = emptyList(),
	@SerialName("disk_io") val diskIo: List<DiskIo> = emptyList(),
	val net: List<Net> = emptyList(),
	val displays: List<Display> = emptyList(),
	/** "services" before 0.5, and still sent beside it for apps 0.4. */
	@JsonNames("services") val links: List<LinkStatus> = emptyList(),
	val media: Media? = null,
	@SerialName("process_count") val processCount: Int? = null,
	val processes: List<Proc>? = null,
) : StreamMessage

@Serializable
data class Cpu(
	val usage: Float = 0f,
	val cores: List<Float> = emptyList(),
	val freq: List<Int> = emptyList(),
	val temp: Float? = null,
	val load: List<Float> = emptyList(),
)

@Serializable
data class Memory(
	val total: Long = 0,
	val used: Long = 0,
	val available: Long = 0,
	@SerialName("swap_total") val swapTotal: Long = 0,
	@SerialName("swap_used") val swapUsed: Long = 0,
)

@Serializable
data class Gpu(
	val id: String,
	val state: String = "unknown",
	val usage: Float? = null,
	@SerialName("mem_used") val memUsed: Long? = null,
	@SerialName("mem_total") val memTotal: Long? = null,
	val temp: Float? = null,
	val power: Float? = null,
	val freq: Int? = null,
	@SerialName("freq_max") val freqMax: Int? = null,
	@SerialName("mem_freq") val memFreq: Int? = null,
	val fan: Int? = null,
)

@Serializable
data class Temp(
	val chip: String,
	val kind: String,
	val device: String? = null,
	val label: String,
	val value: Float,
	val high: Float? = null,
	val crit: Float? = null,
)

@Serializable
data class Fan(val chip: String, val label: String, val rpm: Int)

@Serializable
data class Battery(
	val name: String = "",
	val model: String? = null,
	val percent: Float? = null,
	val status: String = "unknown",
	val power: Float? = null,
	@SerialName("energy_now") val energyNow: Float? = null,
	@SerialName("energy_full") val energyFull: Float? = null,
	@SerialName("energy_design") val energyDesign: Float? = null,
	val cycles: Int? = null,
	@SerialName("time_left") val timeLeft: Long? = null,
)

@Serializable
data class Power(
	@SerialName("on_ac") val onAc: Boolean? = null,
	val profile: String? = null,
	val profiles: List<String> = emptyList(),
	val governor: String? = null,
	val epp: String? = null,
	@SerialName("ppd_profile") val ppdProfile: String? = null,
	@SerialName("ppd_profiles") val ppdProfiles: List<String> = emptyList(),
)

@Serializable
data class Disk(
	val device: String,
	val mount: String,
	val fs: String = "",
	val total: Long = 0,
	val available: Long = 0,
	val removable: Boolean = false,
)

@Serializable
data class DiskIo(val name: String, val model: String? = null, val read: Double = 0.0, val write: Double = 0.0)

@Serializable
data class Net(
	val name: String,
	val kind: String,
	val up: Boolean = false,
	val addrs: List<String> = emptyList(),
	val rx: Double = 0.0,
	val tx: Double = 0.0,
	@SerialName("rx_total") val rxTotal: Long = 0,
	@SerialName("tx_total") val txTotal: Long = 0,
	val speed: Long? = null,
)

@Serializable
data class Display(
	val connector: String,
	val gpu: String = "",
	val connected: Boolean = false,
	val active: Boolean = false,
	val dpms: String? = null,
	val internal: Boolean = false,
	val vendor: String? = null,
	val model: String? = null,
	val year: Int? = null,
	@SerialName("width_mm") val widthMm: Int? = null,
	@SerialName("height_mm") val heightMm: Int? = null,
	val mode: Mode? = null,
	val preferred: Mode? = null,
	@SerialName("max_refresh") val maxRefresh: Float? = null,
	val position: List<Int>? = null,
)

@Serializable
data class Mode(val width: Int, val height: Int, val refresh: Float)

@Serializable
data class Proc(
	val pid: Int,
	val ppid: Int? = null,
	val name: String,
	val cmd: String = "",
	val user: String? = null,
	val cpu: Float = 0f,
	val mem: Long = 0,
	val status: String = "",
	val start: Long = 0,
	val read: Double = 0.0,
	val write: Double = 0.0,
	val gpu: Float? = null,
	@SerialName("gpu_mem") val gpuMem: Long? = null,
)

@Serializable
@SerialName("history")
data class History(
	/** Unix milliseconds on the service's clock, one per entry of every other list. */
	val t: List<Long>,
	val cpu: List<Float> = emptyList(),
	@SerialName("cpu_temp") val cpuTemp: List<Float?> = emptyList(),
	val mem: List<Long> = emptyList(),
	val swap: List<Long> = emptyList(),
	val rx: List<Double> = emptyList(),
	val tx: List<Double> = emptyList(),
	val read: List<Double> = emptyList(),
	val write: List<Double> = emptyList(),
	@SerialName("battery_power") val batteryPower: List<Float?> = emptyList(),
	val gpus: List<GpuHistory> = emptyList(),
) : StreamMessage

@Serializable
data class GpuHistory(
	val id: String,
	val usage: List<Float?> = emptyList(),
	val temp: List<Float?> = emptyList(),
)

// ---- control -------------------------------------------------------------------------

@Serializable
data class LinkStatus(
	val id: String,
	val name: String = "",
	val url: String = "",
	val unit: String? = null,
	val scope: String = "user",
	val editable: Boolean = false,
	val port: Int? = null,
	val up: Boolean? = null,
	@SerialName("local_only") val localOnly: Boolean = false,
	@SerialName("unit_state") val unitState: String? = null,
)

@Serializable
data class Media(val players: List<Player> = emptyList(), val volume: Volume? = null)

@Serializable
data class Volume(val level: Float = 0f, val muted: Boolean = false)

@Serializable
data class Player(
	val id: String,
	val name: String = "",
	val status: String = "Stopped",
	val title: String? = null,
	val artist: String? = null,
	val album: String? = null,
	/** Microseconds. */
	val length: Long? = null,
	val position: Long? = null,
	val volume: Double? = null,
	@SerialName("has_art") val hasArt: Boolean = false,
	@SerialName("can_next") val canNext: Boolean = false,
	@SerialName("can_previous") val canPrevious: Boolean = false,
	@SerialName("can_play") val canPlay: Boolean = false,
	@SerialName("can_pause") val canPause: Boolean = false,
	@SerialName("can_seek") val canSeek: Boolean = false,
)

@Serializable
data class ControlInfo(
	/** Control is on now: in the settings, and not held off with `elamond control off`. */
	val enabled: Boolean,
	val power: PowerCaps = PowerCaps(),
	val profiles: Profiles? = null,
	val actions: List<ActionInfo> = emptyList(),
	val volume: Boolean = false,
	/** This phone's role; null from elamond before 0.5, which has none. */
	val role: String? = null,
	/** What this phone may do now, by name, in the service's order; null before 0.5 (see Permissions). */
	val allowed: List<String>? = null,
)

/** One request to act on the computer, from any paired phone, let through or refused. */
@Serializable
data class LogEntry(
	/** Unix milliseconds on the service's clock. */
	val t: Long,
	/** The phone's name when it asked. */
	val phone: String = "",
	@SerialName("client_id") val clientId: String = "",
	/** What it asked for, in the service's words: "power reboot", "unit restart nginx.service". */
	val what: String = "",
	/** "ok", or the error code it was answered with. */
	val result: String = "",
	/** What failed, or how an action ended: "exit 0", "timed out". */
	val message: String? = null,
)

/** logind's answers: "yes", "challenge" (needs a password), "no" or "na". */
@Serializable
data class PowerCaps(
	val poweroff: String = "na",
	val reboot: String = "na",
	val suspend: String = "na",
	val lock: Boolean = false,
)

@Serializable
data class Profiles(val active: String = "", val available: List<String> = emptyList())

/** An action from the service's settings; its id is opaque, a hash of what it runs. */
@Serializable
data class ActionInfo(val id: String, val name: String = "", val confirm: Boolean = true)

@Serializable
data class ActionResult(
	@SerialName("exit_code") val exitCode: Int? = null,
	@SerialName("timed_out") val timedOut: Boolean = false,
	val output: String = "",
)

@Serializable
data class LinkDef(
	val id: String? = null,
	val name: String,
	val url: String,
	val unit: String? = null,
	val scope: String? = null,
)

@Serializable
data class Listener(
	val port: Int,
	@SerialName("local_only") val localOnly: Boolean = false,
	val process: String? = null,
)

@Serializable
data class UnitInfo(val name: String, val description: String = "", val active: String = "")
