package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Display
import io.github.elaizes.elamon.data.Mode

class DisplayTextTest {
	private fun display(connector: String, connected: Boolean = true, active: Boolean = false, vendor: String? = null, model: String? = null) =
		Display(connector, connected = connected, active = active, vendor = vendor, model = model)

	private fun counts(connected: Int, on: Int) =
		listOf(Words.Count(R.plurals.displays_count_connected, connected), Words.Count(R.plurals.displays_count_on, on))

	@Test
	fun countsConnectedAndOn() {
		val laptop = display("eDP-1", active = true)
		val blanked = display("DP-1")
		val freePort = display("HDMI-A-1", connected = false)
		assertEquals(counts(connected = 2, on = 1), displaysSummary(listOf(laptop, blanked, freePort)))
		assertEquals(counts(connected = 1, on = 1), displaysSummary(listOf(laptop)))
		assertEquals(counts(connected = 2, on = 0), displaysSummary(listOf(blanked, display("DP-2"))))
	}

	@Test
	fun noneConnectedHasItsOwnWords() {
		val none = listOf(Words.Str(R.string.displays_none))
		assertEquals(none, displaysSummary(emptyList()))
		// Free ports alone aren't monitors.
		assertEquals(none, displaysSummary(listOf(display("HDMI-A-1", connected = false), display("DP-2", connected = false))))
	}

	@Test
	fun names() {
		assertEquals("Dell U2720Q", displayName(display("DP-1", vendor = "Dell", model = "U2720Q")))
		assertEquals("Dell", displayName(display("DP-1", vendor = "Dell")))
		assertEquals("U2720Q", displayName(display("DP-1", vendor = " ", model = "U2720Q")))
		assertEquals("eDP-1", displayName(display("eDP-1")))
	}

	@Test
	fun modes() {
		assertEquals("1920 × 1080 · 180 Hz", modeText(Mode(1920, 1080, 180f)))
		assertEquals("3840 × 2160 · 59.94 Hz", modeText(Mode(3840, 2160, 59.94f)))
	}
}
