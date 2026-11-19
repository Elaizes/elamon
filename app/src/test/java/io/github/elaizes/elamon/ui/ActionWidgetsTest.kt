package io.github.elaizes.elamon.ui

import java.io.File
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.Params
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.UiDoc
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

class ActionWidgetsTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	@Test
	fun aSliderOfManyStepsHasNoTicks() {
		assertEquals(19, sliderSteps(0.0, 100.0, 5.0))
		assertEquals(100, sliderSteps(0.0, 101.0, 1.0))
		assertEquals(0, sliderSteps(0.0, 100.0, null))
		// 0–1000 by 1: a hundred ticks reached only every tenth value; without them it reaches each.
		assertEquals(0, sliderSteps(0.0, 1000.0, 1.0))
	}

	@Test
	fun aButtonWhoseActionRunsTakesNoTap() {
		assertTrue(takesTap(enabled = true, running = false))
		assertFalse(takesTap(enabled = false, running = false))
		// Tapped again while it runs, it would ask again, and the run would be dropped.
		assertFalse(takesTap(enabled = true, running = true))
	}

	@Test
	fun aButtonShowsHowItEndedOnlyOnceItHas() {
		val done = ActionResult(exitCode = 0, output = "hello")
		assertEquals(done, shownResult(running = false, result = done))
		// Run again: the spinner in the icon's place says so, and the last outcome isn't shown meanwhile.
		assertNull(shownResult(running = true, result = done))
		assertNull(shownResult(running = false, result = null))
	}

	@Test
	fun whatTheFormGaveIsNamedInWhatAsksNext() {
		// A107: a button that leaves the mode open asked "Mode?", where a select asks "Mode: Server?".
		val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "ui.json").readText())
		val mode = ui.action("mode")!!
		val asked = Params.asked(mode, JsonObject(emptyMap()))
		assertEquals("Mode: Server", phraseWith("Mode", asked, JsonObject(mapOf("mode" to JsonPrimitive("server")))))
		// A number with its unit; several, each by its name; nothing given, the phrase alone.
		val fan = ui.action("fan")!!
		assertEquals("Fan speed: 50%", phraseWith("Fan speed", Params.asked(fan, JsonObject(emptyMap())), JsonObject(mapOf("speed" to JsonPrimitive(50)))))
		val both = JsonObject(mapOf("mode" to JsonPrimitive("desktop"), "speed" to JsonPrimitive(25)))
		assertEquals("Run: Mode Desktop, Speed 25%", phraseWith("Run", mode.params + fan.params, both))
		assertEquals("Mode", phraseWith("Mode", asked, JsonObject(emptyMap())))
		assertEquals("Mode", phraseWith("Mode", emptyList(), JsonObject(mapOf("mode" to JsonPrimitive("server")))))
		assertNull(valueWords(mode.params.single(), JsonNull))
	}

	@Test
	fun aToggleNamesTheChoiceItTurnsTo() {
		val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "0.10/ui-displays.json").readText())
		val state = ui.action("display.output.DP-1")!!.params.single()
		assertEquals("DP-1: Off", togglePhrase("DP-1", state, JsonPrimitive("off")))
		assertEquals("DP-1: On", togglePhrase("DP-1", state, JsonPrimitive("on")))
		// A choice called as the toggle is, said once; a parameter that isn't a choice, the label alone.
		val profile = ui.action("profile.set")!!.params.single()
		assertEquals("Power saver", togglePhrase("Power saver", profile, JsonPrimitive("power-saver")))
		assertEquals("Power saver: Balanced", togglePhrase("Power saver", profile, JsonPrimitive("balanced")))
		assertEquals("Mute", togglePhrase("Mute", null, JsonPrimitive(true)))
	}
}
