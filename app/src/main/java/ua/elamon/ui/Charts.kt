package ua.elamon.ui

import android.content.res.Resources
import android.graphics.BlurMaskFilter
import android.os.Build
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import ua.elamon.Points
import ua.elamon.R
import ua.elamon.lastTime

class ChartSeries(
	val points: Points,
	val color: Color,
	val fill: Boolean = true,
	/** What the line is, for the chart's spoken summary: "CPU", "Download". */
	val name: String? = null,
	/** A value alone, for that summary: "54°C". */
	val value: ((Float) -> String)? = null,
	/** How the value reads when someone touches the chart; the value alone unless given. */
	val format: ((Float) -> String)? = value,
)

/**
 * A time chart that ends at the newest point on the right and spans the series' window, so a
 * point sits where its moment falls whatever the update interval. A long pause breaks the line.
 * Dragging a finger along it shows the values at the nearest moment. [maxValue] fixes the scale
 * (100 for percentages); otherwise it follows the data with headroom, to round numbers ([binary]
 * for bytes) unless [round] is off. A screen reader hears each named series' newest value and peak.
 */
@Composable
fun LineChart(
	series: List<ChartSeries>,
	modifier: Modifier = Modifier,
	maxValue: Float? = null,
	minScale: Float = 1f,
	binary: Boolean = false,
	round: Boolean = true,
	height: Dp = ChartStyle.height,
	grid: Boolean = true,
	label: ((Float) -> String)? = null,
	interactive: Boolean = true,
) {
	val gridColor = MaterialTheme.colorScheme.outlineVariant.copy(alpha = Alpha.GRID)
	val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
	val tipBg = MaterialTheme.colorScheme.inverseSurface
	val tipFg = MaterialTheme.colorScheme.inverseOnSurface
	val axisStyle = ChartStyle.axisText.copy(color = labelColor)
	val tipStyle = ChartStyle.tooltipText.copy(color = tipFg)
	val measurer = rememberTextMeasurer()
	val res = resources()
	val density = LocalDensity.current
	val cache = remember(density) { ChartCache(density) }
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

	// Worked out only when something asks, so a phone without a screen reader doesn't pay for it.
	val summary = Modifier.semantics { chartDescription(res, series)?.let { contentDescription = it } }

	Canvas(modifier.fillMaxWidth().height(height).then(touch).then(summary)) {
		val w = size.width
		val h = size.height
		var tLast = Long.MIN_VALUE
		for (k in series.indices) tLast = maxOf(tLast, series[k].points.lastTime())
		val window = series.firstOrNull()?.points?.windowMs ?: 1L
		val tStart = tLast - window
		var top = 0f
		if (tLast != Long.MIN_VALUE) {
			for (k in series.indices) {
				val p = series[k].points
				for (i in firstVisibleIndex(p, tStart) until p.size) {
					val v = p.v(i)
					if (!v.isNaN() && v > top) top = v
				}
			}
		}
		val scale = chartScale(top, maxValue, minScale, binary, round)
		if (grid) {
			val rows = Thresholds.CHART_GRID_ROWS
			for (i in 1 until rows) {
				val y = h * i / rows
				drawLine(gridColor, Offset(0f, y), Offset(w, y), strokeWidth = ChartStyle.grid.toPx(), pathEffect = cache.dash)
			}
		}
		if (tLast != Long.MIN_VALUE) {
			// The line entering from the left stops at the edge; the newest dot's halo may stick out.
			val room = ChartStyle.haloRoom.toPx()
			clipRect(top = -room, right = w + room, bottom = h + room) {
				for (k in series.indices) drawSeries(series[k], cache.tracer(k), cache.line, scale, tLast, window, w, h)
			}
		}
		if (label != null && grid) {
			drawText(measurer, label(scale), topLeft = Offset(ChartStyle.labelInset.toPx(), 0f), style = axisStyle)
		}

		val tx = touchX ?: return@Canvas
		val ref = series.firstOrNull { it.points.size > 0 }?.points ?: return@Canvas
		val i = nearestIndex(ref, timeAtX(tx, tLast, window, w))
		val t = ref.t(i)
		val x = chartX(t, tLast, window, w).coerceIn(0f, w)
		drawLine(labelColor.copy(alpha = Alpha.CROSSHAIR), Offset(x, 0f), Offset(x, h), strokeWidth = ChartStyle.crosshair.toPx())
		val tip = cache.tip
		tip.setLength(0)
		tip.append(Fmt.ago(res, ((tLast - t) / 1000).coerceAtLeast(0)))
		for (k in series.indices) {
			val s = series[k]
			val p = s.points
			val j = if (p === ref) i else nearestIndex(p, t)
			if (j < 0 || p.t(j) != t) continue
			val v = p.v(j)
			if (v.isNaN()) continue
			val y = h - (v / scale).coerceIn(0f, 1f) * h
			drawCircle(s.color, ChartStyle.touchDot.toPx(), Offset(x, y))
			drawCircle(tipFg, ChartStyle.touchDotCore.toPx(), Offset(x, y))
			s.format?.let { tip.append(TIP_SEPARATOR).append(it(v)) }
		}
		val pad = ChartStyle.tooltipPadding.toPx()
		// At a large font the label may not fit on one line: it wraps inside the chart instead.
		val text = measurer.measure(
			tip.toString(),
			tipStyle,
			constraints = Constraints(maxWidth = (w - pad * 2).toInt().coerceAtLeast(1)),
		)
		val bw = text.size.width + pad * 2
		val bx = tooltipLeft(x, bw, w)
		drawRoundRect(tipBg, Offset(bx, 0f), Size(bw, text.size.height + pad * TIP_HEIGHT_PADS), CornerRadius(Radius.s.toPx()))
		drawText(text, topLeft = Offset(bx + pad, pad * TIP_TEXT_TOP))
	}
}

