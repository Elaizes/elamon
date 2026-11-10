package ua.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import ua.elamon.Points
import ua.elamon.lastTime

class ChartMathTest {
	private class Fixed(private val times: LongArray, override val windowMs: Long = 600_000, override val gapMs: Long = 3000) : Points {
		override val size get() = times.size
		override fun t(i: Int) = times[i]
		override fun v(i: Int) = i.toFloat()
	}

	@Test
	fun xFollowsTime() {
		val w = 600f
		assertEquals(600f, chartX(1_000_000, 1_000_000, 600_000, w))
		assertEquals(0f, chartX(400_000, 1_000_000, 600_000, w))
		assertEquals(300f, chartX(700_000, 1_000_000, 600_000, w))
		// Older than the window: left of the chart, clipped there.
		assertTrue(chartX(300_000, 1_000_000, 600_000, w) < 0f)
		assertEquals(700_000L, timeAtX(300f, 1_000_000, 600_000, w))
		assertEquals(1_000_000L, timeAtX(600f, 1_000_000, 600_000, w))
	}

	@Test
	fun nearestPointByTime() {
		val p = Fixed(longArrayOf(1000, 2000, 5000, 6000))
		assertEquals(0, nearestIndex(p, 0))
		assertEquals(0, nearestIndex(p, 1400))
		assertEquals(1, nearestIndex(p, 1600))
		assertEquals(1, nearestIndex(p, 3400))
		assertEquals(2, nearestIndex(p, 3600))
		assertEquals(3, nearestIndex(p, 99_000))
		assertEquals(-1, nearestIndex(Fixed(longArrayOf()), 5))
		assertEquals(6000L, p.lastTime())
		assertEquals(Long.MIN_VALUE, Fixed(longArrayOf()).lastTime())
	}

	@Test
	fun firstVisibleStartsJustBeforeTheWindow() {
		val p = Fixed(longArrayOf(1000, 2000, 3000, 4000))
		assertEquals(1, firstVisibleIndex(p, 2500))
		assertEquals(1, firstVisibleIndex(p, 2000))
		assertEquals(0, firstVisibleIndex(p, 500))
		assertEquals(3, firstVisibleIndex(p, 9000))
	}

	@Test
	fun gaps() {
		assertFalse(isGap(1000, 4000, 3000))
		assertTrue(isGap(1000, 4001, 3000))
	}

	@Test
	fun tooltipStaysInside() {
		assertEquals(250f, tooltipLeft(300f, 100f, 600f))
		assertEquals(0f, tooltipLeft(10f, 100f, 600f))
		assertEquals(500f, tooltipLeft(590f, 100f, 600f))
		// Wider than the chart, as at a large font: it starts at the left edge instead of throwing.
		assertEquals(0f, tooltipLeft(100f, 800f, 600f))
		assertEquals(0f, tooltipLeft(0f, 0f, 0f))
	}
}
