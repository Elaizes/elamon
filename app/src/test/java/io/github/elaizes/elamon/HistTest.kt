package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.HistoryEntry
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.UiDoc

/** A graph's points: the service's history, then each data line's values appended, what falls out of the range dropped. */
class HistTest {
	private fun Points.values() = (0 until size).map { v(it) }
	private fun Points.times() = (0 until size).map { t(it) }

	private fun Hist.add(t: Long, vararg v: Double?) = append(t, v.toList())

	@Test
	fun appendsInOrder() {
		val h = Hist(2)
		assertTrue(h.add(1000, 1.0, 10.0))
		assertTrue(h.add(2000, 2.0, null))
		assertEquals(listOf(1000L, 2000L), h.points(0).times())
		assertEquals(listOf(1f, 2f), h.points(0).values())
		assertEquals(10f, h.points(1).v(0))
		assertTrue(h.points(1).v(1).isNaN())
		assertEquals(2000L, h.lastTime)
		// A series the graph doesn't have reads as missing.
		assertTrue(h.points(5).values().all { it.isNaN() })
	}

	@Test
	fun aLineWithFewerValuesThanSeriesHasGaps() {
		val h = Hist(2)
		h.append(1000, listOf(1.0))
		h.append(2000, emptyList())
		assertEquals(1f, h.points(0).v(0))
		assertTrue(h.points(1).v(0).isNaN())
		assertTrue(h.points(0).v(1).isNaN())
	}

	@Test
	fun skipsWhatIsNotNewer() {
		val h = Hist(1)
		h.add(5000, 1.0)
		val v = h.version
		assertFalse(h.add(5000, 2.0))
		assertFalse(h.add(4000, 2.0))
		assertEquals(1, h.size)
		assertEquals(v, h.version)
	}

	@Test
	fun clockTurnedFarBackStartsOver() {
		val h = Hist(1, 600_000)
		h.add(10_000_000, 1.0)
		h.add(10_001_000, 1.0)
		// Two hours back, as when the computer's clock went from local time to UTC.
		assertTrue(h.add(10_001_000 - 7_200_000, 1.0))
		assertEquals(1, h.size)
	}

	@Test
	fun dropsWhatFallsOutOfTheRange() {
		val h = Hist(1, 60_000)
		for (s in 0..300) h.add(s * 1000L, s.toDouble())
		val from = h.lastTime - h.windowMs
		// One point at or before the range stays, so a line still enters from the left edge.
		assertTrue(h.t(0) <= from)
		assertTrue(h.t(1) > from)
		assertEquals((h.t(0)..h.lastTime step 1000).toList(), h.points(0).times())
		assertEquals(h.points(0).times().map { it / 1000f }, h.points(0).values())
		// A shorter range drops more at once.
		h.windowMs = 30_000
		assertTrue(h.t(1) > h.lastTime - 30_000)
	}

	@Test
	fun countIsOnlyAMemoryGuard() {
		val h = Hist(1, 100_000_000)
		val n = Hist.MAX_POINTS + 10
		for (i in 0 until n) h.add(i * 10L, i.toDouble())
		assertEquals(Hist.MAX_POINTS, h.size)
		assertEquals(100L, h.t(0))
		assertEquals((n - 1) * 10L, h.lastTime)
		assertEquals(10f, h.points(0).v(0))
	}

	@Test
	fun historyReplacesEverything() {
		val h = Hist(2)
		h.add(999_000, 1.0, 1.0)
		h.replace(HistoryEntry(t = listOf(1000, 2000, 2000, 3000), v = listOf(listOf(1.0, 2.0, 99.0, 3.0), listOf(40.0, null, null))))
		// The repeated moment is dropped; a list shorter than t reads as missing.
		assertEquals(listOf(1000L, 2000L, 3000L), h.points(0).times())
		assertEquals(listOf(1f, 2f, 3f), h.points(0).values())
		assertEquals(40f, h.points(1).v(0))
		assertTrue(h.points(1).v(1).isNaN())
		assertTrue(h.points(1).v(2).isNaN())
		// Lines carry on from the history's last moment.
		assertFalse(h.add(3000, 1.0, 1.0))
		assertTrue(h.add(4000, 1.0, 1.0))
	}

