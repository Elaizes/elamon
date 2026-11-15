package io.github.elaizes.elamon.ui

import android.content.Context
import android.text.format.DateFormat
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import java.text.SimpleDateFormat
import java.time.ZoneId
import java.util.Date
import java.util.Locale
import io.github.elaizes.elamon.HistoryRange
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.RangeChip
import io.github.elaizes.elamon.RangeView
import io.github.elaizes.elamon.SpanUnit
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
