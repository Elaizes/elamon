package io.github.elaizes.elamon.ui

import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.temporal.ChronoUnit
import java.time.temporal.TemporalAdjusters
import kotlin.math.abs
import kotlin.math.max
import io.github.elaizes.elamon.Points
import io.github.elaizes.elamon.data.Segment

/** Where a moment falls on a chart [width] wide whose right edge is [tLast] and which spans [windowMs]. */
fun chartX(t: Long, tLast: Long, windowMs: Long, width: Float): Float =
	width - (tLast - t).toFloat() / windowMs * width

/** The moment at [x] on such a chart. */
fun timeAtX(x: Float, tLast: Long, windowMs: Long, width: Float): Long =
	tLast - ((width - x) / width * windowMs).toLong()

/** The point closest in time to [t], or -1 when there are none. Times only grow, so it's a binary search. */
fun nearestIndex(p: Points, t: Long): Int {
	if (p.size == 0) return -1
	var lo = 0
	var hi = p.size - 1
	while (lo < hi) {
		val mid = (lo + hi) ushr 1
		if (p.t(mid) < t) lo = mid + 1 else hi = mid
	}
	// lo is the first point at or after t; the one before may be closer.
	return if (lo > 0 && abs(t - p.t(lo - 1)) <= abs(p.t(lo) - t)) lo - 1 else lo
}

/** The first point to draw from [tStart] on: the last one at or before it, so the line enters from the edge. */
fun firstVisibleIndex(p: Points, tStart: Long): Int {
	if (p.size == 0) return 0
	var lo = 0
	var hi = p.size - 1
	while (lo < hi) {
		val mid = (lo + hi + 1) ushr 1
		if (p.t(mid) <= tStart) lo = mid else hi = mid - 1
	}
	return lo
}

/** A step from [previous] to [t] too long to join with a line. */
fun isGap(previous: Long, t: Long, gapMs: Long): Boolean = t - previous > gapMs

/** The left edge of a tooltip [boxWidth] wide, centred on [x] but inside a chart [width] wide, or at 0 when it can't fit. */
fun tooltipLeft(x: Float, boxWidth: Float, width: Float): Float =
	(x - boxWidth / 2).coerceIn(0f, max(0f, width - boxWidth))

/**
 * 1, 2, 2.5, 5, 10… so the scale label reads as a round number. Past what a Float holds (a JSON
 * number over 3.4e38 is infinite here) it is the largest one, so the loops end.
 */
fun niceCeiling(v: Float): Float {
	if (v.isNaN() || v <= 0f) return 1f
	if (v.isInfinite()) return Float.MAX_VALUE
	var p = 1f
	while (p * 10 <= v) p *= 10
	while (p > v) p /= 10
	// Under a Float's smallest step there is nothing to round to.
	if (p == 0f) return v
	for (m in NICE_STEPS) if (p * m >= v) return finiteOrMax(p * m)
	return finiteOrMax(p * 10)
}

private fun finiteOrMax(v: Float): Float = if (v.isInfinite()) Float.MAX_VALUE else v

private val NICE_STEPS = floatArrayOf(1f, 2f, 2.5f, 5f, 10f)

/** The next power of two at or above [v]: 512 KiB, 1 MiB, 2 MiB…; the largest Float past them. */
fun niceBinary(v: Float): Float {
	var p = 1f
	while (p < v && p.isFinite()) p *= 2
	return finiteOrMax(p)
}

/**
 * The value at a chart's top: [maxValue] when the scale is fixed; otherwise the data's [top] with
 * headroom, never under [minScale], and rounded up ([binary] for bytes) unless [round] is off.
 */
fun chartScale(top: Float, maxValue: Float?, minScale: Float, binary: Boolean, round: Boolean = true): Float {
	// A fixed top the chart can divide by; one it can't (0, or past what a Float holds) leaves it to the data.
	if (maxValue != null && maxValue.isFinite() && maxValue > 0f) return maxValue
	val wanted = max(top * Thresholds.CHART_HEADROOM, minScale)
	val scale = when {
		!round -> wanted
		binary -> niceBinary(wanted)
		else -> niceCeiling(wanted)
	}
	return if (scale.isNaN() || scale <= 0f) 1f else finiteOrMax(scale)
}

/** The newest value that isn't missing, or NaN when there is none. */
fun Points.latest(): Float {
	for (i in size - 1 downTo 0) {
		val v = v(i)
		if (!v.isNaN()) return v
	}
	return Float.NaN
}

