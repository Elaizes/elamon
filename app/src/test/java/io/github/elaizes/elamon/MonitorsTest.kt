package io.github.elaizes.elamon

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import io.github.elaizes.elamon.data.ActionRequest
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ElamondException.Kind
import io.github.elaizes.elamon.data.Params
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.Typed
import io.github.elaizes.elamon.data.UiDoc

/** A monitor's actions under Hyprland (elamond 0.10, fixtures/0.10): what is typed, and what each answer is to the app. */
class MonitorsTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	private fun fixture(name: String) = Json.parseToJsonElement(File(dir, "$name.json").readText())

	private val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("0.10/ui-displays"))

	/** actions-displays.json: each case's status and body. */
	private val answers: Map<String, JsonObject> = fixture("0.10/actions-displays").jsonObject.mapValues { it.value.jsonObject }

	private fun refusal(case: String): ElamondException {
		val a = answers.getValue(case)
		return ElamondException.fromResponse(a.getValue("status").jsonPrimitive.int, a.getValue("body").toString())
	}

	private fun state(v: String) = JsonObject(mapOf("state" to JsonPrimitive(v)))

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

	@Test
	fun theLastMonitorIsTurnedOffByTypingItsName() {
		val output = ui.action("display.output.DP-1")!!
		// Its name, not the value: "off" is never what is typed.
		assertEquals("DP-1", Params.typedText(output, state("off")))
		assertEquals(Typed.MonitorName, Params.typedKind(output, state("off")))
		// Turning it on asks for nothing typed.
		assertNull(Params.typedText(output, state("on")))
		assertNull(Params.typedKind(output, state("on")))
		// Its picture as well; a monitor that isn't the last one showing a picture, as any action.
		assertEquals("DP-1", Params.typedText(ui.action("display.dpms.DP-1")!!, state("off")))
		assertNull(Params.typedText(ui.action("display.output.eDP-1")!!, state("off")))
		// The request carries the name, as the fixture.
		val off = state("off")
		val request = ActionRequest(off, Params.typedText(output, off))
		assertEquals(fixture("0.10/action-display-typed"), ProtocolJson.encodeToJsonElement(ActionRequest.serializer(), request))
		// A station's command is typed as itself.
		val ups = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("0.7/ui-ups"))
		val cmd = JsonObject(mapOf("cmd" to JsonPrimitive("outlet.1.load.off")))
		assertEquals(Typed.Command, Params.typedKind(ups.action("nut.command.eco")!!, cmd))
	}

	@Test
	fun whatTheAnswersSay() {
		// Asked for the name the app didn't know to ask for (the other monitor went dark meanwhile): it says which.
		assertEquals(Message.Res(R.string.err_confirm_required_text, listOf("DP-1")), messageFor(refusal("confirm_required")))
		assertEquals(Message.Res(R.string.err_not_applied), messageFor(refusal("not_applied")))
		assertEquals(Message.Res(R.string.err_waiting_for_a_screen), messageFor(refusal("waiting_for_a_screen")))
		assertEquals(Message.Res(R.string.err_no_such_action), messageFor(refusal("gone")))
		assertEquals(Message.Res(R.string.err_not_allowed), messageFor(refusal("not_allowed")))
		// A name that would read as something else is shown without what makes it so.
		val odd = ElamondException.fromResponse(403, """{"error":"confirm_required","message":"DP‮-1"}""")
		assertEquals(Message.Res(R.string.err_confirm_required_text, listOf("DP-1")), messageFor(odd))
	}

	@Test
	fun doneIsTheServicesWordNotAnOutput() {
		val done = ActionResult(ok = true, output = "done")
		assertFalse(printsOutput(ui.action("display.output.DP-1")!!, done))
		assertFalse(printsOutput(ui.action("display.brightness.DP-1")!!, done))
		// The toggle and the slider show it: nothing more to say.
		assertNull(done(ui.action("display.output.DP-1")!!, state("off")))
		// A command of the settings' own prints what it printed.
		assertTrue(printsOutput(ui.action("mode")!!, ActionResult(ok = true, exitCode = 0, output = "fast")))
	}
}
