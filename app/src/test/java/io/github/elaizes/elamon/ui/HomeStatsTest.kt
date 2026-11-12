package io.github.elaizes.elamon.ui

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.StreamLine
import io.github.elaizes.elamon.data.UiDoc

/** A computer's card on the home screen: its first tab's first figures. */
class HomeStatsTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "ui.json").readText())
	private val data = ProtocolJson.decodeFromString(StreamLine.serializer(), File(fixtures, "stream-data.json").readText()) as DataLine

	@Test
	fun theFirstThreeFigures() {
		val stats = homeStats(ui, "home", data.widgets)
		assertEquals(listOf("CPU", "Memory", "Temperature"), stats.map { it.label })
		assertEquals(listOf("13%", "7.45 GiB", "56°C"), stats.map { it.text })
		// A gauge's and a meter's bars: to their max, the state's where it changes.
		assertEquals(0.125f, stats[0].fraction)
		assertEquals(0.5f, stats[1].fraction)
		// A value that isn't a share has no bar.
		assertEquals(null, stats[2].fraction)
		assertEquals(listOf("cpu", "memory", "hot"), stats.map { it.color })
		assertEquals(0.025, stats[2].blend)
	}

	@Test
	fun nothingToShowYet() {
		assertEquals(emptyList(), homeStats(ui, "home", emptyMap()))
		assertEquals(emptyList(), homeStats(ui, "gone", data.widgets))
	}
}
