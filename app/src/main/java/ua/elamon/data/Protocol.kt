package ua.elamon.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** What the service sends; mirrors elamond/src/model.rs. Unknown fields are ignored so newer services still work. */
val ProtocolJson = Json {
	ignoreUnknownKeys = true
	classDiscriminator = "type"
	explicitNulls = false
	coerceInputValues = true
}

@Serializable
sealed interface StreamMessage

@Serializable
data class Hello(
	val app: String = "",
	val api: Int = 0,
	val id: String = "",
	val name: String = "",
	val version: String = "",
)

@Serializable
data class PairResponse(
	val token: String,
	@SerialName("client_id") val clientId: String = "",
	val id: String = "",
	val name: String = "",
)

@Serializable
@SerialName("info")
data class Info(
	val id: String = "",
	val name: String = "",
	@SerialName("service_version") val serviceVersion: String = "",
	val api: Int = 0,
	val os: String = "",
	val kernel: String = "",
	val arch: String = "",
	@SerialName("cpu_model") val cpuModel: String = "",
	@SerialName("cpu_physical_cores") val cpuPhysicalCores: Int? = null,
	@SerialName("cpu_logical_cores") val cpuLogicalCores: Int = 0,
	@SerialName("memory_total") val memoryTotal: Long = 0,
	@SerialName("boot_time") val bootTime: Long = 0,
	val gpus: List<GpuInfo> = emptyList(),
	val capabilities: List<String> = emptyList(),
) : StreamMessage

@Serializable
data class GpuInfo(
	val id: String = "",
	val vendor: String = "",
	val name: String = "",
	val driver: String = "",
	@SerialName("boot_display") val bootDisplay: Boolean = false,
)

@Serializable
@SerialName("sample")
data class Sample(
	val t: Long = 0,
	val uptime: Long = 0,
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
	val id: String = "",
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
	val chip: String = "",
	val kind: String = "other",
	val device: String? = null,
	val label: String = "",
	val value: Float = 0f,
	val high: Float? = null,
	val crit: Float? = null,
)

@Serializable
data class Fan(val chip: String = "", val label: String = "", val rpm: Int = 0)

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
)

@Serializable
data class Disk(
	val device: String = "",
	val mount: String = "",
	val fs: String = "",
	val total: Long = 0,
	val available: Long = 0,
	val removable: Boolean = false,
)

@Serializable
data class DiskIo(val name: String = "", val model: String? = null, val read: Double = 0.0, val write: Double = 0.0)

@Serializable
data class Net(
	val name: String = "",
	val kind: String = "virtual",
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
	val connector: String = "",
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
data class Mode(val width: Int = 0, val height: Int = 0, val refresh: Float = 0f)

@Serializable
data class Proc(
	val pid: Int = 0,
	val ppid: Int? = null,
	val name: String = "",
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
	val t: List<Long> = emptyList(),
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
	val id: String = "",
	val usage: List<Float?> = emptyList(),
	val temp: List<Float?> = emptyList(),
)
