package io.github.elaizes.elamon.ui

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.Segment
import io.github.elaizes.elamon.data.TimelineHistory
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.decodeStreamLine

/** A timeline's segments as a bar: where each stretch goes, how long each state lasted, a long range carried on to now. */
class TimelineMathTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val minute = 60_000L
	private val day = 1440 * minute

	private fun seg(from: Long, to: Long, state: String) = Segment(from, to, state)

	@Test
	fun theFixturesDayAcrossTheBar() {
		val line = assertIs<DataLine>(decodeStreamLine(File(dir, "0.7/stream-ups.json").readText()))
		val segments = line.widgets.getValue("ups.eco/power").segments!!
		val s = stretches(segments, line.t - day, line.t, 400f, 2f)
		assertEquals(listOf("none", "line", "none", "line", "battery"), s.map { it.segment.state })
		// Edge to edge, each where the one before ends; 12 minutes on battery are wide enough as they are.
		assertEquals(0f, s.first().x0)
		assertEquals(400f, s.last().x1, 0.01f)
		assertTrue(s.zipWithNext().all { (a, b) -> kotlin.math.abs(a.x1 - b.x0) < 0.01f })
		assertFalse(s.any { it.widened })
		assertEquals(400f * 12 / 1440, s.last().x1 - s.last().x0, 0.01f)
	}

	@Test
	fun aShortStretchIsDrawnWideEnoughToSee() {
		// Ten seconds on battery in a day: a twentieth of a pixel, drawn two wide around its middle.
		val s = stretches(listOf(seg(0, day / 2, "line"), seg(day / 2, day / 2 + 10_000, "battery"), seg(day / 2 + 10_000, day, "line")), 0, day, 400f, 2f)
		val battery = s[1]
		assertTrue(battery.widened)
		assertEquals(2f, battery.x1 - battery.x0, 0.001f)
		assertEquals(200f, (battery.x0 + battery.x1) / 2, 0.1f)
		// A finger on it finds it, though the line's stretches lie under it too.
		assertEquals(1, stretchAt(s, 200f))
		assertEquals(0, stretchAt(s, 100f))
		assertEquals(-1, stretchAt(s, 401f))
		// At the very end, kept inside the bar.
		val end = stretches(listOf(seg(0, day - 1000, "line"), seg(day - 1000, day, "battery")), 0, day, 400f, 2f).last()
		assertEquals(398f, end.x0, 0.001f)
		assertEquals(400f, end.x1, 0.001f)
	}

	@Test
	fun clippedToTheWindow() {
		val s = stretches(listOf(seg(-day, 100 * minute, "none"), seg(100 * minute, 2 * day, "line"), seg(3 * day, 4 * day, "battery")), 0, day, 1440f, 1f)
		// Before the window it starts at its start, after it ends at its end; outside it, nothing.
		assertEquals(listOf(seg(0, 100 * minute, "none"), seg(100 * minute, day, "line")), s.map { it.segment })
		assertEquals(100f, s[0].x1, 0.01f)
		assertEquals(emptyList(), stretches(listOf(seg(0, day, "line")), day, day, 400f, 2f))
		assertEquals(emptyList(), stretches(listOf(seg(0, day, "line")), 0, day, 0f, 2f))
	}

	@Test
	fun howLongEachStateLasted() {
		val segments = listOf(seg(-minute, 10 * minute, "none"), seg(10 * minute, 50 * minute, "line"), seg(50 * minute, 52 * minute, "battery"), seg(52 * minute, 70 * minute, "line"))
		assertEquals(mapOf("none" to 10 * minute, "line" to 48 * minute, "battery" to 2 * minute), timeIn(segments, 0, 60 * minute))
		assertEquals(emptyMap(), timeIn(segments, 80 * minute, 90 * minute))
	}

	@Test
	fun aLongRangeCarriedOnToNow() {
		val history = listOf(seg(0, 100, "none"), seg(100, 500, "line"))
		// The stream's day overlaps it; only what came after it was asked for is added, a stretch that goes on is one.
		val live = listOf(seg(50, 300, "line"), seg(300, 520, "line"), seg(520, 600, "battery"))
		assertEquals(listOf(seg(0, 100, "none"), seg(100, 520, "line"), seg(520, 600, "battery")), splice(history, 500, live))
		// Nothing new yet: the long range as it came.
		assertEquals(history, splice(history, 500, emptyList()))
		assertEquals(history, splice(history, 500, listOf(seg(50, 500, "line"))))
		// A state that changed since: from where the long range ends.
		assertEquals(history + seg(500, 510, "battery"), splice(history, 500, listOf(seg(400, 510, "battery"))))
	}

	@Test
	fun theFixturesWeekWithItsDay() {
		val week = ProtocolJson.decodeFromString(TimelineHistory.serializer(), File(dir, "0.7/history-timeline.json").readText())
		val line = assertIs<DataLine>(decodeStreamLine(File(dir, "0.7/stream-ups.json").readText()))
		val day = line.widgets.getValue("ups.eco/power").segments!!
		// Asked at the line's time: the stream adds nothing yet.
		assertEquals(week.segments, splice(week.segments, week.to, day))
		val totals = timeIn(week.segments, week.from, week.to)
		assertEquals(week.to - week.from, totals.values.sum())
		assertEquals(720_123L, totals.getValue("battery"))
	}

	@Test
	fun sharesOfTheKnownTime() {
		// Nothing at all, or nothing known: no shares, so the legend tells durations alone.
		assertEquals(emptyMap(), shares(emptyMap()))
		assertEquals(emptyMap(), shares(mapOf("none" to day)))
		// On battery all the time something was known: all of it, whatever wasn't known.
		assertEquals(mapOf("battery" to Share.Percent(100)), shares(mapOf("none" to day, "battery" to 5 * minute)))
		// "No data" is no part of the whole, nor a state this app doesn't know.
		assertEquals(mapOf("line" to Share.Percent(75), "battery" to Share.Percent(25)), shares(mapOf("none" to day, "line" to 3 * minute, "battery" to minute, "bypass" to day)))
		// A minute in 1000: under one percent, not 0%; on line rounds to 100, and the two aren't made to add up.
		assertEquals(mapOf("line" to Share.Percent(100), "battery" to Share.UnderOne), shares(mapOf("line" to 1000 * minute, "battery" to minute)))
		// Halves round up; each share on its own.
		assertEquals(mapOf("line" to Share.Percent(3), "battery" to Share.Percent(98)), shares(mapOf("line" to 25L, "battery" to 975L)))
		assertEquals("<1%", Fmt.share(Share.UnderOne))
		assertEquals("4%", Fmt.share(Share.Percent(4)))
	}

	@Test
	fun theFixturesWeekAllOfItInShares() {
		// "All": the long range from where its history starts; the shares are of the time something was known in it.
		val week = ProtocolJson.decodeFromString(TimelineHistory.serializer(), File(dir, "0.7/history-timeline.json").readText())
		val totals = timeIn(week.segments, week.from, week.to)
		val known = totals.getValue("line") + totals.getValue("battery")
		val s = shares(totals)
		assertEquals(setOf("line", "battery"), s.keys)
		assertEquals(Share.Percent(Math.round(totals.getValue("battery") * 100.0 / known).toInt()), s["battery"])
	}

	/** The fixture station's day, to its data line's time: 18 h 46 min not known, 5 h 1 min on line, 12 min on battery. */
	private fun fixtureDay(): Map<String, Long> {
		val line = assertIs<DataLine>(decodeStreamLine(File(dir, "0.7/stream-ups.json").readText()))
		return timeIn(line.widgets.getValue("ups.eco/power").segments!!, line.t - day, line.t)
	}

	@Test
	fun theLegendInEachMode() {
		val totals = fixtureDay()
		val states = listOf("line", "battery", "none")
		val line = 18_080L
		val battery = 720L
		val none = 67_599L
		assertEquals(
			listOf(LegendEntry("line", line, null), LegendEntry("battery", battery, null), LegendEntry("none", none, null)),
			legendEntries(totals, LegendMode.Duration, states),
		)
		// Shares instead of durations, but "No data", which has none, by its duration.
		assertEquals(
			listOf(LegendEntry("line", null, Share.Percent(96)), LegendEntry("battery", null, Share.Percent(4)), LegendEntry("none", none, null)),
			legendEntries(totals, LegendMode.Percent, states),
		)
		assertEquals(
			listOf(LegendEntry("line", line, Share.Percent(96)), LegendEntry("battery", battery, Share.Percent(4)), LegendEntry("none", none, null)),
			legendEntries(totals, LegendMode.Both, states),
		)
		assertEquals(listOf("5 h 1 min", "12 min", "18 h 46 min"), listOf(line, battery, none).map(Fmt::duration))
		// Nothing known in the range: durations alone, in any mode.
		assertEquals(listOf(LegendEntry("none", 3600, null)), legendEntries(mapOf("none" to 3_600_000L), LegendMode.Percent, states))
		// The settings' word, as the service sends it; one this app doesn't know, or none, is durations.
		assertEquals(listOf(LegendMode.Duration, LegendMode.Percent, LegendMode.Both, LegendMode.Duration, LegendMode.Duration), listOf("duration", "percent", "both", "hours", null).map(LegendMode::of))
	}

	@Test
	fun theFixturesLegendsReachTheApp() {
		val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(dir, "0.8.1/ui-legend.json").readText())
		val modes = ui.dashboards.single().widgets.associate { it.id to LegendMode.of(it.legend) }
		assertEquals(mapOf("power/plain" to LegendMode.Duration, "power/duration" to LegendMode.Duration, "power/percent" to LegendMode.Percent, "power/both" to LegendMode.Both), modes)
	}
}
