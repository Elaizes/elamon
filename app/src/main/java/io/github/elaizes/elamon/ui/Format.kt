package io.github.elaizes.elamon.ui

import android.content.res.Resources
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.platform.LocalResources
import java.math.BigDecimal
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sqrt
import io.github.elaizes.elamon.R

/** Numbers as people read them. Unit symbols read the same in every language and live here; words come from strings.xml. */
object Fmt {
	private fun num(v: Double, decimals: Int): String = String.format(Locale.US, "%.${decimals}f", v)

	// ---- unit symbols ----------------------------------------------------------------------

	private val BYTE_UNITS = listOf("B", "KiB", "MiB", "GiB", "TiB")
	private val BIT_UNITS = listOf("bit", "kbit", "Mbit", "Gbit", "Tbit")
	private const val PER_SECOND = "/s"
	const val PERCENT = "%"
	const val CELSIUS = "°C"
	const val FAHRENHEIT = "°F"
	private const val MHZ = "MHz"
	private const val GHZ = "GHz"
	private const val HZ = "Hz"
	private const val WATT = "W"
	private const val WATT_HOUR = "Wh"
	private const val INCH = "″"
	private const val CM = "cm"
	private const val RPM = "RPM"
	private const val MBIT_PER_SECOND = "Mbit/s"

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
		var text = num(v, decimals(i, v))
		val rounded = text.toDouble()
		if (rounded >= base && i < units.lastIndex) {
			v /= base
			i++
			text = num(v, decimals(i, v))
		} else if (decimals(i, rounded) != decimals(i, v)) {
			text = num(v, decimals(i, rounded))
		}
		return text to units[i]
	}

	private fun decimals(unit: Int, v: Double): Int = when {
		unit == 0 -> 0
		v < 10 -> 2
		v < 100 -> 1
		else -> 0
	}

	/** Value and unit apart, for layouts that set the unit smaller. */
	fun bytesParts(b: Double): Pair<String, String> = scaled(b, 1024.0, BYTE_UNITS)

	fun bytes(b: Double): String = bytesParts(b).let { "${it.first} ${it.second}" }
	fun bytes(b: Long): String = bytes(b.toDouble())

	/** Network speed in bytes or, as providers state it, in decimal bits. */
	fun rateParts(bytesPerSecond: Double, bits: Boolean): Pair<String, String> {
		val (v, unit) = if (bits) scaled(bytesPerSecond * 8, 1000.0, BIT_UNITS) else bytesParts(bytesPerSecond)
		return v to "$unit$PER_SECOND"
	}

	fun rate(bytesPerSecond: Double, bits: Boolean = false): String =
		rateParts(bytesPerSecond, bits).let { "${it.first} ${it.second}" }

	/** A network link's speed, which the kernel gives in Mbit/s. */
	fun linkSpeed(mbit: Long): String = "$mbit $MBIT_PER_SECOND"

	fun percent(p: Float, decimals: Int = 0): String = "${num(p.toDouble(), decimals)}$PERCENT"

	fun temp(c: Float, fahrenheit: Boolean = false): String =
		if (fahrenheit) "${(c * 9 / 5 + 32).roundToInt()}$FAHRENHEIT" else "${c.roundToInt()}$CELSIUS"

	fun mhz(mhz: Int): String = if (mhz >= 1000) "${num(mhz / 1000.0, 2)} $GHZ" else "$mhz $MHZ"

	fun watts(w: Float): String = "${num(w.toDouble(), 1)} $WATT"

	/** Battery energy. */
	fun wattHours(wh: Float): String = "${num(wh.toDouble(), 1)} $WATT_HOUR"

	/** A load average, as uptime and top print it. */
	fun load(v: Float): String = num(v.toDouble(), 2)

	fun hz(hz: Float): String {
		val whole = hz.roundToInt()
		return if (abs(hz - whole) < 0.05f) "$whole $HZ" else "${num(hz.toDouble(), 2)} $HZ"
	}

	/** Milliseconds as plain seconds, without trailing zeros: "1", "0.25", "1.5". */
	fun seconds(ms: Int): String = BigDecimal(ms).movePointLeft(3).stripTrailingZeros().toPlainString()

	/** "3:07" or "1:02:45". */
	fun clock(micros: Long): String {
		val s = (micros / 1_000_000).coerceAtLeast(0)
		val h = s / 3600
		val m = s % 3600 / 60
		val sec = s % 60
		return if (h > 0) String.format(Locale.US, "%d:%02d:%02d", h, m, sec) else String.format(Locale.US, "%d:%02d", m, sec)
	}

	fun inches(widthMm: Int, heightMm: Int): String {
		val d = sqrt((widthMm * widthMm + heightMm * heightMm).toDouble()) / 25.4
		return "${num(d, 1)}$INCH"
	}

	/** A screen's width and height in whole centimetres: "53 × 30 cm". */
	fun centimetres(widthMm: Int, heightMm: Int): String = "${widthMm / 10} × ${heightMm / 10} $CM"

	fun rpm(rpm: Int) = "$rpm $RPM"

	// ---- words, from resources --------------------------------------------------------------

	/** "3 d 4 h", "4 h 12 min", "12 min". */
	fun duration(res: Resources, seconds: Long): String {
		val d = seconds / 86_400
		val h = seconds % 86_400 / 3600
		val m = seconds % 3600 / 60
		return when {
			d > 0 -> res.getString(R.string.duration_days_hours, d, h)
			h > 0 -> res.getString(R.string.duration_hours_minutes, h, m)
			else -> res.getString(R.string.duration_minutes, m.coerceAtLeast(if (seconds > 0) 1 else 0))
		}
	}

	/** "now", "30 s ago", "4 min ago". */
	fun ago(res: Resources, seconds: Long): String = when {
		seconds < 2 -> res.getString(R.string.ago_now)
		seconds < 60 -> res.getQuantityString(R.plurals.ago_seconds, seconds.toInt(), seconds)
		else -> (seconds / 60).let { res.getQuantityString(R.plurals.ago_minutes, it.toInt(), it) }
	}
}

/**
 * The app's resources, for words that code outside composition needs (chart labels drawn later,
 * lists built in remember, Fmt's words); re-read on a configuration change, as stringResource is.
 */
@Composable
@ReadOnlyComposable
fun resources(): Resources = LocalResources.current
