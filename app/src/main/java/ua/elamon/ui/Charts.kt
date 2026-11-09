package ua.elamon.ui

import android.graphics.BlurMaskFilter
import android.os.Build
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.max
import kotlin.math.roundToInt

class ChartSeries(
	val values: List<Float>,
	val color: Color,
	val fill: Boolean = true,
	/** How the value reads when someone touches the chart. */
	val format: ((Float) -> String)? = null,
)

/**
 * A time chart that fills from the right: the newest value sits at the right edge and the time
 * scale stays the same whether it holds ten seconds or ten minutes. Dragging a finger along it
 * shows the values at that moment. [maxValue] fixes the scale (100 for percentages); otherwise it
 * follows the data with headroom, to round numbers ([binary] for bytes).
 */
@Composable
fun LineChart(
	series: List<ChartSeries>,
	capacity: Int,
	modifier: Modifier = Modifier,
	maxValue: Float? = null,
	minScale: Float = 1f,
	binary: Boolean = false,
	height: Dp = 120.dp,
	secondsPerPoint: Float = 1f,
	grid: Boolean = true,
	label: ((Float) -> String)? = null,
	interactive: Boolean = true,
) {
	val gridColor = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f)
	val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
	val tipBg = MaterialTheme.colorScheme.inverseSurface
	val tipFg = MaterialTheme.colorScheme.inverseOnSurface
	val measurer = rememberTextMeasurer()
	var touchX by remember { mutableStateOf<Float?>(null) }

	val touch = if (interactive) {
		Modifier
			.pointerInput(Unit) {
				detectHorizontalDragGestures(
					onDragStart = { touchX = it.x },
					onDragEnd = { touchX = null },
					onDragCancel = { touchX = null },
					onHorizontalDrag = { change, _ -> touchX = change.position.x },
				)
			}
			.pointerInput(Unit) {
				detectTapGestures(onPress = {
					touchX = it.x
					tryAwaitRelease()
					touchX = null
				})
			}
	} else Modifier

	Canvas(modifier.fillMaxWidth().height(height).then(touch)) {
		val top = series.maxOfOrNull { s -> s.values.filter { !it.isNaN() }.maxOrNull() ?: 0f } ?: 0f
		val wanted = max(top * 1.15f, minScale)
		val scale = maxValue ?: if (binary) niceBinary(wanted) else niceCeiling(wanted)
		val w = size.width
		val h = size.height
		if (grid) {
			val dash = PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 5.dp.toPx()))
			for (i in 1..3) {
				val y = h * i / 4f
				drawLine(gridColor, Offset(0f, y), Offset(w, y), strokeWidth = 1.dp.toPx(), pathEffect = dash)
			}
		}
		val step = w / (capacity - 1).coerceAtLeast(1)
		for (s in series) drawSeries(s, scale, step, w, h)
		if (label != null && grid) {
			drawText(measurer, label(scale), topLeft = Offset(2.dp.toPx(), 0f), style = TextStyle(color = labelColor, fontSize = 10.sp, fontFamily = Inter))
		}

		val tx = touchX ?: return@Canvas
		val longest = series.maxOfOrNull { it.values.size } ?: return@Canvas
		if (longest == 0) return@Canvas
		// The touched point, counted back from the newest.
		val back = ((w - tx) / step).roundToInt().coerceIn(0, longest - 1)
		val x = w - back * step
		drawLine(labelColor.copy(alpha = 0.6f), Offset(x, 0f), Offset(x, h), strokeWidth = 1.5.dp.toPx())
		val parts = mutableListOf(Fmt.ago((back * secondsPerPoint).roundToInt().toLong()))
		for (s in series) {
			val i = s.values.size - 1 - back
			val v = s.values.getOrNull(i) ?: continue
			if (v.isNaN()) continue
			val y = h - (v / scale).coerceIn(0f, 1f) * h
			drawCircle(s.color, 5.dp.toPx(), Offset(x, y))
			drawCircle(tipFg, 2.dp.toPx(), Offset(x, y))
			s.format?.let { parts += it(v) }
		}
		val text = measurer.measure(parts.joinToString("  ·  "), TextStyle(color = tipFg, fontSize = 11.sp, fontFamily = Inter, fontWeight = FontWeight.SemiBold))
		val pad = 6.dp.toPx()
		val bw = text.size.width + pad * 2
		val bx = (x - bw / 2).coerceIn(0f, w - bw)
		drawRoundRect(tipBg, Offset(bx, 0f), Size(bw, text.size.height + pad * 1.4f), CornerRadius(8.dp.toPx()))
		drawText(text, topLeft = Offset(bx + pad, pad * 0.7f))
	}
}