/** Between the parts of a chart's touch label. */
private const val TIP_SEPARATOR = "  ·  "

/** The touch label's box: a padding and a bit above and below its text, the text a bit under its top. */
private const val TIP_HEIGHT_PADS = 1.4f
private const val TIP_TEXT_TOP = 0.7f

/** What a chart keeps from frame to frame instead of making it anew. */
private class ChartCache(density: Density) {
	val dash = with(density) { PathEffect.dashPathEffect(floatArrayOf(ChartStyle.dashOn.toPx(), ChartStyle.dashOff.toPx())) }
	val line = with(density) { Stroke(ChartStyle.line.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round) }
	val tip = StringBuilder()
	private val tracers = ArrayList<Tracer>()

	fun tracer(i: Int): Tracer {
		while (tracers.size <= i) tracers += Tracer()
		return tracers[i]
	}
}

/** One series' line and the area under it, built again every frame in the same paths. */
private class Tracer {
	val line = Path()
	val area = Path()
	var isOpen = false
		private set
	private var startX = 0f
	private var px = 0f
	private var py = 0f
	private var bottom = 0f
	private var withArea = true
	private var fillColor = Color.Unspecified
	private var fill: Brush? = null

	fun begin(height: Float, area: Boolean) {
		line.rewind()
		this.area.rewind()
		isOpen = false
		bottom = height
		withArea = area
	}

	fun point(x: Float, y: Float) {
		if (!isOpen) {
			line.moveTo(x, y)
			if (withArea) {
				area.moveTo(x, bottom)
				area.lineTo(x, y)
			}
			startX = x
			isOpen = true
		} else {
			// Through the midpoints: smooth, and never past the points themselves.
			val mx = (px + x) / 2
			val my = (py + y) / 2
			line.quadraticTo(px, py, mx, my)
			if (withArea) area.quadraticTo(px, py, mx, my)
		}
		px = x
		py = y
	}

	/** Ends a stretch: the smoothing stops halfway to the last point, so it finishes there. A lone point becomes a dot. */
	fun end() {
		if (!isOpen) return
		line.lineTo(px, py)
		if (withArea) {
			area.lineTo(px, py)
			area.lineTo(px, bottom)
			area.lineTo(startX, bottom)
			area.close()
		}
		isOpen = false
	}

	/** The fade under a line of [color], made again only when the colour changes. */
	fun fillFor(color: Color): Brush {
		val f = fill
		if (f != null && color == fillColor) return f
		return Brush.verticalGradient(listOf(color.copy(alpha = Alpha.FILL), color.copy(alpha = 0f))).also {
			fill = it
			fillColor = color
		}
	}
}

private fun DrawScope.drawSeries(s: ChartSeries, tr: Tracer, stroke: Stroke, scale: Float, tLast: Long, window: Long, w: Float, h: Float) {
	val p = s.points
	if (p.size == 0) return
	val gap = p.gapMs
	tr.begin(h, s.fill)
	var pt = 0L
	for (i in firstVisibleIndex(p, tLast - window) until p.size) {
		val t = p.t(i)
		val value = p.v(i)
		if (value.isNaN()) {
			tr.end()
			continue
		}
		if (tr.isOpen && isGap(pt, t, gap)) tr.end()
		tr.point(chartX(t, tLast, window, w), h - (value / scale).coerceIn(0f, 1f) * h)
		pt = t
	}
	tr.end()
	if (s.fill) drawPath(tr.area, tr.fillFor(s.color))
	drawPath(tr.line, s.color, style = stroke)
	val last = p.v(p.size - 1)
	if (!last.isNaN()) {
		val x = chartX(p.t(p.size - 1), tLast, window, w)
		val y = h - (last / scale).coerceIn(0f, 1f) * h
		drawCircle(s.color.copy(alpha = Alpha.HALO), ChartStyle.halo.toPx(), Offset(x, y))
		drawCircle(s.color, ChartStyle.dot.toPx(), Offset(x, y))
	}
}

