package io.github.elaizes.elamon

import kotlin.math.max
import kotlin.math.min
import io.github.elaizes.elamon.data.DEFAULT_HISTORY_SECONDS
import io.github.elaizes.elamon.data.DEFAULT_INTERVAL_MS
import io.github.elaizes.elamon.data.HistoryEntry

/** Points of one series, oldest first: [t] on the service's clock in milliseconds, [v] NaN where a value was missing. */
interface Points {
	val size: Int
	fun t(i: Int): Long
	fun v(i: Int): Float

	/** How much time a chart of it spans, ending at its newest point. */
	val windowMs: Long

	/** A step longer than this is a gap (the screen was off, the computer asleep): no line across it. */
	val gapMs: Long
}

fun Points.lastTime(): Long = if (size == 0) Long.MIN_VALUE else t(size - 1)

fun Points.hasData(): Boolean = (0 until size).any { !v(it).isNaN() }

/**
 * A graph's (or a spark's) [seriesCount] series by the service's clock: primitive ring buffers,
 * grown on demand up to [MAX_POINTS], so a data line adds one point instead of copying every
 * series. The service's history replaces it; each data line's values are appended at its time, and
 * what falls out of [windowMs] (the graph's range) is dropped. [version] counts changes, for the
 * screen state that has to tell a redraw apart. Main thread only.
 */
class Hist(val seriesCount: Int, windowMs: Long = DEFAULT_HISTORY_SECONDS * 1000L) {
	/** How far back the series reach. */
	var windowMs: Long = windowMs.coerceAtLeast(MIN_WINDOW_MS)
		set(value) {
			val w = value.coerceAtLeast(MIN_WINDOW_MS)
			if (w == field) return
			field = w
			prune()
			version++
		}

	/** How often points come now; gaps are measured against this as well as the median step. */
	var stepHintMs: Long = DEFAULT_INTERVAL_MS.toLong()

	var version = 0L
		private set

	var size = 0
		private set

	private var capacity = INITIAL_CAPACITY
	private var start = 0
	private var times = LongArray(capacity)
	private var columns = Array(seriesCount) { FloatArray(capacity) }

	private var medianVersion = -1L
	private var median = 0L

	val lastTime: Long get() = if (size == 0) Long.MIN_VALUE else t(size - 1)

	private fun slot(i: Int): Int = (start + i) % capacity

	fun t(i: Int): Long = times[slot(i)]

	/** The series [k], read when drawn; a new view each time, so a chart never skips a change. */
	fun points(k: Int): Points = Series(k)

	/** The usual time between points: what a gap is measured against. */
	fun medianStepMs(): Long {
		if (medianVersion != version) {
			median = if (size < 2) 0 else LongArray(size - 1) { t(it + 1) - t(it) }.apply { sort() }.let { it[it.size / 2] }
			medianVersion = version
		}
		return median
	}

	val gapMs: Long get() = GAP_FACTOR * max(medianStepMs(), stepHintMs)

	fun clear() {
		size = 0
		start = 0
		version++
	}

	/** The service's record replaces the app's: it also covers the time nobody was watching. */
	fun replace(h: HistoryEntry) {
		size = 0
		start = 0
		for (i in h.t.indices) {
			if (size > 0 && h.t[i] <= lastTime) continue
			val s = push(h.t[i])
			for (k in 0 until seriesCount) columns[k][s] = h.v.getOrNull(k)?.getOrNull(i)?.toFloat() ?: Float.NaN
		}
		prune()
		version++
	}

	/** Adds [values] at [t], newer than the last point; false for a time already there. */
	fun append(t: Long, values: List<Double?>): Boolean {
		if (size > 0 && t <= lastTime) {
			// A clock turned back by more than the whole window starts over; a repeat is skipped.
			if (t >= lastTime - windowMs) return false
			clear()
		}
		val s = push(t)
		for (k in 0 until seriesCount) columns[k][s] = values.getOrNull(k)?.toFloat() ?: Float.NaN
		prune()
		version++
		return true
	}

	/** Room for one more point at the end, its time set and every value NaN; returns its slot. */
	private fun push(t: Long): Int {
		if (size == capacity) {
			if (capacity < MAX_POINTS) grow() else dropFirst()
		}
		val s = slot(size)
		times[s] = t
		size++
		for (c in columns) c[s] = Float.NaN
		return s
	}

	private fun grow() {
		val next = min(capacity * 2, MAX_POINTS)
		fun FloatArray.regrown() = FloatArray(next).also { out -> for (i in 0 until size) out[i] = this[slot(i)] }
		val t = LongArray(next).also { out -> for (i in 0 until size) out[i] = times[slot(i)] }
		columns = Array(seriesCount) { columns[it].regrown() }
		times = t
		start = 0
		capacity = next
	}

	private fun dropFirst() {
		start = (start + 1) % capacity
		size--
	}

	/** Drops what fell out of the window, keeping one point before it so a line still enters from the edge. */
	private fun prune() {
		val from = lastTime - windowMs
		while (size > 1 && t(1) <= from) dropFirst()
	}

	private inner class Series(private val k: Int) : Points {
		override val size: Int get() = this@Hist.size
		override fun t(i: Int): Long = this@Hist.t(i)
		override fun v(i: Int): Float = if (k < seriesCount) columns[k][slot(i)] else Float.NaN
		override val windowMs: Long get() = this@Hist.windowMs
		override val gapMs: Long get() = this@Hist.gapMs
	}

	companion object {
		/** Only a memory guard: two hours at a point a second. The window decides what stays. */
		const val MAX_POINTS = 7200
		const val GAP_FACTOR = 3L
		private const val MIN_WINDOW_MS = 10_000L
		private const val INITIAL_CAPACITY = 64
	}
}
