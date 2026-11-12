package io.github.elaizes.elamon.ui

import android.content.res.Resources
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.platform.LocalResources
import java.math.BigDecimal
import java.math.RoundingMode
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.abs
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import io.github.elaizes.elamon.R

/**
 * Numbers as people read them, by the service's formats (docs/config.md, "Formats"):
 * `elamond/fixtures/formats.json` holds the cases both sides are tested against. Halves round away
 * from zero; a unit follows its number after a space, but %, °C and °F follow it directly. Unit
 * symbols, and the few words a format writes, read as the service writes them and live here.
 */
object Fmt {
	// ---- unit symbols ----------------------------------------------------------------------

	private val BYTE_UNITS = listOf("B", "KiB", "MiB", "GiB", "TiB")
	private val BIT_UNITS = listOf("bit", "kbit", "Mbit", "Gbit", "Tbit")
	private const val PER_SECOND = "/s"
	private const val BITS_PER_SECOND = "bit/s"
	const val PERCENT = "%"
	const val CELSIUS = "°C"
	const val FAHRENHEIT = "°F"
	private const val MHZ = "MHz"
	private const val GHZ = "GHz"
	private const val WATT = "W"
	private const val WATT_HOUR = "Wh"
	private const val RPM = "RPM"

	/** Units written right after their number. */
	private val ATTACHED = setOf(PERCENT, CELSIUS, FAHRENHEIT)

	/** What `bool` writes, and `duration`'s units: as the service writes them into strings. */
	private const val YES = "yes"
	private const val NO = "no"
	private const val DAYS = "d"
	private const val HOURS = "h"
	private const val MINUTES = "min"

	private const val KILO = 1000.0
	private const val KIBI = 1024.0
	private const val SECONDS_PER_DAY = 86_400L
	private const val SECONDS_PER_HOUR = 3_600L
	private const val SECONDS_PER_MINUTE = 60L
	private const val MS_PER_SECOND = 1000L

	/** `{:.2}`: that many decimals. */
	private val PATTERN = Regex("""^\{:\.(\d)}$""")

	/** [v] with [decimals] decimals, halves away from zero, as the shortest text of the number reads. */
	fun round(v: Double, decimals: Int): String {
		if (!v.isFinite()) return v.toString()
		val s = BigDecimal(v.toString()).setScale(decimals, RoundingMode.HALF_UP).toPlainString()
		// No "-0": a figure rounded to nothing has no sign.
		return if (s.startsWith("-") && s.all { it == '-' || it == '0' || it == '.' }) s.drop(1) else s
	}

	/** [number] and its [unit], with a space unless the unit follows it directly. */
	fun withUnit(number: String, unit: String?): String = when {
		unit.isNullOrEmpty() -> number
		unit in ATTACHED -> number + unit
		else -> "$number $unit"
	}

	/**
	 * [v] by [format] and [unit]; [field] is the table field it comes from, for `time` (`t` is in
	 * milliseconds). A string is shown as it is, a bool as yes or no; null when the value is missing.
	 */
	fun format(v: JsonElement?, format: String?, unit: String?, field: String? = null): String? {
		val p = v as? JsonPrimitive ?: return null
		if (p is JsonNull) return null
		if (p.isString) return p.content
		p.booleanOrNull?.let { return bool(it) }
		val n = p.doubleOrNull ?: return p.content
		return number(n, format, unit, field)
	}

	fun bool(b: Boolean): String = if (b) YES else NO

	/** A number by [format] and [unit]. */
	fun number(v: Double, format: String?, unit: String?, field: String? = null): String {
		val (n, u) = parts(v, format, unit, field)
		return withUnit(n, u)
	}

	/** A number by [format] and [unit], the figure and its unit apart, for layouts that set the unit smaller. */
	fun parts(v: Double, format: String?, unit: String?, field: String? = null): Pair<String, String?> {
		PATTERN.matchEntire(format.orEmpty())?.let { m -> return round(v, m.groupValues[1].toInt()) to unit }
		return when (format) {
			"integer" -> round(v, 0) to unit
			"percent" -> round(v, 0) to PERCENT
			"bytes" -> bytesParts(v)
			"rate" -> rateParts(v, bits = unit == BITS_PER_SECOND)
			"temp" -> round(v, 0) to (unit?.ifEmpty { null } ?: CELSIUS)
			"mhz" -> mhzParts(v)
			"watts" -> round(v, 1) to (unit?.ifEmpty { null } ?: WATT)
			"wh" -> round(v, 1) to (unit?.ifEmpty { null } ?: WATT_HOUR)
			"rpm" -> round(v, 0) to (unit?.ifEmpty { null } ?: RPM)
			"load" -> round(v, 2) to unit
			"duration" -> duration(v.toLong()) to null
			"time" -> time(v, millis = field == "t") to null
			"bool" -> bool(v != 0.0) to null
			// "number", "text" and any other: up to one decimal.
			else -> round(v, 1).removeSuffix(".0") to unit
		}
	}