/** What a chart says to a screen reader: each named series' newest value and its peak, over the time it spans. */
private fun chartDescription(res: Resources, series: List<ChartSeries>): String? {
	val first = series.firstOrNull() ?: return null
	val tLast = series.maxOf { it.points.lastTime() }
	val window = first.points.windowMs
	val parts = series.mapNotNull { s ->
		val name = s.name ?: return@mapNotNull null
		val value = s.value ?: return@mapNotNull null
		val now = s.points.latest()
		if (now.isNaN()) res.getString(R.string.a11y_chart_series_empty, name)
		else res.getString(R.string.a11y_chart_series, name, value(now), value(s.points.peak(tLast - window)))
	}
	if (parts.isEmpty()) return null
	return res.getString(R.string.a11y_chart, Fmt.duration(res, window / 1000), parts.joinToString(res.getString(R.string.a11y_list_separator)))
}

/** A compact chart without grid or labels, for tiles; [name] and [value] are what a screen reader hears. */
@Composable
fun Sparkline(
	points: Points,
	color: Color,
	modifier: Modifier = Modifier,
	maxValue: Float? = null,
	minScale: Float = 1f,
	round: Boolean = true,
	binary: Boolean = false,
	height: Dp = ChartStyle.sparkline,
	name: String? = null,
	value: ((Float) -> String)? = null,
) {
	LineChart(
		listOf(ChartSeries(points, color, name = name, value = value)),
		modifier,
		maxValue = maxValue,
		minScale = minScale,
		binary = binary,
		round = round,
		height = height,
		grid = false,
		interactive = false,
	)
}

/** A three-quarter ring, open at the bottom, filling with a gradient and a soft glow at its tip. */
@Composable
fun RingGauge(
	fraction: Float,
	color: Color,
	modifier: Modifier = Modifier,
	size: Dp = Gauge.size,
	thickness: Dp = Gauge.thickness,
	/** What a screen reader hears instead of what's inside the ring. */
	description: String? = null,
	content: @Composable () -> Unit,
) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val animate = LocalPrefs.current.animations
	val target = fraction.coerceIn(0f, 1f)
	val animated by animateFloatAsState(target, tween(if (animate) Motion.GAUGE else 0), label = "gauge")
	val density = LocalDensity.current
	val strokePx = with(density) { thickness.toPx() }
	val ring = remember(strokePx) { Stroke(strokePx, cap = StrokeCap.Round) }
	// A blurred copy under the arc reads as light coming off it; its paint is made once, not every frame.
	val glow = remember(strokePx, density) {
		if (Build.VERSION.SDK_INT >= 28) {
			android.graphics.Paint().apply {
				isAntiAlias = true
				style = android.graphics.Paint.Style.STROKE
				strokeWidth = strokePx
				strokeCap = android.graphics.Paint.Cap.ROUND
				maskFilter = BlurMaskFilter(with(density) { Gauge.glow.toPx() }, BlurMaskFilter.Blur.NORMAL)
			}
		} else null
	}
	val said = if (description != null) Modifier.clearAndSetSemantics { contentDescription = description } else Modifier
	Box(modifier.size(size).then(said), contentAlignment = Alignment.Center) {
		Canvas(Modifier.size(size)) {
			val inset = strokePx / 2 + Gauge.inset.toPx()
			val arcSize = Size(this.size.width - inset * 2, this.size.height - inset * 2)
			val tl = Offset(inset, inset)
			drawArc(track, Gauge.START, Gauge.SWEEP, false, tl, arcSize, style = ring)
			if (animated > Gauge.MIN_SHOWN) {
				val sweep = Gauge.SWEEP * animated
				if (glow != null) {
					glow.color = color.copy(alpha = Alpha.GLOW).toArgb()
					drawIntoCanvas { canvas ->
						canvas.nativeCanvas.drawArc(tl.x, tl.y, tl.x + arcSize.width, tl.y + arcSize.height, Gauge.START, sweep, false, glow)
					}
				}
				// The sweep gradient starts at 3 o'clock; turning it puts its dark end at the ring's start.
				rotate(Gauge.START) {
					drawArc(
						Brush.sweepGradient(0f to color.copy(alpha = Alpha.RING_START), (sweep / FULL_TURN).coerceAtLeast(MIN_STOP) to color),
						0f,
						sweep,
						false,
						tl,
						arcSize,
						style = ring,
					)
				}
			}
		}
		content()
	}
}

private const val FULL_TURN = 360f

/** The gradient's second stop never sits on its first, which would make it a hard edge. */
private const val MIN_STOP = 0.01f

