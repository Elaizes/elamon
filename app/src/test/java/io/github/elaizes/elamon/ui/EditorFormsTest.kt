package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import io.github.elaizes.elamon.data.EditorAction
import io.github.elaizes.elamon.data.EditorDescription
import io.github.elaizes.elamon.data.EditorKey
import io.github.elaizes.elamon.data.EditorValue

/** What the editor's fields make of what is typed, and which of an `any`'s kinds a value is. */
class EditorFormsTest {
	@Test
	fun typedTextAsValues() {
		// Whole numbers stay whole; a decimal comma is a point; what isn't one is kept as typed, for the computer to refuse.
		assertEquals(JsonPrimitive(100L), numberOf("100"))
		assertEquals(JsonPrimitive(1.5), numberOf("1,5"))
		assertEquals(JsonPrimitive("lots"), numberOf("lots"))
		assertNull(numberOf("  "))
		assertEquals(JsonPrimitive(4L), intOf(" 4 "))
		assertEquals(JsonPrimitive("4.5"), intOf("4.5"))
		assertNull(intOf(""))
		// A scalar: a bool, a number or a string.
		assertEquals(JsonPrimitive(true), scalarOf("true"))
		assertEquals(JsonPrimitive(7L), scalarOf("7"))
		assertEquals(JsonPrimitive("server"), scalarOf("server"))
		assertEquals(JsonPrimitive("NaN"), scalarOf("NaN"))
		assertNull(scalarOf(""))
		// JSON text; what isn't JSON, kept as a string.
		assertEquals(Json.parseToJsonElement("""{"a":[1,2]}"""), jsonOf("""{"a":[1,2]}"""))
		assertEquals(JsonPrimitive("{oops"), jsonOf("{oops"))
		assertNull(jsonOf(" "))
	}

	@Test
	fun valuesShownAsText() {
		assertEquals("cpu.usage", asText(JsonPrimitive("cpu.usage")))
		assertEquals("100", asText(JsonPrimitive(100)))
		assertEquals("[\"a\"]", asText(JsonArray(listOf(JsonPrimitive("a")))))
		assertEquals("", asText(null))
	}

	@Test
	fun whichKindAValueIs() {
		val desc = EditorDescription(values = listOf(EditorValue("cpu.usage", "12 %")), actions = listOf(EditorAction("mode", "Mode")))
		val number = EditorKey(kind = "number")
		val address = EditorKey(kind = "address")
		assertTrue(holds(number, JsonPrimitive(90), desc))
		assertFalse(holds(number, JsonPrimitive("cpu.usage"), desc))
		assertTrue(holds(address, JsonPrimitive("cpu.usage"), desc))
		// An address the computer hasn't listed is still text; which of the any's it is, the field decides.
		assertFalse(holds(address, JsonPrimitive("gpu.temp"), desc))
		assertTrue(holds(EditorKey(kind = "ref", to = "action"), JsonPrimitive("mode"), desc))
		assertFalse(holds(EditorKey(kind = "ref", to = "action"), JsonPrimitive("nothing"), desc))
		assertTrue(holds(EditorKey(kind = "list"), JsonArray(emptyList()), desc))
		assertTrue(holds(EditorKey(kind = "record"), JsonObject(emptyMap()), desc))
		assertFalse(holds(EditorKey(kind = "stardust"), JsonPrimitive(1), desc))
		// An item to fill, by its kind.
		assertEquals(JsonObject(emptyMap()), emptyOf(EditorKey(kind = "record")))
		assertEquals(JsonPrimitive(""), emptyOf(EditorKey(kind = "address")))
		assertEquals(JsonPrimitive(""), emptyOf(null))
	}

	@Test
	fun refsPickWhatTheComputerHas() {
		val desc = EditorDescription(actions = listOf(EditorAction("power.lock", "Lock", "power")))
		assertEquals(listOf(PickItem("power.lock", "Lock", "power.lock · power")), refItems("action", desc))
		assertEquals(emptyList(), refItems("dashboard", desc))
		assertNull(refItems("stardust", desc))
	}

	@Test
	fun anExampleInALine() {
		assertEquals("label: CPU · value: cpu.usage · max: 100", exampleLine(Json.parseToJsonElement("""{"type":"gauge","label":"CPU","value":"cpu.usage","max":100}""") as JsonObject))
	}
}
