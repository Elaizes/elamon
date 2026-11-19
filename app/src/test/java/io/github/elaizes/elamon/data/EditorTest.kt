package io.github.elaizes.elamon.data

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** The dashboard editor against elamond/fixtures/0.11: its description, the drafts made of it, and what is sent back. */
class EditorTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	private fun fixture(name: String): JsonObject = Json.parseToJsonElement(File(dir, "0.11/$name.json").readText()).jsonObject

	private fun description(raw: JsonObject = fixture("editor")): EditorDescription = ProtocolJson.decodeFromJsonElement(EditorDescription.serializer(), raw)

	private fun obj(text: String): JsonObject = Json.parseToJsonElement(text).jsonObject

	/** Every key of [keys], and those their items and alternatives are, all the way down. */
	private fun allKeys(keys: List<EditorKey>): List<EditorKey> = keys.flatMap { k -> listOf(k) + allKeys(listOfNotNull(k.item) + k.alternatives) }

	@Test
	fun theDescriptionAsSent() {
		// elamond's own, from its tests: no editor's file yet, so no hash.
		val d = description()
		assertEquals("elamond.d/zz-editor.toml", d.file)
		assertNull(d.hash)
		assertTrue(d.writable)
		assertEquals("default", d.profile)
		// A dashboard's keys: its title must be there; its widgets are a list of widgets.
		val title = d.dashboard.keys.first { it.key == "title" }
		assertTrue(title.required)
		assertEquals("widgets", d.dashboard.keys.first { it.key == "widgets" }.kind)
		// Every widget's keys: its width among them, 1 to 4; its type is no key of the form.
		val span = d.common.first { it.key == "span" }
		assertEquals("int", span.kind)
		assertEquals(1.0, span.min)
		assertEquals(4.0, span.max)
		assertTrue(d.common.none { it.key == "type" })
		assertEquals("dashboard", d.common.first { it.key == "open" }.to)
		// Every type the app draws, each with its label, width and an example of itself.
		assertTrue(d.types.map { it.type }.containsAll(WIDGET_TYPES), "${WIDGET_TYPES - d.types.map { it.type }.toSet()}")
		for (t in d.types) {
			assertTrue(t.name.isNotBlank(), t.type)
			assertTrue((t.span ?: 0) in 1..Grid.COLUMNS, t.type)
			assertEquals(t.type, t.newWidget()["type"]!!.jsonPrimitive.content)
		}
		assertEquals(Grid.COLUMNS, d.spanOf(obj("""{"type":"graph"}""")))
		assertEquals(2, d.spanOf(obj("""{"type":"gauge"}""")))
		assertEquals(1, d.spanOf(obj("""{"type":"graph","span":1}""")))
		// The kinds of keys: each read, `of` and `record` where their kind has them, every record named described.
		val keys = allKeys(d.dashboard.keys + d.common + d.types.flatMap { it.keys } + d.records.values.flatten())
		assertTrue(keys.all { it.kind.isNotEmpty() })
		val kinds = keys.map { it.kind }.toSet()
		assertTrue(kinds.containsAll(listOf("string", "text", "id", "address", "ref", "bool", "int", "number", "scalar", "enum", "format", "list", "map", "record", "widgets", "any")), "$kinds")
		for (k in keys) {
			when (k.kind) {
				"list", "map" -> assertTrue(k.item != null, "${k.key} has no item")
				"any" -> assertTrue(k.alternatives.isNotEmpty(), "${k.key} is one of nothing")
				"record" -> assertTrue(k.record in d.records, "${k.key}: no record ${k.record}")
				"ref" -> assertTrue(k.to != null, "${k.key} names nothing")
				"enum" -> assertTrue(k.choiceValues.isNotEmpty(), "${k.key} has no choices")
			}
		}
		val values = d.type("graph")!!.keys.first { it.key == "values" }
		assertEquals("address", values.item!!.kind)
		assertEquals("column", d.type("table")!!.keys.first { it.key == "columns" }.item!!.record)
		assertEquals("action", d.type("button")!!.keys.first { it.key == "action" }.to)
		assertEquals("widgets", d.type("group")!!.keys.first { it.key == "widgets" }.kind)
		assertTrue(d.records.getValue("column").any { it.key == "field" })
		// What there is to pick from, with its reading now.
		assertTrue(d.values.isNotEmpty())
		assertTrue(d.values.all { it.address.isNotEmpty() })
		assertTrue(d.values.any { !it.now.isNullOrEmpty() })
		assertTrue(d.collections.any { it.name == "disks" && it.fields.isNotEmpty() })
		assertTrue(d.actions.any { it.id == "power.poweroff" && it.name.isNotEmpty() && it.kind == "power" })
		// The dashboards as the settings merge them, and whose file has them.
		val overview = d.dashboard("overview")!!
		assertTrue(Drafts.widgets(overview.def).isNotEmpty())
		assertEquals("elamond.toml", overview.from.first().file)
		assertTrue(!overview.editor && !overview.own)
		assertEquals("collection", d.dashboard.keys.first { it.key == "for_each" }.to)
		// The profiles: their tabs, header and Android dashboards, and who has them.
		val own = d.profile(d.profile)!!
		assertTrue(own.def["dashboards"]!!.jsonArray.isNotEmpty())
		assertTrue(own.def["android"] is JsonObject)
		assertTrue(own.clients.isNotEmpty())
	}

	@Test
	fun aNewerDescriptionStillReads() {
		// Fields nobody knows yet, a kind no form has, a type with no example: read, nothing lost.
		val raw = fixture("editor")
		val types = raw.getValue("types").jsonArray
		val sparkle = obj("""{"type":"sparkle","label":"Sparkle","keys":[{"key":"glitter","kind":"stardust","label":"Glitter","hint":"","weight":3}],"later":true}""")
		val newer = JsonObject(raw + ("types" to JsonArray(types + sparkle)) + ("templates" to JsonArray(emptyList())))
		val d = description(newer)
		val t = d.type("sparkle")!!
		assertNull(t.example)
		assertEquals(obj("""{"type":"sparkle"}"""), t.newWidget())
		val glitter = t.keys.single()
		assertEquals("stardust", glitter.kind)
		assertEquals("Glitter", glitter.name)
		// A widget of it takes the common keys and its own; those the description doesn't know of are kept apart.
		val w = obj("""{"type":"sparkle","glitter":{"a":1},"shine":"bright"}""")
		assertEquals(d.common.map { it.key } + "glitter", d.keysOf(w).map { it.key })
		assertEquals(listOf("type", "shine"), Drafts.unknownKeys(w, d.keysOf(w)))
		// No width of its own: two columns.
		assertEquals(EditorDescription.DEFAULT_SPAN, d.spanOf(w))
		// A key the app can't read at all is still a key, of a kind no form knows.
		assertEquals(EditorKey(key = "odd"), editorKey(obj("""{"key":"odd","kind":["not","a","string"]}""")))
	}

	@Test
	fun addingMovingAndWidth() {
		val d = description()
		var draft = obj("""{"title":"Mine","widgets":[{"type":"value","label":"Uptime","value":"system.uptime"}]}""")
		// An added widget is its type's example.
		draft = Drafts.add(draft, emptyList(), d.type("gauge")!!.newWidget())
		assertEquals(d.type("gauge")!!.example, Drafts.widget(draft, listOf(1)))
		draft = Drafts.add(draft, emptyList(), d.type("button")!!.newWidget())
		assertEquals(listOf("value", "gauge", "button"), Drafts.widgets(draft).map { it.jsonObject["type"]!!.jsonPrimitive.content })
		// Moved to another's place: the others close up.
		draft = Drafts.move(draft, emptyList(), 2, 0)
		assertEquals(listOf("button", "value", "gauge"), Drafts.widgets(draft).map { it.jsonObject["type"]!!.jsonPrimitive.content })
		draft = Drafts.move(draft, emptyList(), 0, 2)
		assertEquals(listOf("value", "gauge", "button"), Drafts.widgets(draft).map { it.jsonObject["type"]!!.jsonPrimitive.content })
		// Out of range: nothing moves.
		assertEquals(draft, Drafts.move(draft, emptyList(), 7, 0))
		// A width of its own, kept within the grid; its type's again without.
		draft = Drafts.span(draft, listOf(1), 4)
		assertEquals(4, d.spanOf(Drafts.widget(draft, listOf(1))!!))
		assertEquals(4, Drafts.widget(Drafts.span(draft, listOf(1), 9), listOf(1))!!["span"]!!.jsonPrimitive.content.toInt())
		draft = Drafts.span(draft, listOf(1), null)
		assertNull(Drafts.widget(draft, listOf(1))!!["span"])
		assertEquals(d.type("gauge")!!.span, d.spanOf(Drafts.widget(draft, listOf(1))!!))
		// Taken out.
		draft = Drafts.remove(draft, listOf(0))
		assertEquals(listOf("gauge", "button"), Drafts.widgets(draft).map { it.jsonObject["type"]!!.jsonPrimitive.content })
		// The dashboard's own keys are as they were.
		assertEquals("Mine", draft["title"]!!.jsonPrimitive.content)
		// A dashboard the computer has, changed: what it had stays.
		val overview = d.dashboard("overview")!!.def
		val changed = Drafts.move(Drafts.add(overview, emptyList(), d.type("text")!!.newWidget()), emptyList(), Drafts.widgets(overview).size, 0)
		assertEquals(overview.keys, changed.keys)
		assertEquals(Drafts.widgets(overview), Drafts.widgets(changed).drop(1))
	}

	@Test
	fun aGroupsChildren() {
		val d = description()
		val example = d.type("group")!!.newWidget()
		val first = Drafts.widgets(example).first().jsonObject["label"]!!.jsonPrimitive.content
		var draft = Drafts.add(obj("""{"title":"G","widgets":[]}"""), emptyList(), example)
		val group = listOf(0)
		assertEquals(first, Drafts.widget(draft, group + 0)!!["label"]!!.jsonPrimitive.content)
		// Added into the group, moved there, changed there: the dashboard's own list stays one group.
		draft = Drafts.add(draft, group, d.type("gauge")!!.newWidget())
		draft = Drafts.move(draft, group, 1, 0)
		draft = Drafts.setKey(draft, group + 0, "label", JsonPrimitive("Inner"))
		draft = Drafts.span(draft, group + 1, 1)
		assertEquals(1, Drafts.widgets(draft).size)
		val inner = Drafts.widgets(Drafts.widget(draft, group)!!)
		assertEquals(listOf("Inner", first), inner.map { it.jsonObject["label"]!!.jsonPrimitive.content })
		assertEquals("gauge", inner[0].jsonObject["type"]!!.jsonPrimitive.content)
		assertEquals(1, inner[1].jsonObject["span"]!!.jsonPrimitive.content.toInt())
		draft = Drafts.remove(draft, group + 0)
		assertEquals(listOf(first), Drafts.widgets(Drafts.widget(draft, group)!!).map { it.jsonObject["label"]!!.jsonPrimitive.content })
		// A place with nothing there changes nothing.
		assertEquals(draft, Drafts.setKey(draft, listOf(5, 0), "label", JsonPrimitive("x")))
		assertNull(Drafts.widget(draft, listOf(0, 3)))
	}

	@Test
	fun keysSetAndTakenOut() {
		var draft = obj("""{"title":"T","icon":"gauge","widgets":[{"type":"value","label":"A","value":"cpu.usage","unit":"%"}]}""")
		// Changed in its place; a new one last; taken out with null.
		draft = Drafts.setKey(draft, listOf(0), "label", JsonPrimitive("B"))
		assertEquals(listOf("type", "label", "value", "unit"), Drafts.widget(draft, listOf(0))!!.keys.toList())
		draft = Drafts.setKey(draft, listOf(0), "format", JsonPrimitive("percent"))
		assertEquals(listOf("type", "label", "value", "unit", "format"), Drafts.widget(draft, listOf(0))!!.keys.toList())
		draft = Drafts.setKey(draft, listOf(0), "unit", null)
		assertEquals(listOf("type", "label", "value", "format"), Drafts.widget(draft, listOf(0))!!.keys.toList())
		// The dashboard's own, by an empty place.
		draft = Drafts.setKey(draft, emptyList(), "icon", null)
		draft = Drafts.setKey(draft, emptyList(), "role", JsonPrimitive("admin"))
		assertEquals(listOf("title", "widgets", "role"), draft.keys.toList())
	}

	@Test
	fun aKeyTheAppDoesntKnowIsSentBack() {
		val d = description()
		val def = obj(
			"""{"title":"Mine","icon":"gauge","later":{"x":[1,2]},"widgets":[
			  {"type":"gauge","label":"CPU","value":"cpu.usage","max":100,"sparkle":{"glitter":[1,2,3]},"halo":true},
			  {"type":"sparkle","shine":"bright"}]}""",
		)
		var draft = Drafts.setKey(def, listOf(0), "label", JsonPrimitive("Processor"))
		draft = Drafts.span(draft, listOf(0), 1)
		draft = Drafts.add(draft, emptyList(), d.type("value")!!.newWidget())
		draft = Drafts.move(draft, emptyList(), 2, 0)
		val sent = dashboardBody("mine", draft, d.hash).toString()
		val back = Json.parseToJsonElement(sent).jsonObject.getValue("dashboard").jsonObject
		val gauge = Drafts.widget(back, listOf(1))!!
		assertEquals(obj("""{"glitter":[1,2,3]}"""), gauge["sparkle"])
		assertEquals(JsonPrimitive(true), gauge["halo"])
		assertEquals("Processor", gauge["label"]!!.jsonPrimitive.content)
		assertEquals(obj("""{"type":"sparkle","shine":"bright"}"""), Drafts.widget(back, listOf(2)))
		assertEquals(obj("""{"x":[1,2]}"""), back["later"])
		// Every key it was given, in the order it had them, and those set here after.
		assertEquals(listOf("type", "label", "value", "max", "sparkle", "halo", "span"), gauge.keys.toList())
	}

	@Test
	fun mistakesPointAtWidgetsAndKeys() {
		assertEquals(MistakeAt.InDashboard("mine", listOf(1), "max"), mistakeAt("dashboards.mine.widgets[1].max"))
		assertEquals(MistakeAt.InDashboard("mine", listOf(2, 0), "label"), mistakeAt("dashboards.mine.widgets[2].widgets[0].label"))
		// Deeper than a key: the key holds it.
		assertEquals(MistakeAt.InDashboard("disks-2", listOf(0), "columns"), mistakeAt("dashboards.disks-2.widgets[0].columns[1].field"))
		assertEquals(MistakeAt.InDashboard("mine", listOf(3), null), mistakeAt("dashboards.mine.widgets[3]"))
		assertEquals(MistakeAt.InDashboard("mine", emptyList(), "title"), mistakeAt("dashboards.mine.title"))
		assertEquals(MistakeAt.InDashboard("mine", emptyList(), "widgets"), mistakeAt("dashboards.mine.widgets"))
		assertEquals(MistakeAt.InDashboard("mine", emptyList(), null), mistakeAt("dashboards.mine"))
		assertEquals(MistakeAt.InProfile("pixel-9", "android.widget"), mistakeAt("profiles.pixel-9.android.widget"))
		assertEquals(MistakeAt.InProfile("pixel-9", "dashboards"), mistakeAt("profiles.pixel-9.dashboards[1]"))
		assertEquals(MistakeAt.InProfile("pixel-9", null), mistakeAt("profiles.pixel-9"))
		// A key not of letters, digits, _ and - alone comes quoted.
		assertEquals(MistakeAt.InProfile("my.phone", "android.widget"), mistakeAt("profiles.\"my.phone\".android.widget"))
		assertEquals(MistakeAt.InDashboard("mine", listOf(0), "map"), mistakeAt("dashboards.mine.widgets[0].map.\"on.ac\""))
		assertEquals(MistakeAt.Elsewhere, mistakeAt("port"))
		assertEquals(MistakeAt.Elsewhere, mistakeAt("dashboards"))
		assertEquals(MistakeAt.Elsewhere, mistakeAt(null))
		assertEquals(MistakeAt.Elsewhere, mistakeAt(""))
	}

	@Test
	fun mistakesFromTheAnswers() {
		// A key a dashboard can't have (editor-dashboard.json), and a widget's key (admin-save.json).
		val command = fixture("editor-dashboard").getValue("a_command").jsonObject.getValue("response")
		val e = ElamondException.fromResponse(422, command.toString())
		val m = mistakesOf(e).single()
		assertEquals("dashboards.mine.command", m.path)
		assertNull(m.line)
		assertEquals(MistakeAt.InDashboard("mine", emptyList(), "command"), mistakeAt(m.path))
		val saved = fixture("admin-save").getValue("bad_settings").jsonObject
		val bad = mistakesOf(ElamondException.fromResponse(422, saved.getValue("body").toString())).single()
		assertEquals(3, bad.line)
		assertEquals(MistakeAt.InDashboard("mine", listOf(0), "max"), mistakeAt(bad.path))
		// Another refusal has none.
		assertTrue(mistakesOf(ElamondException.fromResponse(409, """{"error":"changed_on_disk","hash":"0000111122223333"}""")).isEmpty())
		assertEquals("0000111122223333", ElamondException.fromResponse(409, """{"error":"changed_on_disk","hash":"0000111122223333"}""").body!!["hash"]!!.jsonPrimitive.content)
	}

	@Test
	fun requestsAreWrittenAsTheFixtures() {
		// A dashboard written whole, with the editor's file's hash.
		val dashboard = fixture("editor-dashboard")
		val request = dashboard.getValue("request").jsonObject
		assertEquals(request, dashboardBody("mine", request.getValue("dashboard").jsonObject, "8877665544332211"))
		assertEquals(EditorSaved(true, "9a8b7c6d5e4f3021", null, 5), ProtocolJson.decodeFromJsonElement(EditorSaved.serializer(), dashboard.getValue("response")))
		// A profile's tabs, and back to the files.
		val profile = fixture("editor-profile")
		val p = profile.getValue("request").jsonObject
		assertEquals(p, profileBody("pixel-9", p.getValue("profile").jsonObject, "9a8b7c6d5e4f3021"))
		val revert = profile.getValue("revert").jsonObject
		assertEquals(revert.getValue("request"), tableBody("profile", "pixel-9", "0a1b2c3d4e5f6071"))
		assertEquals("1763550300789", ProtocolJson.decodeFromJsonElement(EditorSaved.serializer(), revert.getValue("response")).backup)
		// An editor's file that isn't there yet: no hash, said as null.
		assertEquals(JsonNull, dashboardBody("mine", obj("{}"), null)["base"])
		assertEquals(obj("""{"dashboard":"mine","base":"x"}"""), tableBody("dashboard", "mine", "x"))
		// A preview: the draft as the editor's write would send it, its dashboard, the types the app draws.
		val preview = fixture("admin-preview").getValue("request").jsonObject
		val draft = preview.getValue("editor").jsonObject.getValue("dashboards").jsonObject.getValue("mine").jsonObject
		val body = previewBody(mapOf("mine" to draft), emptyMap(), "mine")
		assertEquals(JsonObject(preview - "widgets"), JsonObject(body - "widgets"))
		// The same types, in the app's order.
		assertEquals(preview.getValue("widgets").jsonPrimitive.content.split(",").toSet(), body.getValue("widgets").jsonPrimitive.content.split(",").toSet())
		val withTabs = previewBody(mapOf("mine" to draft), mapOf("pixel-9" to p.getValue("profile").jsonObject), "mine")
		assertEquals(setOf("dashboards", "profiles"), withTabs.getValue("editor").jsonObject.keys)
	}

	@Test
	fun thePreviewIsReadAsTheUi() {
		val answer = fixture("admin-preview").getValue("response").jsonObject
		// The fixture's ui leaves out what the app keeps of the computer: taken from the ui kept.
		val kept = ProtocolJson.decodeFromString(UiDoc.serializer(), File(dir, "ui.json").readText())
		val p = previewFrom(answer, kept)
		assertEquals("owner", p.ui.role)
		assertEquals(kept.service, p.ui.service)
		assertEquals(kept.profile.id, p.ui.profile.id)
		val mine = p.ui.dashboard("mine")!!
		assertEquals("gauge", mine.widgets.single().type)
		val line = p.line!!
		assertEquals("mine", line.dashboard)
		assertEquals(12.5, line.widgets.getValue("mine/w0").number)
		// Each widget of the draft and what the preview drew of it.
		val draft = Drafts.widgets(fixture("admin-preview").getValue("request").jsonObject.getValue("editor").jsonObject.getValue("dashboards").jsonObject.getValue("mine").jsonObject)
		assertEquals(listOf(listOf("mine/w0")), previewOf(draft, mine).map { cell -> cell.map { it.id } })
		// Without a ui kept, one that says nothing of the computer isn't one.
		val e = runCatching { previewFrom(answer, null) }.exceptionOrNull()
		assertIs<ElamondException>(e)
	}

	@Test
	fun theWidgetsThePreviewDrewOfEach() {
		fun w(id: String, type: String = "value") = Widget(id = id, type = type)
		val draft = Json.parseToJsonElement(
			"""[{"type":"value"},{"type":"gauge","id":"cpu"},{"type":"value","for_each":"disks"},{"type":"text","when":"ups.status"},{"type":"group","id":"g"}]""",
		).jsonArray
		// By place without an id, by its id with one, an item of each for_each, none where it isn't shown.
		val shown = Dashboard("d", widgets = listOf(w("d/0"), w("d/cpu", "gauge"), w("d/2.sda"), w("d/2.sdb"), w("d/g", "group")))
		assertEquals(
			listOf(listOf("d/0"), listOf("d/cpu"), listOf("d/2.sda", "d/2.sdb"), emptyList(), listOf("d/g")),
			previewOf(draft, shown).map { cell -> cell.map { it.id } },
		)
		// A widget whose id begins as another's isn't taken for it.
		val two = Json.parseToJsonElement("""[{"type":"value","id":"cpu"},{"type":"value","id":"cpu2"}]""").jsonArray
		assertEquals(listOf(listOf("d/cpu"), listOf("d/cpu2")), previewOf(two, Dashboard("d", widgets = listOf(w("d/cpu"), w("d/cpu2")))).map { c -> c.map { it.id } })
		// Ids no draft names (another scheme): place by place, as many of each.
		assertEquals(listOf(listOf("d/a"), listOf("d/b")), previewOf(two, Dashboard("d", widgets = listOf(w("d/a"), w("d/b")))).map { c -> c.map { it.id } })
		// No preview yet: nothing drawn of any.
		assertEquals(listOf<List<Widget>>(emptyList(), emptyList()), previewOf(two, null))
	}

	@Test
	fun idsAsTheSettingsTakeThem() {
		for (ok in listOf("mine", "a", "disk_2", "my-tab", "x".repeat(32))) assertTrue(isSettingsId(ok), ok)
		for (bad in listOf("", "Mine", "my tab", "a.b", "x".repeat(33), "ü")) assertTrue(!isSettingsId(bad), bad)
	}

	@Test
	fun exampleWidgetsKeepTheirType() {
		val example = obj("""{"label":"No type"}""")
		val t = EditorType(type = "value", example = example)
		assertEquals(obj("""{"type":"value","label":"No type"}"""), t.newWidget())
		assertEquals("type", t.newWidget().keys.first())
		val typed = EditorType(type = "gauge", example = obj("""{"type":"gauge","max":100}"""))
		assertEquals(typed.example, typed.newWidget())
	}
}
