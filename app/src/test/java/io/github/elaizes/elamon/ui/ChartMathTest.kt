package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import io.github.elaizes.elamon.Points
import io.github.elaizes.elamon.lastTime

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

	@Test
	fun scales() {
		// Fixed, as for percentages.
		assertEquals(100f, chartScale(130f, 100f, 1f, binary = false))
		// The data with headroom, rounded: 80 °C wants 92, which reads as 100.
		assertEquals(100f, chartScale(80f, null, 60f, binary = false))
		assertEquals(50f, chartScale(30f, null, 50f, binary = false))
		assertEquals(2048f, chartScale(1000f, null, 64f, binary = true))
		// A temperature sparkline: 100 °C unless the data runs hotter, then the data itself.
		assertEquals(100f, chartScale(80f, null, Thresholds.TEMP_BAR_TOP, binary = false, round = false))
		assertEquals(110f * Thresholds.CHART_HEADROOM, chartScale(110f, null, Thresholds.TEMP_BAR_TOP, binary = false, round = false))
	}

	private class Values(private val times: LongArray, private val values: FloatArray) : Points {
		override val size get() = times.size
		override val windowMs = 600_000L
		override val gapMs = 3000L
		override fun t(i: Int) = times[i]
		override fun v(i: Int) = values[i]
	}

	@Test
	fun latestAndPeak() {
		val p = Values(longArrayOf(1000, 2000, 3000, 4000), floatArrayOf(90f, 20f, 50f, Float.NaN))
		assertEquals(50f, p.latest())
		assertEquals(90f, p.peak(0))
		// From 2500 on, with the point just before it that the line enters by.
		assertEquals(50f, p.peak(2500))
		assertTrue(Values(longArrayOf(1000), floatArrayOf(Float.NaN)).latest().isNaN())
		assertTrue(Values(longArrayOf(), floatArrayOf()).peak(0).isNaN())
	}
}
