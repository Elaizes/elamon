package io.github.elaizes.elamon

import kotlin.math.roundToLong

/** The long ranges `GET /api/v1/history` answers, by the names a graph's or a timeline's `ranges` give them; [ms] null for all of it. */
enum class HistoryRange(val key: String, val ms: Long?) {
	Hour("1h", HOUR_MS),
	Day("24h", DAY_MS),
	Week("7d", 7 * DAY_MS),
	Month("30d", 30 * DAY_MS),
	Year("1y", 365 * DAY_MS),
	All("all", null);

	companion object {
		/** A range by its name; null for one this app doesn't know (a newer service's). */
		fun of(key: String): HistoryRange? = entries.find { it.key == key }
	}
}

/** One of the chips over a graph or a timeline: its own range, live ([range] null), or a long one; [ms] orders them. */
data class RangeChip(val range: HistoryRange?, val ms: Long)

/**
 * The chips of a graph or timeline whose own range is [liveMs] and which offers [ranges]: its own
 * and the long ones this app knows, shortest first, leaving out one as long as its own. None
 * where it offers none (a service before 0.7, a small graph), or only its own would be left.
 */
fun rangeChips(liveMs: Long, ranges: List<String>?): List<RangeChip> {
	val long = ranges.orEmpty().mapNotNull(HistoryRange::of).distinct().filter { it.ms != liveMs }
	if (long.isEmpty()) return emptyList()
	return (long.map { RangeChip(it, it.ms ?: Long.MAX_VALUE) } + RangeChip(null, liveMs)).sortedBy { it.ms }
}

/** What a span is counted in, as a chip says it. */
enum class SpanUnit { Seconds, Minutes, Hours, Days }

/** A span as a chip says it: seconds under a minute, minutes under an hour, hours under two days, then days; rounded. */
fun spanParts(ms: Long): Pair<SpanUnit, Long> = when {
	ms < MINUTE_MS -> SpanUnit.Seconds to (ms / SECOND_MS.toDouble()).roundToLong()
	ms < HOUR_MS -> SpanUnit.Minutes to (ms / MINUTE_MS.toDouble()).roundToLong()
	ms < 2 * DAY_MS -> SpanUnit.Hours to (ms / HOUR_MS.toDouble()).roundToLong()
	else -> SpanUnit.Days to (ms / DAY_MS.toDouble()).roundToLong()
}

private const val SECOND_MS = 1000L
private const val MINUTE_MS = 60 * SECOND_MS
private const val HOUR_MS = 60 * MINUTE_MS
private const val DAY_MS = 24 * HOUR_MS
