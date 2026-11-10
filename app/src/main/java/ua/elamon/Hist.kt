package ua.elamon

import kotlin.math.max
import kotlin.math.min
import ua.elamon.data.DEFAULT_HISTORY_SECONDS
import ua.elamon.data.DEFAULT_INTERVAL_MS
import ua.elamon.data.History
import ua.elamon.data.Sample

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
 * The main series for charts, by the service's clock. Primitive ring buffers, grown on demand up to
 * [MAX_POINTS]: a sample adds one point instead of copying every series. [version] counts changes,
 * for the screen state that has to tell a redraw apart. Main thread only.
 */
class Hist {
	/** How far back the series reach: the service's history_seconds. */
	var windowMs: Long = DEFAULT_HISTORY_SECONDS * 1000L
		set(value) {
			field = value.coerceAtLeast(MIN_WINDOW_MS)
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
	private var columns = Array(COLUMNS) { FloatArray(capacity) }

	/** Per GPU id: usage and temperature, in step with [times]. */
	private val gpus = LinkedHashMap<String, Array<FloatArray>>()

	private var medianVersion = -1L
	private var median = 0L

	val gpuIds: Set<String> get() = gpus.keys

	val lastTime: Long get() = if (size == 0) Long.MIN_VALUE else t(size - 1)

	private fun slot(i: Int): Int = (start + i) % capacity

	fun t(i: Int): Long = times[slot(i)]

	val cpu: Points get() = Series(CPU, null)
	val cpuTemp: Points get() = Series(CPU_TEMP, null)
	val mem: Points get() = Series(MEM, null)
	val swap: Points get() = Series(SWAP, null)
	val rx: Points get() = Series(RX, null)
	val tx: Points get() = Series(TX, null)
	val read: Points get() = Series(READ, null)
	val write: Points get() = Series(WRITE, null)
	val batteryPower: Points get() = Series(BATTERY_POWER, null)
	fun gpuUsage(id: String): Points = Series(GPU_USAGE, id)
	fun gpuTemp(id: String): Points = Series(GPU_TEMP, id)

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
		gpus.clear()
		version++
	}

	/** The service's record replaces the app's: it also covers the time nobody was watching. */
	fun replace(h: History) {
		size = 0
		start = 0
		gpus.clear()
		for (i in h.t.indices) {
			if (size > 0 && h.t[i] <= lastTime) continue
			val s = push(h.t[i])
			columns[CPU][s] = h.cpu.getOrNull(i) ?: Float.NaN
			columns[CPU_TEMP][s] = h.cpuTemp.getOrNull(i) ?: Float.NaN
			columns[MEM][s] = h.mem.getOrNull(i)?.toFloat() ?: Float.NaN
			columns[SWAP][s] = h.swap.getOrNull(i)?.toFloat() ?: Float.NaN
			columns[RX][s] = h.rx.getOrNull(i)?.toFloat() ?: Float.NaN
			columns[TX][s] = h.tx.getOrNull(i)?.toFloat() ?: Float.NaN
			columns[READ][s] = h.read.getOrNull(i)?.toFloat() ?: Float.NaN
			columns[WRITE][s] = h.write.getOrNull(i)?.toFloat() ?: Float.NaN
			columns[BATTERY_POWER][s] = h.batteryPower.getOrNull(i) ?: Float.NaN
			for (g in h.gpus) {
				val a = gpu(g.id)
				a[GPU_USAGE][s] = g.usage.getOrNull(i) ?: Float.NaN
				a[GPU_TEMP][s] = g.temp.getOrNull(i) ?: Float.NaN
			}
		}
		prune()
		version++
	}

	/** Adds a sample newer than the last point; false for one already there. */
	fun append(sample: Sample): Boolean {
		if (size > 0 && sample.t <= lastTime) {
			// A clock turned back by more than the whole window starts over; a repeat is skipped.
			if (sample.t >= lastTime - windowMs) return false
			clear()
		}
		val s = push(sample.t)
		// Wired and wireless only: VPN and bridge traffic would count the same bytes twice.
		var rxSum = 0.0
		var txSum = 0.0
		for (n in sample.net) {
			if (n.kind == "wired" || n.kind == "wireless") {
				rxSum += n.rx
				txSum += n.tx
			}
		}
		columns[CPU][s] = sample.cpu.usage
		columns[CPU_TEMP][s] = sample.cpu.temp ?: Float.NaN
		columns[MEM][s] = sample.memory.used.toFloat()
		columns[SWAP][s] = sample.memory.swapUsed.toFloat()
		columns[RX][s] = rxSum.toFloat()
		columns[TX][s] = txSum.toFloat()
		columns[READ][s] = sample.diskIo.sumOf { it.read }.toFloat()
		columns[WRITE][s] = sample.diskIo.sumOf { it.write }.toFloat()
		columns[BATTERY_POWER][s] = sample.battery?.power ?: Float.NaN
		for (g in sample.gpus) {
			val a = gpu(g.id)
			a[GPU_USAGE][s] = g.usage ?: Float.NaN
			a[GPU_TEMP][s] = g.temp ?: Float.NaN
		}
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
		for (a in gpus.values) for (c in a) c[s] = Float.NaN
		return s
	}

	private fun grow() {
		val next = min(capacity * 2, MAX_POINTS)
		fun FloatArray.regrown() = FloatArray(next).also { out -> for (i in 0 until size) out[i] = this[slot(i)] }
		val t = LongArray(next).also { out -> for (i in 0 until size) out[i] = times[slot(i)] }
		columns = Array(COLUMNS) { columns[it].regrown() }
		for (id in gpus.keys.toList()) gpus[id] = Array(2) { gpus.getValue(id)[it].regrown() }
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

	private fun gpu(id: String): Array<FloatArray> =
		gpus.getOrPut(id) { Array(2) { FloatArray(capacity) { Float.NaN } } }

	private fun value(column: Int, gpu: String?, i: Int): Float {
		val c = if (gpu == null) columns[column] else gpus[gpu]?.get(column) ?: return Float.NaN
		return c[slot(i)]
	}

	/** A view into the buffers, read when drawn; a new one each time, so a chart never skips a change. */
	private inner class Series(private val column: Int, private val gpu: String?) : Points {
		override val size: Int get() = this@Hist.size
		override fun t(i: Int): Long = this@Hist.t(i)
		override fun v(i: Int): Float = value(column, gpu, i)
		override val windowMs: Long get() = this@Hist.windowMs
		override val gapMs: Long get() = this@Hist.gapMs
	}

	companion object {
		/** Only a memory guard: two hours at a sample a second. The window decides what stays. */
		const val MAX_POINTS = 7200
		const val GAP_FACTOR = 3L
		private const val MIN_WINDOW_MS = 60_000L
		private const val INITIAL_CAPACITY = 128

		private const val CPU = 0
		private const val CPU_TEMP = 1
		private const val MEM = 2
		private const val SWAP = 3
		private const val RX = 4
		private const val TX = 5
		private const val READ = 6
		private const val WRITE = 7
		private const val BATTERY_POWER = 8
		private const val COLUMNS = 9
		private const val GPU_USAGE = 0
		private const val GPU_TEMP = 1
	}
}
