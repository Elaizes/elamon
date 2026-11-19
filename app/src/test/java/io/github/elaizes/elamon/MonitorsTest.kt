package io.github.elaizes.elamon

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ElamondException.Kind
import io.github.elaizes.elamon.data.ProtocolJson

/** A monitor's actions under Hyprland (elamond 0.10, fixtures/0.10): what each answer is to the app. */
class MonitorsTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	/** actions-displays.json: each case's status and body. */
	private val answers: Map<String, JsonObject> =
		Json.parseToJsonElement(File(dir, "0.10/actions-displays.json").readText()).jsonObject.mapValues { it.value.jsonObject }

	private fun refusal(case: String): ElamondException {
		val a = answers.getValue(case)
		return ElamondException.fromResponse(a.getValue("status").jsonPrimitive.int, a.getValue("body").toString())
	}

	@Test
	fun everyCaseIsReadAsTheServiceMeansIt() {
		assertEquals(setOf("done", "confirm_required", "not_applied", "waiting_for_a_screen", "gone", "not_allowed"), answers.keys)
		val done = ProtocolJson.decodeFromJsonElement(ActionResult.serializer(), answers.getValue("done").getValue("body"))
		assertEquals(ActionResult(ok = true, output = "done"), done)
		// The name to type comes beside the code.
		val typed = refusal("confirm_required")
		assertEquals(Kind.Forbidden, typed.kind)
		assertEquals("DP-1", typed.detail)
		for (case in listOf("not_applied", "waiting_for_a_screen")) {
			val e = refusal(case)
			assertEquals(Kind.Failed, e.kind, case)
			assertEquals(case, e.detail, case)
		}
		assertEquals(Kind.NotFound, refusal("gone").kind)
		assertEquals(Kind.NotAllowed, refusal("not_allowed").kind)
	}
}
