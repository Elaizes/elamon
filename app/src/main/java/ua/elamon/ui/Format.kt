package ua.elamon.ui

import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sqrt

/** Numbers as people read them. */
object Fmt {
	private fun num(v: Double, decimals: Int): String = String.format(Locale.US, "%.${decimals}f", v)

	private val BYTE_UNITS = listOf("B", "KiB", "MiB", "GiB", "TiB")
	private val BIT_UNITS = listOf("bit", "kbit", "Mbit", "Gbit", "Tbit")

	/** Value and unit apart, for layouts that set the unit smaller. */
	fun bytesParts(b: Double): Pair<String, String> {
		var v = abs(b)
		var i = 0
		while (v >= 1024 && i < BYTE_UNITS.lastIndex) {
			v /= 1024
			i++
		}
		val d = when {
			i == 0 -> 0
			v < 10 -> 2
			v < 100 -> 1
			else -> 0
		}
		return num(v, d) to BYTE_UNITS[i]
	}

	fun bytes(b: Double): String = bytesParts(b).let { "${it.first} ${it.second}" }
	fun bytes(b: Long): String = bytes(b.toDouble())

	/** Network speed in bytes or, as providers state it, in decimal bits. */
	fun rateParts(bytesPerSecond: Double, bits: Boolean): Pair<String, String> {
		if (!bits) return bytesParts(bytesPerSecond).let { it.first to "${it.second}/s" }
		var v = abs(bytesPerSecond) * 8
		var i = 0
		while (v >= 1000 && i < BIT_UNITS.lastIndex) {
			v /= 1000
			i++
		}
		val d = when {
			i == 0 -> 0
			v < 10 -> 2
			v < 100 -> 1
			else -> 0
		}
		return num(v, d) to "${BIT_UNITS[i]}/s"
	}

	fun rate(bytesPerSecond: Double, bits: Boolean = false): String =
		rateParts(bytesPerSecond, bits).let { "${it.first} ${it.second}" }

	fun percent(p: Float, decimals: Int = 0): String = "${num(p.toDouble(), decimals)}%"

	fun temp(c: Float, fahrenheit: Boolean = false): String =
		if (fahrenheit) "${(c * 9 / 5 + 32).roundToInt()}°F" else "${c.roundToInt()}°C"

	fun mhz(mhz: Int): String = if (mhz >= 1000) "${num(mhz / 1000.0, 2)} GHz" else "$mhz MHz"

	fun watts(w: Float): String = "${num(w.toDouble(), 1)} W"

	fun hz(hz: Float): String {
		val whole = hz.roundToInt()
		return if (abs(hz - whole) < 0.05f) "$whole Hz" else "${num(hz.toDouble(), 2)} Hz"
	}

	/** "3 d 4 h", "4 h 12 min", "12 min". */
	fun duration(seconds: Long): String {
		val d = seconds / 86_400
		val h = seconds % 86_400 / 3600
		val m = seconds % 3600 / 60
		return when {
			d > 0 -> "$d d $h h"
			h > 0 -> "$h h $m min"
			else -> "${m.coerceAtLeast(if (seconds > 0) 1 else 0)} min"
		}
	}

	/** "3:07" or "1:02:45". */
	fun clock(micros: Long): String {
		val s = (micros / 1_000_000).coerceAtLeast(0)
		val h = s / 3600
		val m = s % 3600 / 60
		val sec = s % 60
		return if (h > 0) String.format(Locale.US, "%d:%02d:%02d", h, m, sec) else String.format(Locale.US, "%d:%02d", m, sec)
	}

	/** "now", "30 s ago", "4 min ago". */
	fun ago(seconds: Long): String = when {
		seconds < 2 -> "now"
		seconds < 60 -> "$seconds s ago"
		else -> "${seconds / 60} min ago"
	}

	fun inches(widthMm: Int, heightMm: Int): String {
		val d = sqrt((widthMm * widthMm + heightMm * heightMm).toDouble()) / 25.4
		return "${num(d, 1)}″"
	}

	fun rpm(rpm: Int) = "$rpm RPM"
}