/** The highest value from [tStart] on, or NaN when there is none. */
fun Points.peak(tStart: Long): Float {
	var top = Float.NaN
	for (i in firstVisibleIndex(this, tStart) until size) {
		val v = v(i)
		if (!v.isNaN() && (top.isNaN() || v > top)) top = v
	}
	return top
}

// ---- time axes -----------------------------------------------------------------------------------

/** How a time axis's labels read: a time of day, a day, a month, a year. */
enum class TickUnit { Time, Day, Month, Year }

/** A time axis's labels: when each falls (Unix ms), and how they read. */
data class Ticks(val unit: TickUnit, val times: List<Long>)

/** A step between labels: [n] of [unit], each label at a round local moment of it. */
private class TickStep(val unit: ChronoUnit, val n: Int, val reads: TickUnit)

private val TICK_STEPS = listOf(
	TickStep(ChronoUnit.MINUTES, 1, TickUnit.Time),
	TickStep(ChronoUnit.MINUTES, 2, TickUnit.Time),
	TickStep(ChronoUnit.MINUTES, 5, TickUnit.Time),
	TickStep(ChronoUnit.MINUTES, 10, TickUnit.Time),
	TickStep(ChronoUnit.MINUTES, 15, TickUnit.Time),
	TickStep(ChronoUnit.MINUTES, 30, TickUnit.Time),
	TickStep(ChronoUnit.HOURS, 1, TickUnit.Time),
	TickStep(ChronoUnit.HOURS, 2, TickUnit.Time),
	TickStep(ChronoUnit.HOURS, 3, TickUnit.Time),
	TickStep(ChronoUnit.HOURS, 6, TickUnit.Time),
	TickStep(ChronoUnit.HOURS, 12, TickUnit.Time),
	TickStep(ChronoUnit.DAYS, 1, TickUnit.Day),
	TickStep(ChronoUnit.DAYS, 2, TickUnit.Day),
	TickStep(ChronoUnit.WEEKS, 1, TickUnit.Day),
	TickStep(ChronoUnit.WEEKS, 2, TickUnit.Day),
	TickStep(ChronoUnit.MONTHS, 1, TickUnit.Month),
	TickStep(ChronoUnit.MONTHS, 2, TickUnit.Month),
	TickStep(ChronoUnit.MONTHS, 3, TickUnit.Month),
	TickStep(ChronoUnit.MONTHS, 6, TickUnit.Month),
	TickStep(ChronoUnit.YEARS, 1, TickUnit.Year),
	TickStep(ChronoUnit.YEARS, 2, TickUnit.Year),
	TickStep(ChronoUnit.YEARS, 5, TickUnit.Year),
	TickStep(ChronoUnit.YEARS, 10, TickUnit.Year),
)

/**
 * The labels of a time axis from [start] to [end] (Unix ms), at most [most] of them: at round
 * moments of the clock in [zone] (every 6 hours from midnight, each Monday, each first of a month),
 * the shortest step that keeps to [most]. None when [most] is under 1 or the axis spans nothing.
 */
fun timeTicks(start: Long, end: Long, most: Int, zone: ZoneId): Ticks {
	val none = Ticks(TickUnit.Time, emptyList())
	if (most < 1 || end <= start) return none
	for (step in TICK_STEPS) {
		// Times past what a calendar holds come from a broken answer: no labels rather than a crash.
		val times = runCatching { ticksOf(step, start, end, most, zone) }.getOrElse { return none } ?: continue
		return Ticks(step.reads, times)
	}
	return none
}

/** [step]'s moments from [start] to [end], by the local clock in [zone]; null past [most]. */
private fun ticksOf(step: TickStep, start: Long, end: Long, most: Int, zone: ZoneId): List<Long>? {
	var at = firstTick(step, LocalDateTime.ofInstant(Instant.ofEpochMilli(start), zone))
	val out = ArrayList<Long>(most)
	while (true) {
		val ms = at.atZone(zone).toInstant().toEpochMilli()
		if (ms > end) return out
		if (ms >= start) {
			if (out.size == most) return null
			out += ms
		}
		at = at.plus(step.n.toLong(), step.unit)
	}
}