	@Test
	fun valuesPastAFloatAreMissing() {
		// 1e39 is infinite as a Float: no chart scales to it, so it is a gap like a missing value.
		val h = Hist(2)
		h.replace(HistoryEntry(t = listOf(1000, 2000), v = listOf(listOf(1e39, 1.0), listOf(-1e39, Double.NaN))))
		h.add(3000, Double.POSITIVE_INFINITY, 2.0)
		assertTrue(h.points(0).v(0).isNaN())
		assertEquals(1f, h.points(0).v(1))
		assertTrue(h.points(0).v(2).isNaN())
		assertTrue(h.points(1).v(0).isNaN())
		assertTrue(h.points(1).v(1).isNaN())
		assertEquals(2f, h.points(1).v(2))
	}

	@Test
	fun aLineWithHistoryKeepsItsOwnPointToo() {
		// The service's history ends at the line's moment: nothing more to add.
		val h = Hist(1)
		h.take(3000, listOf(3.0), HistoryEntry(t = listOf(1000, 2000, 3000), v = listOf(listOf(1.0, 2.0, 3.0))))
		assertEquals(listOf(1000L, 2000L, 3000L), h.points(0).times())
		// Ending before it, the line's own value is the newest point.
		h.take(4000, listOf(4.0), HistoryEntry(t = listOf(1000, 2000), v = listOf(listOf(1.0, 2.0))))
		assertEquals(listOf(1000L, 2000L, 4000L), h.points(0).times())
		assertEquals(4f, h.points(0).v(2))
		// Reaching past it, nothing is cleared or added.
		h.take(1500, listOf(9.0), HistoryEntry(t = listOf(1000, 2000), v = listOf(listOf(1.0, 2.0))))
		assertEquals(listOf(1f, 2f), h.points(0).values())
		// An empty history, as for a graph with nothing kept yet: the line's point starts it.
		h.take(5000, listOf(5.0), HistoryEntry())
		assertEquals(listOf(5000L), h.points(0).times())
		// Without history, appended.
		h.take(6000, listOf(6.0), null)
		assertEquals(listOf(5f, 6f), h.points(0).values())
	}

	@Test
	fun theFixturesHistoryReads() {
		// stream-data.json: the network graph's two series, the temperature's spark with a gap.
		val h = Hist(1)
		h.replace(HistoryEntry(t = listOf(1_700_000_000_000, 1_700_000_001_000), v = listOf(listOf(50.0, null))))
		assertEquals(50f, h.points(0).v(0))
		assertTrue(h.points(0).v(1).isNaN())
		assertTrue(h.add(1_700_000_002_000, 55.5))
		assertEquals(3, h.size)
	}

	@Test
	fun wrapsAroundTheRing() {
		val h = Hist(1, 60_000)
		for (s in 0 until 1000) h.add(s * 1000L, s.toDouble())
		assertEquals((h.t(0)..999_000L step 1000).toList(), h.points(0).times())
		assertEquals(999f, h.points(0).v(h.size - 1))
	}

	@Test
	fun gapsAreMeasuredAgainstTheUsualStep() {
		val h = Hist(1)
		h.stepHintMs = 1000
		for (s in 0 until 10) h.add(s * 1000L, 1.0)
		assertEquals(1000L, h.medianStepMs())
		assertEquals(3000L, h.gapMs)
		// Asking every 5 s from now on: those steps aren't gaps.
		h.stepHintMs = 5000
		assertEquals(15_000L, h.gapMs)
		assertEquals(15_000L, h.points(0).gapMs)
	}

