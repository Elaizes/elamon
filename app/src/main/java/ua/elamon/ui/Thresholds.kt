package ua.elamon.ui

import kotlin.math.abs
import kotlin.math.max
import ua.elamon.data.Temp

/**
 * Where readings turn amber and red, and how the bars and charts that show them are scaled.
 * Kept apart from the design tokens: these are about the computer, not the look.
 */
object Thresholds {
	// ---- temperature, °C ---------------------------------------------------------------------

	/** Red from here, unless the sensor gives its own limit. */
	const val TEMP_HOT = 95f

	/** Amber starts this far below red, and green turns toward amber this far below that. */
	const val TEMP_BAND = 20f

	/** Amber never starts lower than this, whatever a sensor's limits say. */
	const val TEMP_WARM_MIN = 40f

	/** A temperature bar is full here, and a temperature sparkline's scale starts here, unless the sensor has a limit. */
	const val TEMP_BAR_TOP = 100f

	/** A sensor's limit is written beside it once the reading is this close. */
	const val TEMP_LIMIT_NEAR = 15f

	// ---- load, a fraction of the whole --------------------------------------------------------

	const val LOAD_WARN = 0.75f
	const val LOAD_HOT = 0.9f

	// ---- battery, % ---------------------------------------------------------------------------

	const val BATTERY_HOT = 15f
	const val BATTERY_WARN = 30f

	/** Less than this many watts in or out is noise, not worth a figure. */
	const val BATTERY_POWER_SHOWN = 0.05f

	// ---- processes ----------------------------------------------------------------------------

	/** A process's CPU bar is full at this share, in %: one busy thread on a big machine is a lot already. */
	const val PROCESS_CPU_FULL = 25f

	/** Below this share, in %, a process's CPU figure stays grey. */
	const val PROCESS_CPU_SHOWN = 1f

	// ---- fans ---------------------------------------------------------------------------------

	/** The fastest fan fills 1 / this of its bar, and the bar never stands for fewer RPM than the minimum. */
	const val FAN_HEADROOM = 1.25f
	const val FAN_MIN_SCALE = 3000f

	// ---- charts -------------------------------------------------------------------------------

	/** The top of a percentage scale. */
	const val PERCENT = 100f

	/** Room above the highest point, so a line doesn't run along the top. */
	const val CHART_HEADROOM = 1.15f

	/** Rows the dashed grid divides a chart into. */
	const val CHART_GRID_ROWS = 4

	/** The least each chart's scale goes down to, so a quiet line stays low instead of filling it. */
	const val CHART_MIN_TEMP = 60f
	const val CHART_MIN_NET = 64f * 1024
	const val CHART_MIN_DISK = 1024f * 1024
	const val CHART_MIN_POWER = 5f
}

/** Where a reading sits on the way from fine to too much. */
enum class Band { Good, GoodToWarn, Warn, WarnToHot, Hot }

/** A [band], and for the two that blend, how far across it the reading is, from 0 to 1. */
data class Level(val band: Band, val t: Float = 0f)

/**
 * A temperature's level: red at the sensor's critical limit (or its high one, or [Thresholds.TEMP_HOT]),
 * amber from its high limit (or [Thresholds.TEMP_BAND] below red), green below that by as much again.
 */
fun tempLevel(value: Float, high: Float? = null, crit: Float? = null): Level {
	val hot = crit ?: high ?: Thresholds.TEMP_HOT
	val warm = (high?.takeIf { it < hot } ?: (hot - Thresholds.TEMP_BAND)).coerceAtLeast(Thresholds.TEMP_WARM_MIN)
	val cool = warm - Thresholds.TEMP_BAND
	return when {
		value >= hot -> Level(Band.Hot)
		value >= warm -> Level(Band.WarnToHot, ((value - warm) / (hot - warm)).coerceIn(0f, 1f))
		value >= cool -> Level(Band.GoodToWarn, ((value - cool) / Thresholds.TEMP_BAND).coerceIn(0f, 1f))
		else -> Level(Band.Good)
	}
}

/** A load's level, [fraction] of the whole: its own colour until it nears full. */
fun loadLevel(fraction: Float): Level = when {
	fraction >= Thresholds.LOAD_HOT -> Level(Band.Hot)
	fraction >= Thresholds.LOAD_WARN ->
		Level(Band.WarnToHot, ((fraction - Thresholds.LOAD_WARN) / (Thresholds.LOAD_HOT - Thresholds.LOAD_WARN)).coerceIn(0f, 1f))
	else -> Level(Band.Good)
}

/** A battery's level by its charge in %. */
fun batteryLevel(percent: Float): Level = when {
	percent < Thresholds.BATTERY_HOT -> Level(Band.Hot)
	percent < Thresholds.BATTERY_WARN -> Level(Band.Warn)
	else -> Level(Band.Good)
}

/** What a temperature bar stands for when full: the sensor's limit, or [Thresholds.TEMP_BAR_TOP]. */
fun tempBarTop(high: Float?, crit: Float?): Float = crit ?: high ?: Thresholds.TEMP_BAR_TOP

/** How full a temperature's bar is. */
fun tempBarFraction(value: Float, high: Float? = null, crit: Float? = null): Float = (value / tempBarTop(high, crit)).coerceIn(0f, 1f)

/** Whether a sensor's critical limit is near enough to write beside the reading. */
fun limitNear(value: Float, crit: Float?): Boolean = crit != null && value >= crit - Thresholds.TEMP_LIMIT_NEAR

/** What the fan bars stand for when full, with the fastest fan at [maxRpm]. */
fun fanBarTop(maxRpm: Int): Float = max(maxRpm * Thresholds.FAN_HEADROOM, Thresholds.FAN_MIN_SCALE)

/** How full a process's CPU bar is, and how hot its colour, at [cpu] %. */
fun processCpuFraction(cpu: Float): Float = cpu / Thresholds.PROCESS_CPU_FULL

/**
 * The sensor the service took the CPU's temperature from, for its limits: the first CPU sensor named
 * like the whole package, else the first CPU sensor, else the ACPI zone; null unless it reads [cpuTemp].
 */
fun cpuSensor(temps: List<Temp>, cpuTemp: Float?): Temp? {
	if (cpuTemp == null) return null
	val cpu = temps.filter { it.kind == "cpu" }
	val found = cpu.firstOrNull { isPackageLabel(it.label) } ?: cpu.firstOrNull()
		?: temps.firstOrNull { it.chip == "acpitz" || it.chip == "x86_pkg_temp" }
	return found?.takeIf { abs(it.value - cpuTemp) < SAME_READING }
}

/** Labels the service reads the whole CPU's temperature from. */
private fun isPackageLabel(label: String): Boolean {
	val l = label.lowercase()
	return l.startsWith("package") || l == "tctl" || l == "tdie" || l == "cpu"
}

/** Both are rounded to a tenth from the same reading: anything further apart is another sensor. */
private const val SAME_READING = 0.05f
