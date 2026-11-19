package io.github.elaizes.elamon.widget

import android.graphics.Bitmap
import android.graphics.BlurMaskFilter
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.SweepGradient
import androidx.core.graphics.createBitmap
import kotlin.math.roundToInt
import io.github.elaizes.elamon.ui.Alpha
import io.github.elaizes.elamon.ui.Gauge

/** A ring's stroke, the room around it for its glow, and the glow, as parts of its size (drawable/widget_ring_track.xml's too). */
private const val THICKNESS = 0.11f
private const val INSET = 0.04f
private const val GLOW = 0.05f

private const val FULL_TURN = 360f
private const val OPAQUE = 255

/** The gradient's second stop never sits on its first, which would make it a hard edge. */
private const val MIN_STOP = 0.01f

/**
 * The filled part of a gauge's small ring on the home-screen widget, [fraction] of the way round and
 * [px] square: as the app's ring, a gradient from its start with a glow under it, but in alpha only,
 * for the widget to tint with the level's colour (a picture can't change with day and night; a tint
 * can). Its empty track is drawable/widget_ring_track.xml, of the same proportions.
 */
internal fun ringFill(fraction: Float, px: Int): Bitmap {
	val bitmap = createBitmap(px, px, Bitmap.Config.ALPHA_8)
	val f = fraction.coerceIn(0f, 1f)
	if (f <= Gauge.MIN_SHOWN) return bitmap
	val canvas = Canvas(bitmap)
	val stroke = px * THICKNESS
	val inset = stroke / 2 + px * INSET
	val oval = RectF(inset, inset, px - inset, px - inset)
	val sweep = Gauge.SWEEP * f
	fun paint() = Paint(Paint.ANTI_ALIAS_FLAG).apply {
		style = Paint.Style.STROKE
		strokeWidth = stroke
		strokeCap = Paint.Cap.ROUND
	}
	val glow = paint().apply {
		alpha = (Alpha.GLOW * OPAQUE).roundToInt()
		maskFilter = BlurMaskFilter(px * GLOW, BlurMaskFilter.Blur.NORMAL)
	}
	canvas.drawArc(oval, Gauge.START, sweep, false, glow)
	// The sweep gradient starts at 3 o'clock; turning it puts its faint end at the ring's start.
	val centre = px / 2f
	val ring = paint().apply {
		shader = SweepGradient(
			centre,
			centre,
			intArrayOf(alphaOnly(Alpha.RING_START), alphaOnly(1f)),
			floatArrayOf(0f, (sweep / FULL_TURN).coerceAtLeast(MIN_STOP)),
		).apply { setLocalMatrix(Matrix().apply { setRotate(Gauge.START, centre, centre) }) }
	}
	canvas.drawArc(oval, Gauge.START, sweep, false, ring)
	return bitmap
}

/** A colour that is only [alpha]: the tint gives it its colour. */
private fun alphaOnly(alpha: Float): Int = (alpha * OPAQUE).roundToInt() shl 24
