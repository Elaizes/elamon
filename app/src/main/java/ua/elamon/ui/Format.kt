package ua.elamon.ui

import android.content.Context
import ua.elamon.R
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sqrt

/** Numbers as people read them; units come from string resources so they translate. */
object Fmt {
	private fun num(v: Double, decimals: Int): String = String.format(Locale.getDefault(), "%.${decimals}f", v)

	fun bytes(ctx: Context, b: Double): String {
		val units = ctx.resources.getStringArray(R.array.byte_units)
		var v = abs(b)
		var i = 0
		while (v >= 1024 && i < units.lastIndex) {
			v /= 1024
			i++
		}
		val d = when {
			i == 0 -> 0
			v < 10 -> 2
			v < 100 -> 1
			else -> 0
		}
		return "${num(v, d)} ${units[i]}"
	}

	fun bytes(ctx: Context, b: Long) = bytes(ctx, b.toDouble())

	fun rate(ctx: Context, bps: Double): String = ctx.getString(R.string.unit_per_second, bytes(ctx, bps))

	fun percent(p: Float, decimals: Int = 0): String = "${num(p.toDouble(), decimals)}%"

	fun temp(c: Float): String = "${c.roundToInt()}°C"

	fun mhz(ctx: Context, mhz: Int): String =
		if (mhz >= 1000) ctx.getString(R.string.unit_ghz, num(mhz / 1000.0, 2)) else ctx.getString(R.string.unit_mhz, mhz)

	fun watts(ctx: Context, w: Float): String = ctx.getString(R.string.unit_watts, num(w.toDouble(), 1))

	fun hz(ctx: Context, hz: Float): String {
		val whole = hz.roundToInt()
		val text = if (abs(hz - whole) < 0.05f) whole.toString() else num(hz.toDouble(), 2)
		return ctx.getString(R.string.unit_hz, text)
	}

	/** "3 d 4 h", "4 h 12 min", "12 min". */
	fun duration(ctx: Context, seconds: Long): String {
		val d = seconds / 86_400
		val h = seconds % 86_400 / 3600
		val m = seconds % 3600 / 60
		return when {
			d > 0 -> ctx.getString(R.string.duration_days_hours, d, h)
			h > 0 -> ctx.getString(R.string.duration_hours_minutes, h, m)
			else -> ctx.getString(R.string.duration_minutes, m.coerceAtLeast(if (seconds > 0) 1 else 0))
		}
	}

	fun inches(widthMm: Int, heightMm: Int): String {
		val d = sqrt((widthMm * widthMm + heightMm * heightMm).toDouble()) / 25.4
		return "${num(d, 1)}″"
	}

	fun rpm(ctx: Context, rpm: Int) = ctx.getString(R.string.unit_rpm, rpm)
}
