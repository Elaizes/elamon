package io.github.elaizes.elamon.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/** The service's levels as the app's colour bands: green, or a widget's own colour, to amber to red. */
class ThresholdsTest {
	private val good = Color.Green
	private val warn = Color.Yellow
	private val hot = Color.Red

	@Test
	fun levelsAndBlends() {
		assertEquals(Level(Band.Good), levelOf("normal", null))
		assertEquals(Level(Band.Good), levelOf("normal", 0.0))
		// A temperature nearing its warning: normal, turned some way toward it.
		assertEquals(Level(Band.GoodToWarn, 0.025f), levelOf("normal", 0.025))
		assertEquals(Level(Band.Warn), levelOf("warning", null))
		assertEquals(Level(Band.WarnToHot, 0.333f), levelOf("warning", 0.333))
		assertEquals(Level(Band.Hot), levelOf("critical", null))
		assertEquals(Level(Band.Hot), levelOf("critical", 0.5))
		// Out of range is held to it.
		assertEquals(Level(Band.WarnToHot, 1f), levelOf("warning", 3.0))
		assertEquals(Level(Band.Good), levelOf("normal", -1.0))
		// None, or one from a newer service: no colour of its own.
		assertNull(levelOf(null, null))
		assertNull(levelOf("alarming", 0.5))
	}

	@Test
	fun bandsToColours() {
		assertEquals(good, levelOf("normal", null)!!.color(good, warn, hot))
		assertEquals(lerp(good, warn, 0.5f), levelOf("normal", 0.5)!!.color(good, warn, hot))
		assertEquals(warn, levelOf("warning", null)!!.color(good, warn, hot))
		assertEquals(lerp(warn, hot, 0.25f), levelOf("warning", 0.25)!!.color(good, warn, hot))
		assertEquals(hot, levelOf("critical", null)!!.color(good, warn, hot))
		// A load's ring stays its own colour while normal.
		val cpu = Color.Blue
		assertEquals(cpu, levelOf("normal", null)!!.color(cpu, warn, hot))
	}
}
