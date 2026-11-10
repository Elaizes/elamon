package ua.elamon.ui

import kotlin.math.abs
import kotlin.math.max
import ua.elamon.Points

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
