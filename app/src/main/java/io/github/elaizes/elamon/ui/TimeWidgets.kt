package io.github.elaizes.elamon.ui

import android.content.Context
import android.text.format.DateFormat
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import java.text.SimpleDateFormat
import java.time.Instant
import java.time.ZoneId
import java.util.Date
import java.util.Locale
import io.github.elaizes.elamon.HistoryRange
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.RangeChip
import io.github.elaizes.elamon.RangeView
import io.github.elaizes.elamon.SpanUnit
import io.github.elaizes.elamon.data.Segment
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.rangeChips
import io.github.elaizes.elamon.spanParts

/** The chips over a graph or timeline: its own range, live, and the long ones; tapping one shows it. */
@Composable
fun RangeChips(chips: List<RangeChip>, selected: HistoryRange?, onSelect: (HistoryRange?) -> Unit) {
	Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
		for (c in chips) {
			FilterChip(
				selected = c.range == selected,
				onClick = { if (c.range != selected) onSelect(c.range) },
				label = { Text(rangeLabel(c)) },
				shape = PillShape,
			)
		}
	}
}

/** What a chip says: "10 min", "24 h", "7 days", "Year", "All". */
@Composable
fun rangeLabel(chip: RangeChip): String = when (chip.range) {
	HistoryRange.Year -> stringResource(R.string.range_year)
	HistoryRange.All -> stringResource(R.string.range_all)
	else -> spanLabel(chip.ms)
}

/** A span as a chip says it: "30 s", "10 min", "24 h", "7 days". */
@Composable
fun spanLabel(ms: Long): String {
	val (unit, n) = spanParts(ms)
	val count = n.toInt()
	return when (unit) {
		SpanUnit.Seconds -> stringResource(R.string.range_seconds, count)
		SpanUnit.Minutes -> stringResource(R.string.range_minutes, count)
		SpanUnit.Hours -> stringResource(R.string.range_hours, count)
		SpanUnit.Days -> pluralStringResource(R.plurals.range_days, count, count)
	}
}

/** How a time axis writes its moments, the phone's way: its clock (12 or 24 hours), its order of day and month. */
private class TickFormats(context: Context, locale: Locale) {
	private val time = DateFormat.getTimeFormat(context)
	private val day = SimpleDateFormat(DateFormat.getBestDateTimePattern(locale, "dMMM"), locale)
	private val month = SimpleDateFormat(DateFormat.getBestDateTimePattern(locale, "MMM"), locale)
	private val year = SimpleDateFormat(DateFormat.getBestDateTimePattern(locale, "yyyy"), locale)

	fun of(unit: TickUnit, ms: Long): String = when (unit) {
		TickUnit.Time -> time
		TickUnit.Day -> day
		TickUnit.Month -> month
		TickUnit.Year -> year
	}.format(Date(ms))
}

/**
 * The moments from [start] to [end] (Unix ms) under a chart or a timeline as wide: labels at round
 * times of the phone's clock, as many as fit without touching.
 */
@Composable
fun TimeAxis(start: Long, end: Long, modifier: Modifier = Modifier) {
	val style = ChartStyle.axisText.copy(color = MaterialTheme.colorScheme.onSurfaceVariant)
	val measurer = rememberTextMeasurer()
	val context = LocalContext.current
	val locale = LocalConfiguration.current.locales[0]
	val formats = remember(context, locale) { TickFormats(context, locale) }
	val height = with(LocalDensity.current) { remember(measurer, style) { measurer.measure("0", style).size.height }.toDp() }
	Canvas(modifier.fillMaxWidth().height(height)) {
		val w = size.width
		val ticks = timeTicks(start, end, (w / ChartStyle.axisSpacing.toPx()).toInt(), ZoneId.systemDefault())
		val n = ticks.times.size
		if (n == 0) return@Canvas
		val texts = ticks.times.map { measurer.measure(formats.of(ticks.unit, it), style) }
		val xs = FloatArray(n) { chartX(ticks.times[it], end, end - start, w) }
		val widths = FloatArray(n) { texts[it].size.width.toFloat() }
		for ((i, left) in axisLabels(xs, widths, w, ChartStyle.axisGap.toPx())) drawText(texts[i], topLeft = Offset(left, 0f))
	}
}

/** A moment a finger reads off a long range: the date and time, the phone's way. */
fun moment(ms: Long): String = Fmt.time(ms.toDouble(), millis = true)

