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
		val newer = JsonObject(raw + ("types" to JsonArray(types + sparkle)) + ("macros" to JsonArray(emptyList())))
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
	fun whatTheFilesBelowSetIsFixed() {
		val d = description()
		// The stock overview: its title, icon and widgets set in elamond.toml, key by key; nothing else of it is.
		val overview = d.dashboard("overview")!!
		assertEquals(setOf("title", "icon", "widgets"), overview.below.keys)
		assertEquals(EditorFrom("elamond.toml", 143), overview.fixed("title"))
		assertEquals("elamond.toml:143", overview.fixed("title")!!.where)
		assertNull(overview.fixed("role"))
		assertTrue(overview.from.none { it.nix })
		// Only the keys below says: one the editor's file sets alone isn't, whatever other file sets the dashboard.
		val mine = EditorDashboard(
			id = "mine",
			def = obj("""{"title":"Mine","icon":"gauge","widgets":[]}"""),
			from = listOf(EditorFrom("/nix/store/0000-elamond.toml", 3, nix = true), EditorFrom("elamond.d/zz-editor.toml", 1)),
			below = mapOf("title" to EditorFrom("/nix/store/0000-elamond.toml", 4)),
			editor = true,
		)
		assertNull(mine.fixed("icon"))
		// Where it is set, the file's being in the Nix store as from says.
		assertEquals(EditorFrom("/nix/store/0000-elamond.toml", 4, nix = true), mine.fixed("title"))
		assertEquals("elamond.d/zz-editor.toml", EditorFrom("elamond.d/zz-editor.toml").where)
	}

	@Test
	fun aProfilesFormIsTheDescriptions() {
		val d = description()
		// The editor's part of a profile: its tabs and header, then, under android, the dashboards shown outside the app.
		assertEquals(listOf("dashboards", "header"), d.profileKeys.map { it.key })
		assertEquals("list", d.profileKeys[0].kind)
		assertEquals("dashboard", d.profileKeys[0].item!!.to)
		assertEquals("dashboard", d.profileKeys[1].to)
		assertEquals(listOf("widget", "tiles", "card"), d.androidKeys.map { it.key })
		assertTrue(d.androidKeys.all { it.kind == "ref" && it.to == "dashboard" && it.label!!.isNotBlank() && !it.hint.isNullOrBlank() })
		// Every key a profile's def has, the description has a form for.
		for (p in d.profiles) {
			assertEquals(p.def.keys - PROFILE_ANDROID, d.profileKeys.map { it.key }.toSet(), p.id)
			assertEquals(p.def.getValue(PROFILE_ANDROID).jsonObject.keys, d.androidKeys.map { it.key }.toSet(), p.id)
		}
	}

	@Test
	fun aProfileWriteSendsWhatChangedAlone() {
		val def = description().profile("default")!!.def
		val android = def.getValue(PROFILE_ANDROID).jsonObject
		assertEquals(obj("{}"), profileChanges(def, def))
		// A tab added ("add it to my tabs"): the tabs alone, not the header nor the Android dashboards the profile has from the files.
		val tabs = Drafts.withKey(def, PROFILE_TABS, JsonArray(def.getValue(PROFILE_TABS).jsonArray + JsonPrimitive("mine")))
		assertEquals(
			obj("""{"id":"default","profile":{"dashboards":["overview","ups","processes","sensors","control","mine"]},"base":"h"}"""),
			profileBody("default", profileChanges(def, tabs), "h"),
		)
		// One of android's: that one alone, in android.
		val widget = Drafts.withKey(def, PROFILE_ANDROID, Drafts.withKey(android, "widget", JsonPrimitive("phone")))
		assertEquals(obj("""{"android":{"widget":"phone"}}"""), profileChanges(def, widget))
		// Taken out: null, back to what the other files say.
		assertEquals(obj("""{"header":null}"""), profileChanges(def, Drafts.withKey(def, "header", null)))
		assertEquals(obj("""{"android":{"card":null}}"""), profileChanges(def, Drafts.withKey(def, PROFILE_ANDROID, Drafts.withKey(android, "card", null))))
		// Changed, and changed back: nothing of it.
		val back = Drafts.withKey(widget, PROFILE_ANDROID, android)
		assertEquals(obj("""{"dashboards":["overview"]}"""), profileChanges(def, Drafts.withKey(back, PROFILE_TABS, JsonArray(listOf(JsonPrimitive("overview"))))))
		// The fixture's write, of a profile that had none of it: all of it.
		val p = fixture("editor-profile").getValue("request").jsonObject
		assertEquals(p, profileBody("pixel-9", profileChanges(obj("{}"), p.getValue("profile").jsonObject), "9a8b7c6d5e4f3021"))
	}

	@Test
	fun aWidgetWithUseStartsFromItsTemplate() {
		val d = description()
		// The stock Sensors and CPU dashboards: tables made of the service's templates, with no type of their own.
		val processor = Drafts.widgets(d.dashboard("sensors")!!.def).map { it.jsonObject }.first { it["id"]!!.jsonPrimitive.content == "cpu" }
		assertEquals("sensor_table", processor["use"]!!.jsonPrimitive.content)
		assertNull(processor["type"])
		assertEquals("table", d.typeOf(processor))
		assertEquals(d.type("table")!!.span, d.spanOf(processor))
		assertEquals("Table · Processor", d.titleOf(processor))
		// The table's keys, what the template sets of them what they are while the widget doesn't.
		val keys = d.keysOf(processor)
		assertEquals(d.common.map { it.key } + d.type("table")!!.keys.map { it.key }, keys.map { it.key })
		assertEquals(d.templates.getValue("sensor_table")["columns"], keys.first { it.key == "columns" }.default)
		assertEquals(d.type("table")!!.keys.first { it.key == "limit" }.default, keys.first { it.key == "limit" }.default)
		val top = Drafts.widgets(d.dashboard("cpu")!!.def).map { it.jsonObject }.first { it["use"]?.jsonPrimitive?.content == "top_processes" }
		assertEquals("table", d.typeOf(top))
		assertEquals("Table · Top processes", d.titleOf(top))
		assertEquals("processes", d.valueOf(top, "icon")!!.jsonPrimitive.content)
		// A template that uses another: the nearer wins, `use` and `with` aren't keys of the widget;
		// a loop ends where it comes round; a template there isn't gives nothing.
		val chained = EditorDescription(
			common = d.common,
			types = d.types,
			templates = mapOf(
				"base" to obj("""{"type":"graph","range":300,"label":"{name}","span":2}"""),
				"mine" to obj("""{"use":"base","with":{"name":"x"},"range":60}"""),
				"a" to obj("""{"use":"b","label":"A"}"""),
				"b" to obj("""{"use":"a","type":"gauge"}"""),
			),
		)
		val w = obj("""{"use":"mine","with":{"name":"CPU"}}""")
		assertEquals(obj("""{"type":"graph","range":60,"label":"{name}","span":2}"""), chained.templateOf(w))
		assertEquals("graph", chained.typeOf(w))
		assertEquals(2, chained.spanOf(w))
		assertEquals(3, chained.spanOf(obj("""{"use":"mine","span":3}""")))
		assertEquals(JsonPrimitive(60), chained.keysOf(w).first { it.key == "range" }.default)
		assertEquals("gauge", chained.typeOf(obj("""{"use":"a"}""")))
		assertEquals("A", chained.valueOf(obj("""{"use":"a"}"""), "label")!!.jsonPrimitive.content)
		assertEquals(obj("{}"), chained.templateOf(obj("""{"use":"none"}""")))
		assertEquals(EditorDescription.DEFAULT_SPAN, chained.spanOf(obj("""{"use":"none"}""")))
		// Its own type and keys win.
		assertEquals("value", chained.typeOf(obj("""{"use":"mine","type":"value"}""")))
		assertEquals("Graph · Mine", chained.titleOf(obj("""{"use":"mine","label":"Mine"}""")))
	}

	@Test
	fun aBrokenEntryCostsItselfAlone() {
		val raw = fixture("editor")
		val whole = readEditor(raw)
		assertEquals(description(raw), whole)
		// A type, a value, a dashboard and a template the app can't read, a record broken, and a part
		// of the wrong shape: each left out, the rest as it came.
		fun plus(key: String, extra: JsonElement) = key to JsonArray(raw.getValue(key).jsonArray + extra)
		val broken = JsonObject(
			raw + listOf(
				plus("types", obj("""{"label":"No type"}""")),
				plus("values", obj("""{"now":"12 %"}""")),
				plus("dashboards", obj("""{"id":"odd","from":"elamond.toml"}""")),
				"templates" to JsonObject(raw.getValue("templates").jsonObject + ("odd" to JsonPrimitive("not a table"))),
				"records" to JsonObject(raw.getValue("records").jsonObject + ("odd" to JsonPrimitive(3))),
				"writable" to obj("""{"yes":true}"""),
			),
		)
		// Read whole, it doesn't.
		assertTrue(runCatching { ProtocolJson.decodeFromJsonElement(EditorDescription.serializer(), broken) }.isFailure)
		val d = readEditor(broken)
		assertEquals(whole.types, d.types)
		assertEquals(whole.values, d.values)
		assertEquals(whole.dashboards, d.dashboards)
		assertEquals(whole.templates, d.templates)
		assertEquals(whole.records, d.records)
		assertEquals(whole.profiles, d.profiles)
		assertEquals(whole.file, d.file)
		// The part that didn't read: as if it weren't sent.
		assertTrue(d.writable)
		// A key of a dashboard that doesn't read: that part gone, not the description.
		val keys = JsonObject(raw + ("dashboard" to obj("""{"keys":"not a list"}""")))
		assertEquals(EditorDashboardKeys(), readEditor(keys).dashboard)
		assertEquals(whole.common, readEditor(keys).common)
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
		// A preview: the draft as the editor's write would send it and its dashboard, as the service's tests send them.
		val preview = fixture("admin-preview").getValue("request").jsonObject
		val draft = preview.getValue("editor").jsonObject.getValue("dashboards").jsonObject.getValue("draft").jsonObject
		val body = previewBody(mapOf("draft" to draft), emptyMap(), "draft")
		assertEquals(preview, JsonObject(body - "widgets"))
		// And the types the app draws, as it asks for its `ui`.
		assertEquals(WIDGET_TYPES.joinToString(","), body.getValue("widgets").jsonPrimitive.content)
		val withTabs = previewBody(mapOf("draft" to draft), mapOf("pixel-9" to p.getValue("profile").jsonObject), "draft")
		assertEquals(setOf("dashboards", "profiles"), withTabs.getValue("editor").jsonObject.keys)
	}

	@Test
	fun thePreviewIsReadAsTheUi() {
		val preview = fixture("admin-preview")
		val p = previewFrom(preview.getValue("response").jsonObject)
		// The ui the asking client would get with the draft: the computer, its control and the profile its own.
		assertEquals("owner", p.ui.role)
		assertEquals("0.11.0", p.ui.service.version)
		assertTrue("editor" in p.ui.service.features)
		assertTrue(p.ui.control.enabled)
		assertEquals("default", p.ui.profile.id)
		// The draft's dashboard among the tabs, its widgets by place.
		assertTrue("draft" in p.ui.tabs)
		val shown = p.ui.dashboard("draft")!!
		assertEquals(listOf("draft/0"), shown.widgets.map { it.id })
		assertEquals("gauge", shown.widgets.single().type)
		val line = p.line!!
		assertEquals("draft", line.dashboard)
		assertEquals(12.5, line.widgets.getValue("draft/0").number)
		// Each widget of the draft and what the preview drew of it.
		val draft = Drafts.widgets(preview.getValue("request").jsonObject.getValue("editor").jsonObject.getValue("dashboards").jsonObject.getValue("draft").jsonObject)
		assertEquals(listOf(listOf("draft/0")), previewOf(draft, shown).map { cell -> cell.map { it.id } })
		// An answer without a ui, or one that isn't a ui, isn't a preview.
		assertIs<ElamondException>(runCatching { previewFrom(obj("""{"data":{}}""")) }.exceptionOrNull())
		assertIs<ElamondException>(runCatching { previewFrom(obj("""{"ui":{"dashboards":[]}}""")) }.exceptionOrNull())
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
		for (ok in listOf("mine", "a", "7", "2nd", "disk_2", "my-tab", "a_", "b-", "x".repeat(32))) assertTrue(isSettingsId(ok), ok)
		// It starts with a letter or a digit.
		for (bad in listOf("", "Mine", "my tab", "a.b", "x".repeat(33), "ü", "_x", "-x", "_", "-")) assertTrue(!isSettingsId(bad), bad)
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