private fun DrawScope.drawSeries(s: ChartSeries, scale: Float, step: Float, w: Float, h: Float) {
	val v = s.values
	if (v.isEmpty()) return
	val line = Path()
	val area = Path()
	var open = false
	var startX = 0f
	var px = 0f
	var py = 0f
	fun close() {
		if (open && s.fill) {
			area.lineTo(px, h)
			area.lineTo(startX, h)
			area.close()
		}
		open = false
	}
	v.forEachIndexed { i, value ->
		val x = w - (v.size - 1 - i) * step
		if (value.isNaN() || x < -step) {
			close()
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
			// Through the midpoints: smooth, and never past the points themselves.
			val mx = (px + x) / 2
			val my = (py + y) / 2
			line.quadraticTo(px, py, mx, my)
			area.quadraticTo(px, py, mx, my)
		}
		px = x
		py = y
	}
	if (open) {
		line.lineTo(px, py)
		area.lineTo(px, py)
	}
	close()
	if (s.fill) {
		drawPath(area, Brush.verticalGradient(listOf(s.color.copy(alpha = 0.34f), s.color.copy(alpha = 0.0f))))
	}
	drawPath(line, s.color, style = Stroke(2.2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
	val last = v.last()
	if (!last.isNaN()) {
		val y = h - (last / scale).coerceIn(0f, 1f) * h
		drawCircle(s.color.copy(alpha = 0.25f), 7.dp.toPx(), Offset(w, y))
		drawCircle(s.color, 3.5.dp.toPx(), Offset(w, y))
	}
}

/** A compact chart without grid or labels, for tiles. */
@Composable
fun Sparkline(values: List<Float>, color: Color, capacity: Int, modifier: Modifier = Modifier, maxValue: Float? = null, binary: Boolean = false, height: Dp = 36.dp) {
	LineChart(
		listOf(ChartSeries(values, color)),
		capacity,
		modifier,
		maxValue = maxValue,
		binary = binary,
		height = height,
		grid = false,
		interactive = false,
	)
}

/** 1, 2, 2.5, 5, 10… so the scale label reads as a round number. */
fun niceCeiling(v: Float): Float {
	if (v <= 0f) return 1f
	var p = 1f
	while (p * 10 <= v) p *= 10
	while (p > v) p /= 10
	for (m in floatArrayOf(1f, 2f, 2.5f, 5f, 10f)) if (p * m >= v) return p * m
	return p * 10
}

/** The next power of two at or above [v]: 512 KiB, 1 MiB, 2 MiB… */
fun niceBinary(v: Float): Float {
	var p = 1f
	while (p < v) p *= 2
	return p
}

/** A three-quarter ring, open at the bottom, filling with a gradient and a soft glow at its tip. */
@Composable
fun RingGauge(
	fraction: Float,
	color: Color,
	modifier: Modifier = Modifier,
	size: Dp = 112.dp,
	thickness: Dp = 11.dp,
	content: @Composable () -> Unit,
) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val animate = LocalPrefs.current.animations
	val target = fraction.coerceIn(0f, 1f)
	val animated by animateFloatAsState(target, if (animate) tween(600) else tween(0), label = "gauge")
	Box(modifier.size(size), contentAlignment = Alignment.Center) {
		Canvas(Modifier.size(size)) {
			val stroke = thickness.toPx()
			val inset = stroke / 2 + 4.dp.toPx()
			val arcSize = Size(this.size.width - inset * 2, this.size.height - inset * 2)
			val tl = Offset(inset, inset)
			drawArc(track, 135f, 270f, false, tl, arcSize, style = Stroke(stroke, cap = StrokeCap.Round))
			if (animated > 0.004f) {
				val sweep = 270f * animated
				if (Build.VERSION.SDK_INT >= 28) {
					// A blurred copy under the arc reads as light coming off it.
					drawIntoCanvas { canvas ->
						val p = android.graphics.Paint().apply {
							isAntiAlias = true
							style = android.graphics.Paint.Style.STROKE
							strokeWidth = stroke
							strokeCap = android.graphics.Paint.Cap.ROUND
							this.color = color.copy(alpha = 0.45f).toArgb()
							maskFilter = BlurMaskFilter(8.dp.toPx(), BlurMaskFilter.Blur.NORMAL)
						}
						canvas.nativeCanvas.drawArc(tl.x, tl.y, tl.x + arcSize.width, tl.y + arcSize.height, 135f, sweep, false, p)
					}
				}
				// The sweep gradient starts at 3 o'clock; turning it puts its dark end at the ring's start.
				rotate(135f) {
					drawArc(
						Brush.sweepGradient(0f to color.copy(alpha = 0.55f), (sweep / 360f).coerceAtLeast(0.01f) to color),
						0f,
						sweep,
						false,
						tl,
						arcSize,
						style = Stroke(stroke, cap = StrokeCap.Round),
					)
				}
			}
		}
		content()
	}
}

@Composable
fun UsageBar(fraction: Float, color: Color, modifier: Modifier = Modifier, height: Dp = 8.dp) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val animate = LocalPrefs.current.animations
	val animated by animateFloatAsState(fraction.coerceIn(0f, 1f), if (animate) tween(500) else tween(0), label = "bar")
	Canvas(modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2))) {
		drawRect(track)
		if (animated > 0f) {
			drawRoundRect(
				Brush.horizontalGradient(listOf(color.copy(alpha = 0.7f), color), endX = size.width * animated),
				size = Size(size.width * animated, size.height),
				cornerRadius = CornerRadius(size.height / 2),
			)
		}
	}
}

