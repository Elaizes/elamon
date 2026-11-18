package io.github.elaizes.elamon.ui

import io.github.elaizes.elamon.data.Action
import io.github.elaizes.elamon.data.Choice
import io.github.elaizes.elamon.data.Param
import io.github.elaizes.elamon.data.Params
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** What is open over a computer's screen lives in the screen's store, not in what draws it: a rotation keeps it. */
class DialogsKeptTest {
	private val backup = Action(
		"backup.run",
		name = "Back up",
		params = listOf(
			Param("mode", "choice", choices = listOf(Choice("full"), Choice("quick"))),
			Param("target", "choice", choices = listOf(Choice("disk"), Choice("cloud"))),
			Param("keep", "integer"),
			Param("note", "string", optional = true),
		),
	)

	@Test
	fun aFormKeepsWhatWasGivenWithItsRequest() {
		val req = FormRequest(backup, buildJsonObject { put("mode", "full") }, "Back up", null, backup.id)
		// What the widget fixes isn't asked, nor what may be left out.
		assertEquals(listOf("target", "keep"), req.asked.map { it.id })
		assertEquals(JsonPrimitive("disk"), req.values["target"])
		assertNull(req.values["keep"])
		assertFalse(Params.complete(backup, req.params()))

		// Given in the form, it is the request's: the form drawn again after a rotation shows it still.
		req.values["target"] = JsonPrimitive("cloud")
		req.texts["keep"] = "7"
		req.values["keep"] = JsonPrimitive(7L)
		assertEquals("7", req.texts["keep"])
		val params = req.params()
		assertEquals(buildJsonObject { put("mode", "full"); put("target", "cloud"); put("keep", 7L) }, params)
		assertTrue(Params.complete(backup, params))
	}

	@Test
	fun theComputerIsForgottenOnce() {
		val kept = MachineDialogs()
		assertFalse(kept.forgetting)
		assertTrue(kept.startForgetting())
		// Tapped again before the dialog knows, or drawn again after a rotation: no second request.
		assertFalse(kept.startForgetting())
		assertTrue(kept.forgetting)
	}
}