/** A line under the chips while a long range is on its way ([has] none of it yet), or when the computer couldn't give it. */
@Composable
fun RangeNote(view: RangeView?, chips: List<RangeChip>, has: Boolean) {
	val chip = view?.let { v -> chips.firstOrNull { it.range == v.range } } ?: return
	val text = when {
		view.failed -> stringResource(R.string.range_failed, rangeLabel(chip))
		view.loading && !has -> stringResource(R.string.range_loading)
		else -> return
	}
	Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

// ---- timeline -------------------------------------------------------------------------------------

/** A timeline's range while the service doesn't say: a day. */
private const val TIMELINE_DEFAULT_S = 86_400

/** The states a timeline draws, in the order its legend has them. */
private val TIMELINE_STATES = listOf("line", "battery", "none")

/** The colour of a timeline's state: on line good, on battery a warning, nothing known (or a state this app doesn't know) muted. */
@Composable
private fun stateColor(state: String): Color = when (state) {
	"line" -> Metric.colors.good
	"battery" -> Metric.colors.warn
	else -> MaterialTheme.colorScheme.surfaceContainerHighest
}

@Composable
private fun stateLabel(state: String): String = stringResource(
	when (state) {
		"line" -> R.string.timeline_line
		"battery" -> R.string.timeline_battery
		else -> R.string.timeline_none
	},
)

/**
 * When a source was on line, on battery, or nothing was known, as a bar over its range: live, the
 * stream's segments; or a long range, the computer's, carried on to now by the stream's. Under
 * it, the time and how long each state lasted; a finger on it reads one stretch.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TimelineWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val liveMs = (w.range?.takeIf { it > 0 } ?: TIMELINE_DEFAULT_S) * MS_PER_SECOND
	val chips = rangeChips(liveMs, w.ranges)
	val view = scope.machine.ranges[w.id]?.takeIf { v -> chips.any { it.range == v.range } }
	val long = view?.takeUnless { it.failed }
	val live = s?.segments.orEmpty()
	val history = long?.timeline
	val segments = when {
		long == null -> live
		history != null -> splice(history.segments, history.to, live)
		else -> emptyList()
	}
	val end = maxOf(segments.lastOrNull()?.to ?: 0L, history?.to ?: 0L)
	val start = if (history != null && long.range == HistoryRange.All) history.from else end - (long?.range?.ms ?: liveMs)
	val span = long?.let { v -> chips.firstOrNull { it.range == v.range }?.let { rangeLabel(it) } } ?: spanLabel(liveMs)
	val totals = if (segments.isEmpty()) emptyMap() else timeIn(segments, start, end)
	var touched by remember { mutableStateOf<Segment?>(null) }
	val res = resources()
	WidgetSection(scope, w, accent(w.color), modifier) {
		if (chips.isNotEmpty()) RangeChips(chips, long?.range) { scope.vm.showRange(w, it) }
		RangeNote(view, chips, has = history != null)
		// Each state there is in the range, and how long it lasted: "On battery · 12 min".
		val legend = TIMELINE_STATES.mapNotNull { st -> totals[st]?.let { st to stringResource(R.string.timeline_legend, stateLabel(st), Fmt.duration(it / MS_PER_SECOND)) } }
		val description = stringResource(R.string.a11y_chart, span, legend.joinToString(res.getString(R.string.a11y_list_separator)) { it.second })
		TimelineBar(segments, start, end, description) { touched = it }
		if (segments.isNotEmpty()) TimeAxis(start, end)
		val t = touched
		if (t != null) {
			Text(stretchText(t), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurface)
		} else if (legend.isNotEmpty()) {
			FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.l), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
				for ((st, text) in legend) Legend(stateColor(st), text)
			}
		}
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
	}
}

/** One stretch as a finger reads it: "On battery · 16/11/2026, 14:02 – 14:14 · 12 min". */
@Composable
private fun stretchText(s: Segment): String {
	val context = LocalContext.current
	val sameDay = remember(s) {
		val zone = ZoneId.systemDefault()
		Instant.ofEpochMilli(s.from).atZone(zone).toLocalDate() == Instant.ofEpochMilli(s.to).atZone(zone).toLocalDate()
	}
	val to = if (sameDay) DateFormat.getTimeFormat(context).format(Date(s.to)) else moment(s.to)
	return stringResource(R.string.timeline_touched, stateLabel(s.state), moment(s.from), to, Fmt.duration((s.to - s.from) / MS_PER_SECOND))
}

/**
 * The bar itself: [segments] from [start] to [end], each stretch at least a hair wide; [onTouch]
 * hears the stretch under a finger, and null when it lifts. A screen reader hears [description].
 */
@Composable
private fun TimelineBar(segments: List<Segment>, start: Long, end: Long, description: String, onTouch: (Segment?) -> Unit) {
	val colors = TIMELINE_STATES.associateWith { stateColor(it) }
	val muted = stateColor("none")
	val cross = MaterialTheme.colorScheme.onSurface.copy(alpha = Alpha.CROSSHAIR)
	var touchX by remember { mutableStateOf<Float?>(null) }
	val current by rememberUpdatedState(Triple(segments, start, end))
	val report by rememberUpdatedState(onTouch)
	val density = LocalDensity.current
	val minPx = with(density) { ChartStyle.stretchMin.toPx() }
	fun at(x: Float?, width: Float) {
		touchX = x
		val (segs, a, b) = current
		report(x?.let { stretches(segs, a, b, width, minPx).let { all -> all.getOrNull(stretchAt(all, it))?.segment } })
	}
	Canvas(
		Modifier
			.fillMaxWidth()
			.height(ChartStyle.timeline)
			.clip(RoundedCornerShape(Radius.xs))
			.pointerInput(Unit) {
				detectHorizontalDragGestures(
					onDragStart = { at(it.x, size.width.toFloat()) },
					onDragEnd = { at(null, size.width.toFloat()) },
					onDragCancel = { at(null, size.width.toFloat()) },
					onHorizontalDrag = { change, _ -> at(change.position.x, size.width.toFloat()) },
				)
			}
			.pointerInput(Unit) {
				detectTapGestures(onPress = {
					at(it.x, size.width.toFloat())
					tryAwaitRelease()
					at(null, size.width.toFloat())
				})
			}
			.clearAndSetSemantics { contentDescription = description },
	) {
		drawRect(muted)
		val all = stretches(segments, start, end, size.width, minPx)
		// The short ones, widened to be seen, over their neighbours.
		for (wide in listOf(false, true)) {
			for (st in all) {
				if (st.widened != wide) continue
				drawRect(colors[st.segment.state] ?: muted, Offset(st.x0, 0f), Size(st.x1 - st.x0, size.height))
			}
		}
		touchX?.let { x -> drawLine(cross, Offset(x, 0f), Offset(x, size.height), strokeWidth = ChartStyle.crosshair.toPx()) }
	}
}

private const val MS_PER_SECOND = 1000L