/** One thin column per CPU core. */
@Composable
fun CoreBars(values: List<Float>, modifier: Modifier = Modifier, height: Dp = 44.dp) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val base = Metric.colors.cpu
	val colors = values.map { loadColor(it / 100f, base) }
	val animate = LocalPrefs.current.animations
	Row(modifier.fillMaxWidth().height(height), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
		values.forEachIndexed { i, v ->
			val animated by animateFloatAsState((v / 100f).coerceIn(0f, 1f), if (animate) tween(500) else tween(0), label = "core")
			Canvas(Modifier.weight(1f).fillMaxHeight()) {
				val r = CornerRadius(4.dp.toPx())
				drawRoundRect(track, cornerRadius = r)
				val bh = size.height * animated
				if (bh > 0.5f) {
					drawRoundRect(
						Brush.verticalGradient(listOf(colors[i], colors[i].copy(alpha = 0.6f)), startY = size.height - bh, endY = size.height),
						topLeft = Offset(0f, size.height - bh),
						size = Size(size.width, bh),
						cornerRadius = r,
					)
				}
			}
		}
	}
}

/** Text whose characters roll to their new values, like a mechanical counter. */
@Composable
fun RollingText(text: String, style: TextStyle, modifier: Modifier = Modifier, color: Color = Color.Unspecified) {
	if (!LocalPrefs.current.animations) {
		Text(text, style = style, color = color, modifier = modifier, maxLines = 1)
		return
	}
	Row(modifier) {
		text.forEach { ch ->
			AnimatedContent(
				targetState = ch,
				transitionSpec = {
					val up = targetState > initialState
					(slideInVertically(tween(260)) { if (up) it else -it } + fadeIn(tween(260)))
						.togetherWith(slideOutVertically(tween(260)) { if (up) -it else it } + fadeOut(tween(200)))
				},
				label = "digit",
			) { c ->
				Text(c.toString(), style = style, color = color, maxLines = 1)
			}
		}
	}
}