	/**
	 * [value] in the largest of [units] (each [base] times the one before) that keeps it at or above 1,
	 * with two decimals under 10, one under 100, none above. The rounded figure decides, so
	 * 1023.7 KiB reads "1.00 MiB", not "1024 KiB", and 9.996 reads "10.0", not "10.00".
	 */
	private fun scaled(value: Double, base: Double, units: List<String>): Pair<String, String> {
		var v = abs(value)
		var i = 0
		while (v >= base && i < units.lastIndex) {
			v /= base
			i++
		}
		var text = round(v, decimals(i, v))
		val rounded = text.toDouble()
		if (rounded >= base && i < units.lastIndex) {
			v /= base
			i++
			text = round(v, decimals(i, v))
		} else if (decimals(i, rounded) != decimals(i, v)) {
			text = round(v, decimals(i, rounded))
		}
		return (if (value < 0 && text.any { it in '1'..'9' }) "-$text" else text) to units[i]
	}

	private fun decimals(unit: Int, v: Double): Int = when {
		unit == 0 -> 0
		v < 10 -> 2
		v < 100 -> 1
		else -> 0
	}

	fun bytesParts(b: Double): Pair<String, String> = scaled(b, KIBI, BYTE_UNITS)

	fun bytes(b: Double): String = bytesParts(b).let { "${it.first} ${it.second}" }

	/** A rate in bytes per second, or in bits (by 1000) where the service sends bits. */
	fun rateParts(perSecond: Double, bits: Boolean): Pair<String, String> {
		val (v, unit) = if (bits) scaled(perSecond, KILO, BIT_UNITS) else bytesParts(perSecond)
		return v to "$unit$PER_SECOND"
	}

	private fun mhzParts(mhz: Double): Pair<String, String> =
		if (round(mhz, 0).toDouble() >= KILO) round(mhz / KILO, 2) to GHZ else round(mhz, 0) to MHZ

	fun percent(p: Double): String = round(p, 0) + PERCENT

	/** "3 d 4 h", "4 h 12 min", "12 min"; a moment over nothing is a minute. */
	fun duration(seconds: Long): String {
		val d = seconds / SECONDS_PER_DAY
		val h = seconds % SECONDS_PER_DAY / SECONDS_PER_HOUR
		val m = seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE
		return when {
			d > 0 -> "$d $DAYS $h $HOURS"
			h > 0 -> "$h $HOURS $m $MINUTES"
			else -> "${m.coerceAtLeast(if (seconds > 0) 1 else 0)} $MINUTES"
		}
	}

	/** A moment, the phone's way: from Unix seconds, or milliseconds. */
	fun time(v: Double, millis: Boolean): String {
		val ms = if (millis) v.toLong() else (v * MS_PER_SECOND).toLong()
		return DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(ms))
	}

	/** Milliseconds as plain seconds, without trailing zeros: "1", "0.25", "1.5". */
	fun seconds(ms: Int): String = BigDecimal(ms).movePointLeft(3).stripTrailingZeros().toPlainString()

	/** A track's place, from seconds: "3:07" or "1:02:45". */
	fun clock(seconds: Double): String {
		val s = seconds.toLong().coerceAtLeast(0)
		val h = s / SECONDS_PER_HOUR
		val m = s % SECONDS_PER_HOUR / SECONDS_PER_MINUTE
		val sec = s % SECONDS_PER_MINUTE
		return if (h > 0) String.format(Locale.US, "%d:%02d:%02d", h, m, sec) else String.format(Locale.US, "%d:%02d", m, sec)
	}

	// ---- words, from resources --------------------------------------------------------------

	/** "now", "30 s ago", "4 min ago". */
	fun ago(res: Resources, seconds: Long): String = when {
		seconds < 2 -> res.getString(R.string.ago_now)
		seconds < SECONDS_PER_MINUTE -> res.getQuantityString(R.plurals.ago_seconds, seconds.toInt(), seconds)
		else -> (seconds / SECONDS_PER_MINUTE).let { res.getQuantityString(R.plurals.ago_minutes, it.toInt(), it) }
	}
}

/**
 * The app's resources, for words that code outside composition needs (chart labels drawn later,
 * lists built in remember, Fmt's words); re-read on a configuration change, as stringResource is.
 */
@Composable
@ReadOnlyComposable
fun resources(): Resources = LocalResources.current