/** The round moment of [step] at or before [t]: its unit's start, on a multiple of [TickStep.n]. */
private fun firstTick(step: TickStep, t: LocalDateTime): LocalDateTime {
	val day = t.toLocalDate().atStartOfDay()
	return when (step.unit) {
		ChronoUnit.MINUTES -> t.truncatedTo(ChronoUnit.MINUTES).withMinute(t.minute - t.minute % step.n)
		ChronoUnit.HOURS -> t.truncatedTo(ChronoUnit.HOURS).withHour(t.hour - t.hour % step.n)
		ChronoUnit.DAYS -> day.minusDays(Math.floorMod(day.toLocalDate().toEpochDay(), step.n.toLong()))
		ChronoUnit.WEEKS -> day.with(TemporalAdjusters.previousOrSame(DayOfWeek.MONDAY)).let { monday ->
			// Every other Monday counted from the epoch's first, so the labels don't move as the window does.
			monday.minusWeeks(Math.floorMod(Math.floorDiv(monday.toLocalDate().toEpochDay() - FIRST_MONDAY, DAYS_PER_WEEK), step.n.toLong()))
		}
		ChronoUnit.MONTHS -> day.withDayOfMonth(1).let { it.withMonth(it.monthValue - (it.monthValue - 1) % step.n) }
		else -> day.withDayOfYear(1).let { it.withYear(it.year - Math.floorMod(it.year, step.n)) }
	}
}

/** 1970-01-05, the epoch's first Monday, as a day of the epoch. */
private const val FIRST_MONDAY = 4L
private const val DAYS_PER_WEEK = 7L

/**
 * Where the labels of an axis [width] wide go: each centred on its moment's x in [xs], [widths]
 * wide, kept inside the axis; one that would come closer than [gap] to the one before is left
 * out. Each shown label's index and left edge.
 */
fun axisLabels(xs: FloatArray, widths: FloatArray, width: Float, gap: Float): List<Pair<Int, Float>> {
	val out = ArrayList<Pair<Int, Float>>(xs.size)
	var right = Float.NEGATIVE_INFINITY
	for (i in xs.indices) {
		val left = (xs[i] - widths[i] / 2).coerceIn(0f, max(0f, width - widths[i]))
		if (left < right + gap) continue
		out += i to left
		right = left + widths[i]
	}
	return out
}

// ---- timelines -----------------------------------------------------------------------------------

/** A timeline's [segment] as drawn, from [x0] to [x1]; [widened] when drawn wider than its time, to be seen at all. */
data class Stretch(val x0: Float, val x1: Float, val segment: Segment, val widened: Boolean = false)

/**
 * [segments] on a bar [width] wide from [start] to [end] (Unix ms): each clipped to it, those
 * outside it left out, each at least [minPx] wide around its middle and inside the bar, so even a
 * short stretch on battery shows. A widened one is drawn over its neighbours.
 */
fun stretches(segments: List<Segment>, start: Long, end: Long, width: Float, minPx: Float): List<Stretch> {
	if (end <= start || width <= 0f) return emptyList()
	val span = (end - start).toFloat()
	val out = ArrayList<Stretch>(segments.size)
	for (s in segments) {
		val from = max(s.from, start)
		val to = minOf(s.to, end)
		if (to <= from) continue
		val x0 = (from - start) / span * width
		val x1 = (to - start) / span * width
		val stretch = Stretch(x0, x1, s.copy(from = from, to = to))
		if (x1 - x0 >= minPx) {
			out += stretch
			continue
		}
		val w = minOf(minPx, width)
		val left = ((x0 + x1) / 2 - w / 2).coerceIn(0f, width - w)
		out += stretch.copy(x0 = left, x1 = left + w, widened = true)
	}
	return out
}

/** The stretch under [x]: a widened one first, as it is drawn on top; -1 for none. */
fun stretchAt(stretches: List<Stretch>, x: Float): Int {
	val wide = stretches.indexOfFirst { it.widened && x >= it.x0 && x <= it.x1 }
	return if (wide >= 0) wide else stretches.indexOfFirst { x >= it.x0 && x <= it.x1 }
}

/** How long each state lasts from [start] to [end] in [segments], in ms, by state. */
fun timeIn(segments: List<Segment>, start: Long, end: Long): Map<String, Long> {
	val out = LinkedHashMap<String, Long>()
	for (s in segments) {
		val d = minOf(s.to, end) - max(s.from, start)
		if (d > 0) out[s.state] = (out[s.state] ?: 0L) + d
	}
	return out
}

/**
 * A long range's [history], which reaches [to] (when it was asked for), carried on to now by the
 * stream's [live] segments from then on; a stretch that goes on across [to] is one.
 */
fun splice(history: List<Segment>, to: Long, live: List<Segment>): List<Segment> {
	val out = ArrayList(history)
	for (s in live) {
		if (s.to <= to) continue
		val next = if (s.from < to) s.copy(from = to) else s
		val last = out.lastOrNull()
		if (last != null && last.state == next.state && last.to == next.from) out[out.size - 1] = last.copy(to = next.to) else out += next
	}
	return out
}