@Composable
fun UsageBar(fraction: Float, color: Color, modifier: Modifier = Modifier, height: Dp = Sizes.bar) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val animate = LocalPrefs.current.animations
	val animated by animateFloatAsState(fraction.coerceIn(0f, 1f), tween(if (animate) Motion.BAR else 0), label = "bar")
	Canvas(modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2))) {
		drawRect(track)
		if (animated > 0f) {
			drawRoundRect(
				Brush.horizontalGradient(listOf(color.copy(alpha = Alpha.BAR_START), color), endX = size.width * animated),
				size = Size(size.width * animated, size.height),
				cornerRadius = CornerRadius(size.height / 2),
			)
		}
	}
}

/** One thin column per CPU core; a screen reader hears how many, their average and the busiest. */
@Composable
fun CoreBars(values: List<Float>, modifier: Modifier = Modifier, height: Dp = ChartStyle.cores) {
	val track = MaterialTheme.colorScheme.surfaceContainerHighest
	val base = Metric.colors.cpu
	val colors = values.map { loadColor(it / Thresholds.PERCENT, base) }
	val animate = LocalPrefs.current.animations
	val res = resources()
	val summary = Modifier.semantics {
		if (values.isNotEmpty()) {
			contentDescription = res.getQuantityString(
				R.plurals.a11y_cores, values.size, values.size, Fmt.percent(values.average().toFloat()), Fmt.percent(values.max()),
			)
		}
	}
	Row(modifier.fillMaxWidth().height(height).then(summary), horizontalArrangement = Arrangement.spacedBy(ChartStyle.coreGap)) {
		values.forEachIndexed { i, v ->
			val animated by animateFloatAsState((v / Thresholds.PERCENT).coerceIn(0f, 1f), tween(if (animate) Motion.BAR else 0), label = "core")
			Canvas(Modifier.weight(1f).fillMaxHeight()) {
				val r = CornerRadius(Radius.xs.toPx())
				drawRoundRect(track, cornerRadius = r)
				val bh = size.height * animated
				if (bh > ChartStyle.MIN_PX) {
					drawRoundRect(
						Brush.verticalGradient(listOf(colors[i], colors[i].copy(alpha = Alpha.FADE_END)), startY = size.height - bh, endY = size.height),
						topLeft = Offset(0f, size.height - bh),
						size = Size(size.width, bh),
						cornerRadius = r,
					)
				}
			}
		}
	}
}

/** What a rolling text showed before its last change, and what it shows now. */
private class Rolled(var text: String) {
	var from: String = text
}

/**
 * Text whose characters roll to their new values, like a mechanical counter. Only the characters
 * that changed move, all on one animation read while drawing; a screen reader hears it whole.
 */
@Composable
fun RollingText(text: String, style: TextStyle, modifier: Modifier = Modifier, color: Color = Color.Unspecified) {
	val whole = modifier.clearAndSetSemantics { contentDescription = text }
	if (!LocalPrefs.current.animations) {
		Text(text, style = style, color = color, modifier = whole, maxLines = 1)
		return
	}
	val rolled = remember { Rolled(text) }
	// A new text starts a roll from 0; the same one keeps the roll it has.
	val progress = remember(text) {
		if (rolled.text != text) {
			rolled.from = rolled.text
			rolled.text = text
			Animatable(0f)
		} else {
			Animatable(1f)
		}
	}
	LaunchedEffect(progress) { progress.animateTo(1f, tween(Motion.DIGIT)) }
	val rolling by remember(progress) { derivedStateOf { progress.value < 1f } }
	val from = rolled.from
	Row(whole) {
		text.forEachIndexed { i, ch ->
			val old = from.getOrNull(i)
			if (rolling && old != null && old != ch) RollingChar(old, ch, progress, style, color)
			else Text(ch.toString(), style = style, color = color, maxLines = 1)
		}
	}
}

/** [from] rolling out and [to] rolling in, up when the new character is higher, clipped to its place. */
@Composable
private fun RollingChar(from: Char, to: Char, progress: Animatable<Float, AnimationVector1D>, style: TextStyle, color: Color) {
	val up = to > from
	Box(Modifier.clipToBounds()) {
		Text(
			from.toString(),
			style = style,
			color = color,
			maxLines = 1,
			modifier = Modifier.graphicsLayer {
				val p = progress.value
				translationY = (if (up) -p else p) * size.height
				// The old character is gone a little before the new one has settled.
				alpha = (1f - p * Motion.DIGIT.toFloat() / Motion.DIGIT_FADE).coerceIn(0f, 1f)
			},
		)
		Text(
			to.toString(),
			style = style,
			color = color,
			maxLines = 1,
			modifier = Modifier.graphicsLayer {
				val p = progress.value
				translationY = (if (up) 1f - p else p - 1f) * size.height
				alpha = p
			},
		)
	}
}
