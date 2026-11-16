package io.github.elaizes.elamon

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.withoutRanges

/** The chips over a graph or a timeline: its own range, live, and the long ones its `ranges` offer. */
class HistoryRangesTest {
	private val all = listOf("1h", "24h", "7d", "30d", "1y", "all")
	private val minute = 60_000L
	private val hour = 60 * minute
	private val day = 24 * hour

	@Test
	fun rangesByTheirNames() {
		assertEquals(HistoryRange.entries, all.map { HistoryRange.of(it) })
		assertEquals(listOf(hour, day, 7 * day, 30 * day, 365 * day, null), HistoryRange.entries.map { it.ms })
		// One a newer service names, this app doesn't offer.
		assertNull(HistoryRange.of("90d"))
	}

	@Test
	fun aGraphsOwnRangeComesFirstThenTheLongOnes() {
		val chips = rangeChips(10 * minute, all)
		assertEquals(listOf(null) + HistoryRange.entries, chips.map { it.range })
		assertEquals(10 * minute, chips.first().ms)
	}

	@Test
	fun shortestFirstLeavingOutOneAsLongAsItsOwn() {
		// A timeline of a day: its own is the day, live, between the hour and the week.
		val chips = rangeChips(day, all)
		assertEquals(listOf(HistoryRange.Hour, null, HistoryRange.Week, HistoryRange.Month, HistoryRange.Year, HistoryRange.All), chips.map { it.range })
		// Unknown names are left out, and twice the same is once.
		assertEquals(listOf(null, HistoryRange.Week), rangeChips(10 * minute, listOf("7d", "90d", "7d")).map { it.range })
	}

	@Test
	fun noChipsWithoutLongRanges() {
		// A service before 0.7 (no `ranges`), `ranges = false` (none), a small graph.
		assertEquals(emptyList(), rangeChips(10 * minute, null))
		assertEquals(emptyList(), rangeChips(10 * minute, emptyList()))
		// Nothing but one as long as its own: its own alone needs no chips.
		assertEquals(emptyList(), rangeChips(day, listOf("24h")))
		assertEquals(emptyList(), rangeChips(10 * minute, listOf("90d")))
	}

	@Test
	fun theFixturesChips() {
		val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
		val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(dir, "0.7/ui-ups.json").readText())
		val widgets = ui.dashboard("ups.eco")!!.widgets
		val watts = widgets.first { it.id == "ups.eco/watts" }
		assertEquals(7, rangeChips(watts.range!! * 1000L, watts.ranges).size)
		val power = widgets.first { it.id == "ups.eco/power" }
		assertEquals(6, rangeChips(power.range!! * 1000L, power.ranges).size)
		// A 0.6.2 service's graphs, which have no `ranges`, have none.
		val old = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), withoutRanges(ProtocolJson.parseToJsonElement(File(dir, "ui.json").readText())))
		assertTrue(old.dashboards.flatMap { allWidgets(it.widgets) }.filter { it.type == "graph" }.all { rangeChips((it.range ?: 600) * 1000L, it.ranges).isEmpty() })
	}

	@Test
	fun spansAsAChipSaysThem() {
		assertEquals(SpanUnit.Seconds to 30L, spanParts(30_000))
		assertEquals(SpanUnit.Minutes to 10L, spanParts(10 * minute))
		assertEquals(SpanUnit.Minutes to 59L, spanParts(59 * minute))
		assertEquals(SpanUnit.Hours to 1L, spanParts(hour))
		assertEquals(SpanUnit.Hours to 24L, spanParts(day))
		assertEquals(SpanUnit.Hours to 47L, spanParts(47 * hour))
		assertEquals(SpanUnit.Days to 7L, spanParts(7 * day))
		assertEquals(SpanUnit.Days to 30L, spanParts(30 * day))
		// Rounded: 90 minutes read as 2 h.
		assertEquals(SpanUnit.Hours to 2L, spanParts(90 * minute))
	}
}
