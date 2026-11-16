package io.github.elaizes.elamon.ui

import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** A long range's time axis: labels at round moments of the local clock, as many as fit. */
class TimeAxisTest {
	private val utc = ZoneId.of("UTC")
	private val hour = 3_600_000L
	private val day = 24 * hour

	private fun at(s: String, zone: ZoneId = utc) = LocalDateTime.parse(s).atZone(zone).toInstant().toEpochMilli()

	private fun Ticks.local(zone: ZoneId = utc) = times.map { LocalDateTime.ofInstant(Instant.ofEpochMilli(it), zone).toString() }

	private val now = at("2026-11-16T15:20:00")

	@Test
	fun anHourEveryTenMinutesBothEndsIn() {
		val t = timeTicks(now - hour, now, 7, utc)
		assertEquals(TickUnit.Time, t.unit)
		assertEquals(listOf("14:20", "14:30", "14:40", "14:50", "15:00", "15:10", "15:20").map { "2026-11-16T$it" }, t.local())
		// One fewer: every quarter of an hour.
		assertEquals(listOf("14:30", "14:45", "15:00", "15:15").map { "2026-11-16T$it" }, timeTicks(now - hour, now, 6, utc).local())
	}

	@Test
	fun aDayEverySixHoursFromMidnight() {
		val t = timeTicks(now - day, now, 5, utc)
		assertEquals(TickUnit.Time, t.unit)
		assertEquals(listOf("2026-11-15T18:00", "2026-11-16T00:00", "2026-11-16T06:00", "2026-11-16T12:00"), t.local())
	}

	@Test
	fun aWeekByDayAMonthByMonday() {
		val week = timeTicks(now - 7 * day, now, 8, utc)
		assertEquals(TickUnit.Day, week.unit)
		assertEquals((10..16).map { "2026-11-${it}T00:00" }, week.local())
		val month = timeTicks(now - 30 * day, now, 5, utc)
		assertEquals(TickUnit.Day, month.unit)
		assertEquals(listOf("2026-10-19", "2026-10-26", "2026-11-02", "2026-11-09", "2026-11-16").map { "${it}T00:00" }, month.local())
	}

	@Test
	fun aYearByMonthsAllOfItByYears() {
		val year = timeTicks(now - 365 * day, now, 6, utc)
		assertEquals(TickUnit.Month, year.unit)
		assertEquals(listOf("01", "03", "05", "07", "09", "11").map { "2026-$it-01T00:00" }, year.local())
		val all = timeTicks(at("2021-06-01T00:00:00"), now, 4, utc)
		assertEquals(TickUnit.Year, all.unit)
		assertEquals(listOf("2022", "2024", "2026").map { "$it-01-01T00:00" }, all.local())
	}

	@Test
	fun theLocalClockAcrossAChangeOfTime() {
		// The clocks went back an hour that night: the labels stay at 0, 6, 12 and 18 o'clock there.
		val berlin = ZoneId.of("Europe/Berlin")
		val t = timeTicks(at("2026-10-24T20:00:00", berlin), at("2026-10-25T20:00:00", berlin), 5, berlin)
		assertEquals(listOf("2026-10-25T00:00", "2026-10-25T06:00", "2026-10-25T12:00", "2026-10-25T18:00"), t.local(berlin))
	}

	@Test(timeout = 2000)
	fun nothingToLabel() {
		assertEquals(emptyList(), timeTicks(now - day, now, 0, utc).times)
		assertEquals(emptyList(), timeTicks(now, now, 5, utc).times)
		assertEquals(emptyList(), timeTicks(now, now - day, 5, utc).times)
		// Times no answer of the computer's would have: no labels, quickly, no crash.
		assertEquals(emptyList(), timeTicks(Long.MIN_VALUE, Long.MAX_VALUE, 5, utc).times)
		// Never more than asked for.
		for (most in 1..12) assertTrue(timeTicks(now - 30 * day, now, most, utc).times.size <= most, "$most")
	}

	@Test
	fun labelsThatWouldTouchAreLeftOut() {
		val shown = axisLabels(floatArrayOf(0f, 50f, 100f, 300f), FloatArray(4) { 40f }, 320f, 8f)
		// The first kept inside at the left edge, the second too close to it, the last kept inside at the right.
		assertEquals(listOf(0 to 0f, 2 to 80f, 3 to 280f), shown)
		// Wider than the axis, as at a large font: at the left edge.
		assertEquals(listOf(0 to 0f), axisLabels(floatArrayOf(50f), floatArrayOf(400f), 320f, 8f))
	}
}
