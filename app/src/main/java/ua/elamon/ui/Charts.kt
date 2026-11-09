package ua.elamon.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.max

class ChartSeries(val values: List<Float>, val color: Color, val fill: Boolean = true)

/**
 * A time chart that fills from the right: the newest value sits at the right edge and the chart
 * keeps the same time scale whether it holds ten seconds or ten minutes of data.
 * [maxValue] fixes the scale (100 for percentages); otherwise it follows the data with headroom.
 */
@Composable
fun LineChart(
	series: List<ChartSeries>,
	capacity: Int,
	modifier: Modifier = Modifier,
	maxValue: Float? = null,
	minScale: Float = 1f,
	height: Dp = 96.dp,
	label: ((Float) -> String)? = null,
) {
	val grid = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f)
	val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
	val measurer = rememberTextMeasurer()
	Canvas(modifier.fillMaxWidth().height(height)) {
		val top = series.maxOfOrNull { s -> s.values.filter { !it.isNaN() }.maxOrNull() ?: 0f } ?: 0f
		val scale = maxValue ?: niceCeiling(max(top * 1.15f, minScale))
		val w = size.width
		val h = size.height
		val dash = PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 4.dp.toPx()))
		for (i in 1..3) {
			val y = h * i / 4f
			drawLine(grid, Offset(0f, y), Offset(w, y), strokeWidth = 1.dp.toPx(), pathEffect = dash)
		}
		val step = w / (capacity - 1).coerceAtLeast(1)
		for (s in series) {
			val v = s.values
			if (v.isEmpty()) continue
			val line = Path()
			val area = Path()
			var open = false
			var startX = 0f
			var lastX = 0f
			fun closeArea() {
				if (open && s.fill) {
					area.lineTo(lastX, h)
					area.lineTo(startX, h)
					area.close()
				}
			}
			v.forEachIndexed { i, value ->
				val x = w - (v.size - 1 - i) * step
				if (value.isNaN() || x < -step) {
					closeArea()
					open = false
					return@forEachIndexed
				}
				val y = h - (value / scale).coerceIn(0f, 1f) * h
				if (!open) {
					line.moveTo(x, y)
					area.moveTo(x, h)
					area.lineTo(x, y)
					startX = x
					open = true
				} else {
					line.lineTo(x, y)
					area.lineTo(x, y)
				}
				lastX = x
			}
			closeArea()
			if (s.fill) {
				drawPath(area, Brush.verticalGradient(listOf(s.color.copy(alpha = 0.32f), s.color.copy(alpha = 0.02f))))
			}
			drawPath(line, s.color, style = Stroke(2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
			val last = v.last()
			if (!last.isNaN()) {
				val y = h - (last / scale).coerceIn(0f, 1f) * h
				drawCircle(s.color, 3.5.dp.toPx(), Offset(w, y))
			}
		}
		if (label != null) drawScaleLabel(measurer, label(scale), labelColor)
	}
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawScaleLabel(m: TextMeasurer, text: String, color: Color) {
	drawText(m, text, topLeft = Offset(2.dp.toPx(), 0f), style = TextStyle(color = color, fontSize = 10.sp))
}

/** 1, 2, 5, 10, 20, 50… so the scale label reads as a round number. */
fun niceCeiling(v: Float): Float {
	if (v <= 0f) return 1f
	var p = 1f
	while (p * 10 <= v) p *= 10
	while (p > v) p /= 10
	for (m in floatArrayOf(1f, 2f, 2.5f, 5f, 10f)) if (p * m >= v) return p * m
	return p * 10
}

/** A three-quarter ring, open at the bottom, with content in the middle. */
@Composable
fun RingGauge(
	fraction: Float,
	color: Color,
	modifier: Modifier = Modifier,
	size: Dp = 112.dp,
	thickness: Dp = 10.dp,
	content: @Composable () -> Unit,
) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val animated by animateFloatAsState(fraction.coerceIn(0f, 1f), tween(450), label = "gauge")
	Box(modifier.size(size), contentAlignment = Alignment.Center) {
		Canvas(Modifier.size(size)) {
			val stroke = thickness.toPx()
			val inset = stroke / 2
			val arcSize = Size(this.size.width - stroke, this.size.height - stroke)
			val topLeft = Offset(inset, inset)
			drawArc(track, 135f, 270f, false, topLeft, arcSize, style = Stroke(stroke, cap = StrokeCap.Round))
			if (animated > 0.002f) {
				drawArc(color, 135f, 270f * animated, false, topLeft, arcSize, style = Stroke(stroke, cap = StrokeCap.Round))
			}
		}
		content()
	}
}

@Composable
fun UsageBar(fraction: Float, color: Color, modifier: Modifier = Modifier, height: Dp = 8.dp) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val animated by animateFloatAsState(fraction.coerceIn(0f, 1f), tween(450), label = "bar")
	Canvas(modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2))) {
		drawRect(track)
		if (animated > 0f) {
			drawRoundRect(color, size = Size(size.width * animated, size.height), cornerRadius = CornerRadius(size.height / 2))
		}
	}
}

/** One thin column per CPU core. */
@Composable
fun CoreBars(values: List<Float>, modifier: Modifier = Modifier, height: Dp = 40.dp) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val colors = values.map { loadColor(it / 100f) }
	Row(modifier.fillMaxWidth().height(height), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
		values.forEachIndexed { i, v ->
			val animated by animateFloatAsState((v / 100f).coerceIn(0f, 1f), tween(450), label = "core")
			Canvas(Modifier.weight(1f).fillMaxHeight()) {
				val r = CornerRadius(3.dp.toPx())
				drawRoundRect(track, cornerRadius = r)
				val bh = size.height * animated
				if (bh > 0.5f) {
					drawRoundRect(colors[i], topLeft = Offset(0f, size.height - bh), size = Size(size.width, bh), cornerRadius = r)
				}
			}
		}
	}
}
