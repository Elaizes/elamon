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
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.max
import ua.elamon.Points
import ua.elamon.lastTime

class ChartSeries(
	val points: Points,
	val color: Color,
	val fill: Boolean = true,
	/** How the value reads when someone touches the chart. */
	val format: ((Float) -> String)? = null,
)

/**
 * A time chart that ends at the newest point on the right and spans the series' window, so a
 * point sits where its moment falls whatever the update interval. A long pause breaks the line.
 * Dragging a finger along it shows the values at the nearest moment. [maxValue] fixes the scale
 * (100 for percentages); otherwise it follows the data with headroom, to round numbers ([binary]
 * for bytes).
 */
@Composable
fun LineChart(
	series: List<ChartSeries>,
	modifier: Modifier = Modifier,
	maxValue: Float? = null,
	minScale: Float = 1f,
	binary: Boolean = false,
	height: Dp = 120.dp,
	grid: Boolean = true,
	label: ((Float) -> String)? = null,
	interactive: Boolean = true,
) {
	val gridColor = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.45f)
	val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
	val tipBg = MaterialTheme.colorScheme.inverseSurface
	val tipFg = MaterialTheme.colorScheme.inverseOnSurface
	val measurer = rememberTextMeasurer()
	val res = resources()
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
		val w = size.width
		val h = size.height
		val tLast = series.maxOfOrNull { it.points.lastTime() } ?: Long.MIN_VALUE
		val window = series.firstOrNull()?.points?.windowMs ?: 1L
		val tStart = tLast - window
		var top = 0f
		if (tLast != Long.MIN_VALUE) {
			for (s in series) {
				val p = s.points
				for (i in firstVisibleIndex(p, tStart) until p.size) {
					val v = p.v(i)
					if (!v.isNaN() && v > top) top = v
				}
			}
		}
		val wanted = max(top * 1.15f, minScale)
		val scale = maxValue ?: if (binary) niceBinary(wanted) else niceCeiling(wanted)
		if (grid) {
			val dash = PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 5.dp.toPx()))
			for (i in 1..3) {
				val y = h * i / 4f
				drawLine(gridColor, Offset(0f, y), Offset(w, y), strokeWidth = 1.dp.toPx(), pathEffect = dash)
			}
		}
		if (tLast != Long.MIN_VALUE) {
			// The line entering from the left stops at the edge; the newest dot's halo may stick out.
			val halo = 8.dp.toPx()
			clipRect(top = -halo, right = w + halo, bottom = h + halo) {
				for (s in series) drawSeries(s, scale, tLast, window, w, h)
			}
		}
		if (label != null && grid) {
			drawText(measurer, label(scale), topLeft = Offset(2.dp.toPx(), 0f), style = TextStyle(color = labelColor, fontSize = 10.sp, fontFamily = Inter))
		}

		val tx = touchX ?: return@Canvas
		val ref = series.firstOrNull { it.points.size > 0 }?.points ?: return@Canvas
		val i = nearestIndex(ref, timeAtX(tx, tLast, window, w))
		val t = ref.t(i)
		val x = chartX(t, tLast, window, w).coerceIn(0f, w)
		drawLine(labelColor.copy(alpha = 0.6f), Offset(x, 0f), Offset(x, h), strokeWidth = 1.5.dp.toPx())
		val parts = mutableListOf(Fmt.ago(res, ((tLast - t) / 1000).coerceAtLeast(0)))
		for (s in series) {
			val p = s.points
			val j = if (p === ref) i else nearestIndex(p, t)
			if (j < 0 || p.t(j) != t) continue
			val v = p.v(j)
			if (v.isNaN()) continue
			val y = h - (v / scale).coerceIn(0f, 1f) * h
			drawCircle(s.color, 5.dp.toPx(), Offset(x, y))
			drawCircle(tipFg, 2.dp.toPx(), Offset(x, y))
			s.format?.let { parts += it(v) }
		}
		val pad = 6.dp.toPx()
		// At a large font the label may not fit on one line: it wraps inside the chart instead.
		val text = measurer.measure(
			parts.joinToString("  ·  "),
			TextStyle(color = tipFg, fontSize = 11.sp, fontFamily = Inter, fontWeight = FontWeight.SemiBold),
			constraints = Constraints(maxWidth = (w - pad * 2).toInt().coerceAtLeast(1)),
		)
		val bw = text.size.width + pad * 2
		val bx = tooltipLeft(x, bw, w)
		drawRoundRect(tipBg, Offset(bx, 0f), Size(bw, text.size.height + pad * 1.4f), CornerRadius(8.dp.toPx()))
		drawText(text, topLeft = Offset(bx + pad, pad * 0.7f))
	}
}

private fun DrawScope.drawSeries(s: ChartSeries, scale: Float, tLast: Long, window: Long, w: Float, h: Float) {
	val p = s.points
	if (p.size == 0) return
	val gap = p.gapMs
	val line = Path()
	val area = Path()
	var open = false
	var startX = 0f
	var px = 0f
	var py = 0f
	var pt = 0L
	fun close() {
		if (!open) return
		// The smoothing stops halfway to the last point; finish there. A lone point becomes a dot.
		line.lineTo(px, py)
		if (s.fill) {
			area.lineTo(px, py)
			area.lineTo(px, h)
			area.lineTo(startX, h)
			area.close()
		}
		open = false
	}
	for (i in firstVisibleIndex(p, tLast - window) until p.size) {
		val t = p.t(i)
		val value = p.v(i)
		if (value.isNaN()) {
			close()
			continue
		}
		if (open && isGap(pt, t, gap)) close()
		val x = chartX(t, tLast, window, w)
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
		pt = t
	}
	close()
	if (s.fill) {
		drawPath(area, Brush.verticalGradient(listOf(s.color.copy(alpha = 0.34f), s.color.copy(alpha = 0.0f))))
	}
	drawPath(line, s.color, style = Stroke(2.2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
	val last = p.v(p.size - 1)
	if (!last.isNaN()) {
		val x = chartX(p.t(p.size - 1), tLast, window, w)
		val y = h - (last / scale).coerceIn(0f, 1f) * h
		drawCircle(s.color.copy(alpha = 0.25f), 7.dp.toPx(), Offset(x, y))
		drawCircle(s.color, 3.5.dp.toPx(), Offset(x, y))
	}
}

/** A compact chart without grid or labels, for tiles. */
@Composable
fun Sparkline(points: Points, color: Color, modifier: Modifier = Modifier, maxValue: Float? = null, binary: Boolean = false, height: Dp = 36.dp) {
	LineChart(
		listOf(ChartSeries(points, color)),
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