	@Test
	fun versionCountsChanges() {
		val h = Hist(1)
		val v0 = h.version
		h.add(1000, 1.0)
		h.replace(HistoryEntry(t = listOf(1L)))
		h.windowMs = 120_000
		// The same range again changes nothing.
		h.windowMs = 120_000
		assertEquals(v0 + 3, h.version)
	}

	@Test
	fun aLongRangeTakesAStreamPointABucketAtATime() {
		// The buckets of a range every 2 min, the newest at 0: a stream's point joins only once a step has passed.
		val h = Hist(1, 86_400_000)
		h.replace(HistoryEntry(t = listOf(-240_000L, -120_000L, 0L), v = listOf(listOf(1.0, null, 3.0))))
		assertFalse(h.appendEvery(119_999, listOf(4.0), 120_000))
		assertTrue(h.appendEvery(120_000, listOf(5.0), 120_000))
		assertFalse(h.appendEvery(200_000, listOf(6.0), 120_000))
		assertEquals(listOf(-240_000L, -120_000L, 0L, 120_000L), h.points(0).times())
		assertEquals(5f, h.points(0).v(3))
		// With nothing yet, the first point is taken.
		assertTrue(Hist(1).appendEvery(5, listOf(1.0), 120_000))
	}

	@Test
	fun aLinesGraphsAndSparksAreKeptAsAStreamKeepsThem() {
		// What the machine screen and the editor's preview both keep of a line.
		val ui = ProtocolJson.decodeFromString(
			UiDoc.serializer(),
			"""{"version":"v","service":{"id":"x","history_seconds":60},"dashboards":[{"id":"d","widgets":[
			  {"id":"d/net","type":"graph","series":[{"label":"Down"},{"label":"Up"}],"range":30},
			  {"id":"d/g","type":"group","widgets":[{"id":"d/g/temp","type":"value","spark":true}]},
			  {"id":"d/cpu","type":"value"}]},{"id":"e","widgets":[{"id":"e/load","type":"graph"}]}]}""",
		)
		fun line(t: Long, dashboard: String = "d", net: String = "[1.5,2.5]") = ProtocolJson.decodeFromString(
			DataLine.serializer(),
			"""{"t":$t,"dashboard":"$dashboard","widgets":{"d/net":{"v":$net},"d/g/temp":{"v":50},"d/cpu":{"v":3}}}""",
		)
		val hists = HashMap<String, Hist>()
		val taken = mutableListOf<Pair<String, List<Double?>>>()
		takeHists(hists, ui, line(100_000), 1000L) { w, v -> taken += w.id to v }
		// A graph and a spark, in a group too; a plain value, and another dashboard's graph, none.
		assertEquals(setOf("d/net", "d/g/temp"), hists.keys)
		assertEquals(listOf<Pair<String, List<Double?>>>("d/net" to listOf(1.5, 2.5), "d/g/temp" to listOf(50.0)), taken)
		val net = hists.getValue("d/net")
		assertEquals(2, net.seriesCount)
		// The graph's range; a spark, as far back as the service keeps.
		assertEquals(30_000L, net.windowMs)
		assertEquals(60_000L, hists.getValue("d/g/temp").windowMs)
		assertEquals(1000L, net.stepHintMs)
		assertEquals(listOf(1.5f), net.points(0).values())
		assertEquals(listOf(2.5f), net.points(1).values())
		// The next line: a point more, in the same Hist.
		takeHists(hists, ui, line(101_000), 2000L)
		assertTrue(net === hists.getValue("d/net"))
		assertEquals(listOf(100_000L, 101_000L), net.points(0).times())
		assertEquals(2000L, net.stepHintMs)
		// Another dashboard's line: its own graph's, nothing of this one's.
		takeHists(hists, ui, line(102_000, dashboard = "e"), 1000L)
		assertEquals(2, net.size)
		assertTrue("e/load" in hists)
	}
}
