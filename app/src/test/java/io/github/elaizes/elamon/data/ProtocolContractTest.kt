package io.github.elaizes.elamon.data

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.test.fail
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import io.github.elaizes.elamon.headerData

/**
 * The app against elamond/fixtures, which the service's own tests write: every shape of API 2 it
 * sends, with every optional field filled. A field the service sends that the app doesn't model
 * fails here, and so does a field the app models that the service never sends (a typo in a
 * @SerialName). Widgets and widget states share one shape each across their types, so what the app
 * reads is checked against all of them together: every key it reads, some widget or state has.
 */
class ProtocolContractTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	/** Every fixture and what the app reads it into. */
	private val shapes: Map<String, KSerializer<*>> = mapOf(
		"hello" to Hello.serializer(),
		"pair" to PairResponse.serializer(),
		"ui" to UiDoc.serializer(),
		"ui-default" to UiDoc.serializer(),
		"stream-open" to StreamLine.serializer(),
		"stream-data" to StreamLine.serializer(),
		"stream-data-detail" to StreamLine.serializer(),
		"stream-ui" to StreamLine.serializer(),
		"subscribe" to Subscribe.serializer(),
		"action-request" to ActionRequest.serializer(),
		"action-result" to ActionResult.serializer(),
		"rotations" to ListSerializer(Rotation.serializer()),
	)

	/** Fixtures that aren't a body the app decodes, and the test that reads them. */
	private val checkedElsewhere = mapOf(
		"errors" to "ErrorsTest: every error code and its status",
		"formats" to "FormatTest: every case the service formats",
	)

	/**
	 * Shapes of API 1 that the service's branch still writes until it switches: nothing of them is
	 * read any more. They may go at any time.
	 */
	private val apiOne = setOf("info", "sample", "sample-glance", "history", "control", "control-off", "detected", "units", "log")

	/** Objects keyed by ids rather than by field names: their values are checked together, as a list's items are. */
	private val maps = setOf("stream-data.widgets", "stream-data.history", "stream-data-detail.widgets", "stream-data-detail.history")

	/** A group's widgets are widgets: what they have, the dashboards' widgets are checked for. */
	private val asTheirParents = setOf("ui.dashboards[].widgets[].widgets")

	/** Fixtures only checked one way, that the app reads them: another fixture of the same shape has every field. */
	private val readOnly = mapOf(
		"stream-data-detail" to "stream-data has every field of a data line",
		"ui-default" to "ui has every field of a ui document",
	)

	/**
	 * Fields the app reads that the fixtures leave out, as "fixture.path.field" (a list's items, a
	 * map's values as "[]"), to the reason.
	 */
	private val leftOut: Map<String, String> = mapOf(
		"stream-data.widgets[].labels" to "the fixture's bars have no labels (none by default)",
	)

	/** No unknown keys, no coercion: whatever the app can't place is an error. */
	private val strict = Json {
		ignoreUnknownKeys = false
		coerceInputValues = false
		explicitNulls = false
		classDiscriminator = "type"
	}

	/** Every field the app declares, defaults and nulls included. */
	private val everything = Json {
		encodeDefaults = true
		explicitNulls = true
		classDiscriminator = "type"
	}

	private fun fixture(name: String): JsonElement = Json.parseToJsonElement(File(dir, "$name.json").readText())

	/** [a] and [b] as one: objects key by key, lists one after the other, a value over a missing one. */
	private fun merge(a: JsonElement, b: JsonElement): JsonElement = when {
		a is JsonObject && b is JsonObject -> JsonObject((a.keys + b.keys).associateWith { k ->
			val x = a[k]
			val y = b[k]
			if (x == null) y!! else if (y == null) x else merge(x, y)
		})
		a is JsonArray && b is JsonArray -> JsonArray(a + b)
		a is JsonNull -> b
		else -> a
	}

	private fun union(items: Collection<JsonElement>): JsonElement? = items.reduceOrNull(::merge)

	/** Keys in [app] that [sent] lacks, through objects, and all the items of lists (and [maps]) together. */
	private fun missing(app: JsonElement, sent: JsonElement, path: String, out: MutableList<String>) {
		if (path in asTheirParents) return
		when {
			app is JsonObject && sent is JsonObject && path in maps -> {
				val a = union(app.values) ?: return
				val s = union(sent.values) ?: return
				missing(a, s, "$path[]", out)
			}
			app is JsonObject && sent is JsonObject -> for ((k, v) in app) {
				val s = sent[k]
				if (s == null) out += "$path.$k" else missing(v, s, "$path.$k", out)
			}
			app is JsonArray && sent is JsonArray -> {
				val a = union(app) ?: return
				val s = union(sent) ?: return
				missing(a, s, "$path[]", out)
			}
		}
	}

	private fun <T> roundTrip(name: String, serializer: KSerializer<T>): List<String> {
		val raw = fixture(name)
		val decoded = try {
			strict.decodeFromJsonElement(serializer, raw)
		} catch (e: SerializationException) {
			return listOf("$name: the app can't read what the service sends: ${e.message}")
		}
		if (name in readOnly) return emptyList()
		val back = everything.encodeToJsonElement(serializer, decoded)
		val out = mutableListOf<String>()
		missing(back, raw, name, out)
		return out.filter { it !in leftOut }.map { "$it: the app reads it, the service never sends it" }
	}

	@Test
	fun everyFixtureHasAShape() {
		val files = dir.listFiles { f -> f.extension == "json" }.orEmpty().map { it.nameWithoutExtension }.toSet()
		assertTrue(files.isNotEmpty(), "no fixtures in $dir")
		assertEquals((files - apiOne).sorted(), (shapes.keys + checkedElsewhere.keys).sorted(), "a fixture without a shape in the app, or a shape without a fixture")
	}

	@Test
	fun fixturesMatchTheAppBothWays() {
		val problems = shapes.flatMap { (name, serializer) -> roundTrip(name, serializer) }
		if (problems.isNotEmpty()) fail(problems.joinToString("\n"))
	}

	@Test
	fun requestsAreWrittenAsTheFixtures() {
		// What the app sends, as the service's tests read it.
		val subscribe = Subscribe(dashboard = "detail", interval = 2000, sort = mapOf("detail/procs" to "mem"))
		assertEquals(fixture("subscribe"), ProtocolJson.encodeToJsonElement(Subscribe.serializer(), subscribe))
		val action = ActionRequest(JsonObject(mapOf("mode" to JsonPrimitive("server"))))
		assertEquals(fixture("action-request"), ProtocolJson.encodeToJsonElement(ActionRequest.serializer(), action))
		// Each key of a change is optional: one left out isn't sent at all.
		assertEquals("""{"dashboard":"cpu"}""", ProtocolJson.encodeToString(Subscribe.serializer(), Subscribe(dashboard = "cpu")))
		assertEquals("""{"params":{}}""", ProtocolJson.encodeToString(ActionRequest.serializer(), ActionRequest()))
	}

	@Test
	fun theUiAsDescribed() {
		val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("ui"))
		assertEquals(listOf("home"), ui.tabs)
		assertEquals(listOf("home", "header", "detail"), ui.dashboards.map { it.id })
		assertEquals("header", ui.header)
		assertEquals("1m", ui.profile.unlock.grace)
		assertEquals(false, ui.profile.android.dynamicColor)
		val home = ui.dashboard("home")!!
		val cpu = home.widgets.first { it.id == "home/cpu" }
		// A caption that holds values comes in the state; one that doesn't, in the ui.
		assertEquals(JsonPrimitive(true), cpu.caption)
		assertNull(cpu.captionText)
		assertEquals("detail", cpu.open)
		assertEquals("Readings from the fixture.", home.widgets.first { it.id == "home/note" }.text)
		val disks = home.widgets.first { it.id == "home/disks" }
		assertEquals(true, disks.collapsed)
		assertEquals(100.0, disks.columns!!.last().bar!!.max.number())
		val power = home.widgets.first { it.id == "home/power" }
		assertEquals(listOf("home/power/reboot", "home/power/off"), power.widgets!!.map { it.id })
		val links = home.widgets.first { it.id == "home/links" }
		assertEquals(JsonPrimitive("{id}"), links.rowActions!!.single().params["link"])
		val fan = ui.action("fan")!!.params.single()
		assertEquals("range", fan.kind)
		assertEquals(5.0, fan.step)
		assertEquals(50.0, fan.default.number())
		assertTrue(ui.action("process.signal")!!.params.first { it.id == "start" }.optional)
		assertEquals(true, ui.action("power.reboot")!!.unlock)
		assertEquals(listOf("server", "desktop"), ui.action("mode")!!.params.single().choices!!.map { it.value })
	}

	@Test
	fun theStatesAsSent() {
		val data = assertIs<DataLine>(ProtocolJson.decodeFromJsonElement(StreamLine.serializer(), fixture("stream-data")))
		assertEquals("home", data.dashboard)
		val w = data.widgets
		assertEquals(12.5, w.getValue("home/cpu").number)
		assertEquals("56°C · 3.10 GHz", w.getValue("home/cpu").caption)
		assertEquals(16_000_000_000.0, w.getValue("home/memory").max)
		assertEquals(0.025, w.getValue("home/temp").blend)
		assertEquals("On battery", w.getValue("home/battery").text)
		assertEquals(listOf(1500.5, 300.5), w.getValue("home/net").numbers)
		assertEquals(listOf("normal", "normal"), w.getValue("home/cores").levels)
		val facts = w.getValue("home/facts").items!!
		assertEquals(JsonPrimitive("Some CPU"), facts[0].v)
		assertEquals(3600.0, facts[1].v.number())
		assertEquals("8 cores · 12 threads", facts[2].text)
		val root = w.getValue("home/disks").rows!!.single()
		assertEquals("warning", root.level)
		assertEquals(80.0, root.f["percent"].number())
		assertEquals("btrfs", root.f["fs"].text())
		assertEquals(false, w.getValue("home/mute").v.bool())
		assertNull(w.getValue("home/mode").number)
		val player = w.getValue("home/media").players!!.single()
		assertEquals(240.0, player.length)
		assertEquals(75.0, w.getValue("home/media").volume!!.level)
		assertEquals(listOf(listOf(50.0, null)), data.history!!.getValue("home/temp").v)
		// A table's row: its fields keep their JSON types, for the parameters they fill.
		val detail = assertIs<DataLine>(ProtocolJson.decodeFromJsonElement(StreamLine.serializer(), fixture("stream-data-detail")))
		assertNull(detail.history)
		val proc = detail.widgets.getValue("detail/procs").rows!!.single()
		assertEquals(JsonPrimitive(4242), proc.f["pid"])
		assertEquals(JsonPrimitive("some"), proc.f["name"])
	}

	private fun line(name: String): String = fixture(name).toString()

	@Test
	fun streamLinesDecodeByType() {
		// The stream opens with the version of the client's ui; a ui line names another one.
		val version = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("ui")).version
		assertEquals(StreamOpen("s1a2b3c4", version), decodeStreamLine(line("stream-open")))
		assertIs<DataLine>(decodeStreamLine(line("stream-data")))
		val changed = assertIs<UiChanged>(decodeStreamLine(line("stream-ui")))
		assertTrue(changed.ui.isNotEmpty() && changed.ui != version)
		// The type need not come first.
		val late = JsonObject(fixture("stream-ui").jsonObject - "type" + ("type" to JsonPrimitive("ui"))).toString()
		assertIs<UiChanged>(decodeStreamLine(late))
	}

	@Test
	fun unknownStreamLinesAreSkipped() {
		assertNull(decodeStreamLine("""{"type":"notice","text":"hi"}"""))
		assertNull(decodeStreamLine("""{"text":"no type"}"""))
	}

	@Test
	fun brokenKnownStreamLinesFail() {
		assertFailsWith<SerializationException> { decodeStreamLine("""{"type":"data","dashboard":"home"}""") }
		assertFailsWith<SerializationException> { decodeStreamLine("""{"type":"open","ui":"x"}""") }
		assertFailsWith<SerializationException> { decodeStreamLine("""{"type":"data",""") }
	}

	@Test
	fun unknownWidgetTypesAndKeysAreRead() {
		// A type from a newer service is read as it is (and drawn as a notice); keys nobody knows are ignored.
		val ui = fixture("ui").jsonObject
		val all = ui.getValue("dashboards").jsonArray
		val home = all[0].jsonObject
		val sparkle = Json.parseToJsonElement("""{"id":"home/x","type":"sparkle","span":3,"glitter":{"a":1},"label":"New"}""")
		val dashboards = JsonArray(listOf(JsonObject(home + ("widgets" to JsonArray(home.getValue("widgets").jsonArray + sparkle)))) + all.drop(1))
		val decoded = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), JsonObject(ui + ("dashboards" to dashboards) + ("added_later" to JsonPrimitive(1))))
		val w = decoded.dashboard("home")!!.widgets.last()
		assertEquals("sparkle", w.type)
		assertEquals(3, w.span)
		// A state with keys nobody knows, of a widget nobody knows.
		val data = assertIs<DataLine>(decodeStreamLine("""{"type":"data","t":1,"dashboard":"home","widgets":{"home/x":{"sparkles":[1,2],"v":7}},"later":true}"""))
		assertEquals(7.0, data.widgets.getValue("home/x").number)
	}

	@Test
	fun productionToleratesNewFields() {
		for ((name, serializer) in shapes) ProtocolJson.decodeFromJsonElement(serializer, addField(fixture(name), name))
	}

	/** [e] with a field added to every object but those keyed by ids, whose values get one instead. */
	private fun addField(e: JsonElement, path: String): JsonElement = when (e) {
		is JsonObject -> if (path in maps) {
			JsonObject(e.mapValues { addField(it.value, "$path[]") })
		} else {
			JsonObject(e.mapValues { addField(it.value, "$path.${it.key}") } + ("added_later" to JsonPrimitive("x")))
		}
		is JsonArray -> JsonArray(e.map { addField(it, "$path[]") })
		else -> e
	}

	private fun dropping(name: String, path: String): JsonElement {
		// "dashboards[].widgets[].id": into the lists, then the field of each item.
		fun go(e: JsonElement, parts: List<String>): JsonElement {
			val part = parts.first()
			val key = part.removeSuffix("[]")
			val obj = e as JsonObject
			if (parts.size == 1) return JsonObject(obj - key)
			val child = obj.getValue(key)
			val next = if (part.endsWith("[]")) JsonArray((child as JsonArray).map { go(it, parts.drop(1)) }) else go(child, parts.drop(1))
			return JsonObject(obj + (key to next))
		}
		return go(fixture(name), path.split('.'))
	}

	@Test
	fun identityAndTimeAreRequired() {
		val required = mapOf(
			"hello" to listOf("app", "api", "id", "version"),
			"ui" to listOf("version", "service", "service.id", "dashboards[].id", "dashboards[].widgets[].id", "dashboards[].widgets[].type", "actions[].id", "actions[].params[].id", "actions[].params[].kind"),
			"stream-open" to listOf("stream", "ui"),
			"stream-data" to listOf("t", "dashboard"),
			"stream-ui" to listOf("ui"),
		)
		for ((name, paths) in required) {
			for (path in paths) {
				assertFailsWith<SerializationException>("$name.$path missing should fail, not read as a default") {
					ProtocolJson.decodeFromJsonElement(shapes.getValue(name), dropping(name, path))
				}
			}
		}
		// A table's row without its key.
		val d = fixture("stream-data-detail").jsonObject
		val procs = d.getValue("widgets").jsonObject.getValue("detail/procs").jsonObject
		val rows = JsonArray(procs.getValue("rows").jsonArray.map { JsonObject(it.jsonObject - "key") })
		val keyless = JsonObject(d + ("widgets" to JsonObject(mapOf("detail/procs" to JsonObject(procs + ("rows" to rows))))))
		assertFailsWith<SerializationException> { ProtocolJson.decodeFromJsonElement(StreamLine.serializer(), keyless) }
	}

	@Test
	fun optionalFieldsTakeTheirDefaults() {
		val hello = ProtocolJson.decodeFromJsonElement(Hello.serializer(), JsonObject(dropping("hello", "min_app").jsonObject - "widgets"))
		assertNull(hello.minApp)
		assertNull(hello.widgets)
		assertNull(ProtocolJson.decodeFromJsonElement(PairResponse.serializer(), dropping("pair", "profile")).profile)
		// A ui that says little: the default profile, no tabs, control off.
		val bare = ProtocolJson.decodeFromString(UiDoc.serializer(), """{"version":"v","service":{"id":"0123456789abcdef0123456789abcdef"}}""")
		assertEquals(Profile(), bare.profile)
		assertEquals(emptyList(), bare.service.features)
		assertEquals(false, bare.control.enabled)
		assertNull(bare.header)
		assertEquals(DEFAULT_INTERVAL_MS, bare.service.intervalMs)
		assertEquals(DEFAULT_HISTORY_SECONDS, bare.service.historySeconds)
		assertEquals(listOf("power", "signal", "actions", "units"), bare.profile.unlock.kinds)
	}

	@Test
	fun theHeadersStatesComeWithEveryLine() {
		// The header: a dashboard whose widgets are the pills under the name, whatever is on screen.
		val ui = fixture("ui").jsonObject
		val doc = strict.decodeFromJsonElement(UiDoc.serializer(), ui)
		assertEquals("header", doc.header)
		val data = assertIs<DataLine>(decodeStreamLine(line("stream-data")))
		val header = headerData(doc, data)!!
		assertEquals("header", header.dashboard)
		assertEquals(data.t, header.t)
		assertEquals(setOf("header/power"), header.widgets.keys)
		assertEquals("On battery", header.widgets.getValue("header/power").text)
		// The detail page's lines carry it too.
		val detail = assertIs<DataLine>(decodeStreamLine(line("stream-data-detail")))
		assertEquals(setOf("header/power"), headerData(doc, detail)!!.widgets.keys)
		// No header, or a line without its widgets: nothing.
		val bare = JsonObject(ui - "header" + ("dashboards" to JsonArray(ui.getValue("dashboards").jsonArray.filter { it.jsonObject["id"] != JsonPrimitive("header") })))
		assertNull(headerData(ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), bare), data))
		assertNull(headerData(doc, data.copy(widgets = mapOf("home/cpu" to WidgetState()))))
	}


	@Test
	fun theStandardDashboardsAreAllDrawn() {
		// The service's built-in dashboards, as an admin of the default profile gets them: the 0.5 tabs,
		// the header and the pages their tiles open, with no widget the app can't draw.
		val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("ui-default"))
		assertEquals(listOf("overview", "processes", "sensors", "control"), ui.tabs)
		assertEquals("header", ui.header)
		val ids = ui.dashboards.map { it.id }
		for (page in listOf("cpu", "memory", "network", "storage", "battery", "activity")) assertTrue(page in ids, "no $page page")
		assertTrue(ids.any { it.startsWith("gpu.") }, "no GPU page")
		fun all(ws: List<Widget>): List<Widget> = ws.flatMap { listOf(it) + all(it.widgets.orEmpty()) }
		val types = ui.dashboards.flatMap { all(it.widgets) }.map { it.type }.toSet()
		assertTrue(types.all { it in WIDGET_TYPES }, "types the app doesn't draw: ${types - WIDGET_TYPES.toSet()}")
	}
}
