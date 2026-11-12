package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import io.github.elaizes.elamon.data.DataLine

/** Whether the readings on screen are the stream's, to be called live, or kept from an earlier look. */
class ReadingsLiveTest {
	private val day = 86_400_000L

	@Test
	fun theDashboardTheStreamSends() {
		val data = mapOf("overview" to DataLine(10 * day, "overview"), "header" to DataLine(10 * day, "header"))
		assertTrue(readingsLive(data, "overview", 1000))
	}

	@Test
	fun keptFromAnEarlierLookUntilItsOwnCome() {
		// Switched to Processes: what shows is from yesterday, or a minute ago, while Overview's lines still come.
		val data = mapOf(
			"overview" to DataLine(10 * day, "overview"),
			"processes" to DataLine(9 * day, "processes"),
			"sensors" to DataLine(10 * day - 60_000, "sensors"),
		)
		assertFalse(readingsLive(data, "processes", 1000))
		assertFalse(readingsLive(data, "sensors", 1000))
		// Never read, nothing live.
		assertFalse(readingsLive(data, "control", 1000))
		// Its own line came: live.
		assertTrue(readingsLive(data + ("processes" to DataLine(10 * day + 1000, "processes")), "processes", 1000))
	}

	@Test
	fun aTabLeftAMomentAgoIsStillLive() {
		// Back within two lines: its readings are as new as the stream's.
		val data = mapOf("overview" to DataLine(5000, "overview"), "sensors" to DataLine(3000, "sensors"))
		assertTrue(readingsLive(data, "sensors", 1000))
		assertFalse(readingsLive(data + ("overview" to DataLine(5001, "overview")), "sensors", 1000))
		// Lines every 30 s: a minute behind is two of them.
		assertTrue(readingsLive(mapOf("a" to DataLine(60_000, "a"), "b" to DataLine(0, "b")), "b", 30_000))
	}
}
