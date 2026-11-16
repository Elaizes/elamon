package io.github.elaizes.elamon.data

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.test.fail
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import io.github.elaizes.elamon.Tell
import io.github.elaizes.elamon.Want
import io.github.elaizes.elamon.allWidgets
import io.github.elaizes.elamon.findWidget
import io.github.elaizes.elamon.headerData
import io.github.elaizes.elamon.tell

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
		"pair-request" to PairRequest.serializer(),
		"ui" to UiDoc.serializer(),
		"ui-default" to UiDoc.serializer(),
		"stream-open" to StreamLine.serializer(),
		"stream-data" to StreamLine.serializer(),
		"stream-data-detail" to StreamLine.serializer(),
		"stream-ui" to StreamLine.serializer(),
		"subscribe" to Subscribe.serializer(),
		"action-request" to ActionRequest.serializer(),
		"action-result" to ActionResult.serializer(),
		// What elamond 0.7 adds, in a folder of its own until the service writes them beside the others.
		"0.7/ui-ups" to UiDoc.serializer(),
		"0.7/stream-ups" to StreamLine.serializer(),
		"0.7/history" to GraphHistory.serializer(),
		"0.7/history-timeline" to TimelineHistory.serializer(),
		"0.7/action-request-typed" to ActionRequest.serializer(),
		// And what 0.8 adds.
		"0.8/hello" to Hello.serializer(),
		"0.8/ui-sources" to UiDoc.serializer(),
		"0.8/stream-sources" to StreamLine.serializer(),
		"0.8/stream-log-next" to StreamLine.serializer(),
		"0.8/subscribe-log" to Subscribe.serializer(),
		"0.8/log" to LogPage.serializer(),
	)

	/** The folders of fixtures under [dir], "" for itself. */
	private val folders = listOf("", "0.7/", "0.8/")

	/** Fixtures that aren't a body the app decodes, and the test that reads them. */
	private val checkedElsewhere = mapOf(
		"errors" to "ErrorsTest: every error code and its status",
		"formats" to "FormatTest: every case the service formats",
	)

	/** Objects keyed by ids rather than by field names: their values are checked together, as a list's items are. */
	private val maps = setOf(
		"stream-data.widgets", "stream-data.history", "stream-data-detail.widgets", "stream-data-detail.history", "0.7/stream-ups.widgets",
		"0.8/stream-sources.widgets", "0.8/stream-log-next.widgets", "0.8/subscribe-log.log",
	)

	/** A group's widgets are widgets: what they have, the dashboards' widgets are checked for. */
	private val asTheirParents = setOf("ui.dashboards[].widgets[].widgets", "0.7/ui-ups.dashboards[].widgets[].widgets")

	/** Fixtures only checked one way, that the app reads them: another fixture of the same shape has every field. */
	private val readOnly = mapOf(
		"stream-data-detail" to "stream-data has every field of a data line",
		"ui-default" to "ui has every field of a ui document",
		"0.7/ui-ups" to "its fields count as ui's (alsoSent)",
		"0.7/stream-ups" to "its fields count as stream-data's (alsoSent)",
		"0.8/ui-sources" to "its fields count as ui's (alsoSent)",
		"0.8/stream-sources" to "its fields count as stream-data's (alsoSent)",
		"0.8/stream-log-next" to "its fields count as stream-data's (alsoSent)",
		"0.8/subscribe-log" to "its fields count as subscribe's (alsoSent)",
	)

	/**
	 * Fixtures of 0.7 whose fields count as sent in a fixture of the same shape from before, which
	 * lacks them until the service writes it again at 0.7: a timeline's keys and state, `ranges`,
	 * a choice's and a request's `confirm_text`; and so of 0.8: a log's keys and state, a row action's
	 * `open`, a stream's `log`.
	 */
	private val alsoSent = mapOf(
		"ui" to listOf("0.7/ui-ups", "0.8/ui-sources"),
		"stream-data" to listOf("0.7/stream-ups", "0.8/stream-sources", "0.8/stream-log-next"),
		"action-request" to listOf("0.7/action-request-typed"),
		"subscribe" to listOf("0.8/subscribe-log"),
	)

	/**
	 * Fields the app reads that the fixtures leave out, as "fixture.path.field" (a list's items, a
	 * map's values as "[]"), to the reason.
	 */
	private val leftOut: Map<String, String> = mapOf(
		"stream-data.widgets[].labels" to "the fixture's bars have no labels (none by default)",
		"stream-data.widgets[].note" to "the fixtures' logs read the journal whole (a note says when they can't)",
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
		val sent = alsoSent[name].orEmpty().map(::fixture).fold(raw, ::merge)
		missing(back, sent, name, out)
		return out.filter { it !in leftOut }.map { "$it: the app reads it, the service never sends it" }
	}

	@Test
	fun everyFixtureHasAShape() {
		val files = folders.flatMap { f -> File(dir, f).listFiles { x -> x.extension == "json" }.orEmpty().map { f + it.nameWithoutExtension } }.toSet()
		assertTrue(files.isNotEmpty(), "no fixtures in $dir")
		assertEquals(files.sorted(), (shapes.keys + checkedElsewhere.keys).sorted(), "a fixture without a shape in the app, or a shape without a fixture")
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
		val pair = PairRequest("123456", "Galaxy S25", PreviousPairing("0123abcd", "token"))
		assertEquals(fixture("pair-request"), ProtocolJson.encodeToJsonElement(PairRequest.serializer(), pair))
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
			"0.7/history" to listOf("widget", "range", "from", "to", "step_ms"),
			"0.7/history-timeline" to listOf("widget", "range", "from", "to", "segments[].from", "segments[].to"),
			"0.8/log" to listOf("widget", "lines[].c", "lines[].t"),
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
		// As the default profile has it since 0.7.
		assertEquals(listOf("power", "nut", "signal", "actions", "units"), bare.profile.unlock.kinds)
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

	// ---- elamond 0.7 -------------------------------------------------------------------------------

	@Test
	fun theUpsUiAsDescribed() {
		val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("0.7/ui-ups"))
		assertTrue("ups" in ui.service.features)
		assertEquals(listOf("ups.eco"), ui.tabs)
		val widgets = ui.dashboard("ups.eco")!!.widgets
		val all = listOf("1h", "24h", "7d", "30d", "1y", "all")
		val power = widgets.first { it.id == "ups.eco/power" }
		assertEquals("timeline", power.type)
		assertEquals(86_400, power.range)
		assertEquals(all, power.ranges)
		val watts = widgets.first { it.id == "ups.eco/watts" }
		assertEquals(all, watts.ranges)
		assertEquals(listOf("Grid in", "Output"), watts.series!!.map { it.label })
		// A graph of a 0.6.2 service, which sends no `ranges`, offers no long ranges.
		val ui062 = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), withoutRanges(fixture("ui")))
		assertTrue(ui062.dashboards.flatMap { it.widgets }.all { it.ranges == null })
		// A station's actions: kind nut, a command that is typed in carries its text.
		val command = ui.action("nut.command.eco")!!
		assertEquals("nut", command.kind)
		val choices = command.params.single().choices!!
		assertEquals("outlet.1.load.off", choices.single { it.confirmText != null }.confirmText)
		assertEquals("outlet.1.load.off", choices.first { it.confirmText != null }.value)
		assertNull(choices.first { it.value == "outlet.2.load.off" }.confirmText)
		val limit = ui.action("nut.set.eco.experimental.battery.charge.max")!!.params.single()
		assertEquals(listOf(50.0, 100.0, 1.0), listOf(limit.min, limit.max, limit.step))
		assertTrue(ui.actions.filter { it.id.startsWith("nut.") }.all { it.kind == "nut" && it.confirm && it.unlock })
		assertTrue("nut" in ui.profile.unlock.kinds)
		// Every type of it the app draws, the timeline among them: the service sends it as it is.
		val types = ui.dashboards.flatMap { allWidgets(it.widgets) }.map { it.type }.toSet()
		assertTrue("timeline" in types && types.all { it in WIDGET_TYPES }, "types the app doesn't draw: ${types - WIDGET_TYPES.toSet()}")
	}

	@Test
	fun theUpsStatesAsSent() {
		val data = assertIs<DataLine>(decodeStreamLine(line("0.7/stream-ups")))
		val segments = data.widgets.getValue("ups.eco/power").segments!!
		assertEquals(listOf("none", "line", "none", "line", "battery"), segments.map { it.state })
		// Oldest first, each from where the one before ends, the last at the line's time.
		assertTrue(segments.zipWithNext().all { (a, b) -> a.to == b.from })
		assertEquals(data.t, segments.last().to)
		assertEquals(86_400_000L, segments.last().to - segments.first().from)
		// A source's own level, and a toggle's value read through its map.
		assertEquals("warning", data.widgets.getValue("ups.eco/status").level)
		assertEquals("outlet.1.load.on", data.widgets.getValue("ups.eco/switches/outlet.eco.1").v.text())
		// Other states have no segments.
		assertNull(data.widgets.getValue("ups.eco/watts").segments)
	}

	@Test
	fun historyAnswers() {
		val graph = ProtocolJson.decodeFromJsonElement(GraphHistory.serializer(), fixture("0.7/history"))
		assertEquals("ups.eco/watts", graph.widget)
		assertEquals("all", graph.range)
		assertEquals(3_600_000L, graph.stepMs)
		assertEquals(6, graph.t.size)
		assertEquals(graph.from, graph.t.first())
		assertEquals(graph.to, graph.t.last() + graph.stepMs)
		// A bucket without a point is missing in every series.
		assertEquals(listOf(null, null), graph.v.map { it[3] })
		assertEquals(HistoryEntry(graph.t, graph.v), graph.entry)
		val timeline = ProtocolJson.decodeFromJsonElement(TimelineHistory.serializer(), fixture("0.7/history-timeline"))
		assertEquals("7d", timeline.range)
		assertEquals(timeline.from, timeline.segments.first().from)
		assertEquals(timeline.to, timeline.segments.last().to)
		assertEquals(7 * 86_400_000L, timeline.to - timeline.from)
		// A state from a newer service, or none at all, reads as nothing known.
		val odd = ProtocolJson.decodeFromString(Segment.serializer(), """{"from":1,"to":2}""")
		assertEquals("none", odd.state)
	}

	@Test
	fun theHistoryOfAWidgetIsAskedForByIdAndRange() {
		assertEquals("/api/v1/history?widget=ups.eco%2Fwatts&range=24h", historyPath("ups.eco/watts", "24h"))
		assertEquals("/api/v1/history?widget=a%20b%26c&range=all", historyPath("a b&c", "all"))
	}

	@Test
	fun onlyTheCommandThatCutsPowerIsTypedAndOnlyExactly() {
		val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("0.7/ui-ups"))
		val eco = ui.action("nut.command.eco")!!
		fun cmd(v: String) = JsonObject(mapOf("cmd" to JsonPrimitive(v)))
		assertEquals("outlet.1.load.off", Params.typedText(eco, cmd("outlet.1.load.off")))
		assertNull(Params.typedText(eco, cmd("outlet.2.load.off")))
		// A value the action doesn't have, or none, asks for nothing; neither does an action without a typed choice.
		assertNull(Params.typedText(eco, cmd("nothing")))
		assertNull(Params.typedText(eco, JsonObject(emptyMap())))
		assertNull(Params.typedText(ui.action("nut.set.eco.experimental.battery.charge.max")!!, JsonObject(mapOf("value" to JsonPrimitive(80)))))
		// Exactly: no trimming, no other case.
		val text = "outlet.1.load.off"
		assertTrue(Params.typedMatches(text, text))
		assertFalse(Params.typedMatches("", text))
		assertFalse(Params.typedMatches(" $text", text))
		assertFalse(Params.typedMatches("$text ", text))
		assertFalse(Params.typedMatches(text.uppercase(), text))
		assertFalse(Params.typedMatches(text.dropLast(1), text))
		// What is sent is what the fixture shows.
		val typed = ActionRequest(cmd(text), Params.typedText(eco, cmd(text)))
		assertEquals(fixture("0.7/action-request-typed"), ProtocolJson.encodeToJsonElement(ActionRequest.serializer(), typed))
	}

	@Test
	fun aTypedCommandSendsItsText() {
		val typed = ActionRequest(JsonObject(mapOf("cmd" to JsonPrimitive("outlet.1.load.off"))), confirmText = "outlet.1.load.off")
		assertEquals(fixture("0.7/action-request-typed"), ProtocolJson.encodeToJsonElement(ActionRequest.serializer(), typed))
		// Without one, the request is as before 0.7.
		assertEquals(fixture("action-request"), ProtocolJson.encodeToJsonElement(ActionRequest.serializer(), ActionRequest(JsonObject(mapOf("mode" to JsonPrimitive("server"))))))
	}

	// ---- elamond 0.8 -------------------------------------------------------------------------------

	@Test
	fun theSourcesUiAsDescribed() {
		val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("0.8/ui-sources"))
		assertEquals(listOf("sources", "control"), ui.tabs)
		assertEquals(listOf("sources", "control", "journal", "unit_log.example"), ui.dashboards.map { it.id })
		val sources = ui.dashboard("sources")!!.widgets
		val mine = sources.first { it.id == "sources/journal" }
		assertEquals(listOf("log", "user", "notice", null), listOf(mine.type, mine.scope, mine.level, mine.unit))
		assertEquals(100, mine.lines)
		assertEquals(false, mine.search)
		val app = sources.first { it.id == "sources/app_log" }
		assertEquals(listOf("unit", "example.service", "info"), listOf(app.scope, app.unit, app.level))
		assertEquals(200, app.lines)
		assertEquals(true, app.search)
		assertEquals("journal", ui.dashboard("control")!!.widgets.first { it.id == "control/journal" }.open)
		// Every type of it the app draws, the log among them: the service sends it as it is.
		val types = ui.dashboards.flatMap { allWidgets(it.widgets) }.map { it.type }.toSet()
		assertTrue("log" in types && types.all { it in WIDGET_TYPES }, "types the app doesn't draw: ${types - WIDGET_TYPES.toSet()}")
		assertEquals(3, ProtocolJson.decodeFromJsonElement(Hello.serializer(), fixture("0.8/hello")).widgets)
	}

	@Test
	fun aRowActionOpensTheRowsPageWhereTheUiHasIt() {
		val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("0.8/ui-sources"))
		val links = findWidget(ui, "control/links")!!
		val (restart, journal) = links.rowActions!!
		assertEquals("unit.restart", restart.action)
		assertNull(restart.open)
		assertEquals(listOf("unit_log", "Journal"), listOf(journal.open, journal.label))
		assertNull(journal.action)
		assertEquals(JsonObject(emptyMap()), journal.params)
		// The link example has its page: both are offered, the journal opening it.
		val example = Tables.rowButtons(ui, links, Row("example"))
		assertEquals(listOf(null, "unit_log.example"), example.map { it.opens })
		assertEquals(listOf("unit.restart", null), example.map { it.runs?.id })
		// A link without one (no unit, or one this phone isn't shown): only the action.
		assertEquals(listOf(restart), Tables.rowButtons(ui, links, Row("other")).map { it.rowAction })
		// A row action with neither, or with an action this phone may not run, isn't offered at all.
		val odd = links.copy(rowActions = listOf(RowAction(label = "New"), RowAction("power.nothing", "Gone")))
		assertEquals(emptyList(), Tables.rowButtons(ui, odd, Row("example")))
		// A service before 0.8 sends no `open`: its row actions are as they were.
		val old = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("ui"))
		val oldLinks = old.dashboard("home")!!.widgets.first { it.id == "home/links" }
		assertTrue(oldLinks.rowActions!!.all { it.open == null && it.action != null })
	}

	@Test
	fun theLogsStatesAsSent() {
		val first = assertIs<DataLine>(decodeStreamLine(line("0.8/stream-sources")))
		// A log before its first line: a reset with none.
		val mine = first.widgets.getValue("sources/journal")
		assertEquals(emptyList(), mine.lines)
		assertTrue(mine.reset)
		val app = first.widgets.getValue("sources/app_log")
		assertTrue(app.reset)
		assertEquals(0, app.more)
		assertNull(app.note)
		val lines = app.lines!!
		assertEquals(listOf("info", "warning", "info"), lines.map { it.level })
		// Oldest first; the journal's time in µs, the same in ms beside it.
		assertTrue(lines.zipWithNext().all { (a, b) -> a.c < b.c })
		assertTrue(lines.all { it.c / 1000 == it.t })
		assertEquals(listOf("example.service", "example"), listOf(lines[1].unit, lines[1].id))
		// The sources' values beside them, as any.
		assertEquals(45_000.0, first.widgets.getValue("sources/zone").number)
		// A later line: what came since, and how many were left out before it.
		val next = assertIs<DataLine>(decodeStreamLine(line("0.8/stream-log-next")))
		val system = next.widgets.getValue("journal/system")
		assertFalse(system.reset)
		assertEquals(12, system.more)
		assertEquals(listOf("warning", "error"), system.lines!!.map { it.level })
	}

	@Test
	fun olderLinesAsAnswered() {
		val page = ProtocolJson.decodeFromJsonElement(LogPage.serializer(), fixture("0.8/log"))
		assertEquals("journal/system", page.widget)
		assertEquals(listOf("warning", "error", "critical"), page.lines.map { it.level })
		assertTrue(page.lines.zipWithNext().all { (a, b) -> a.c < b.c })
		// The next page starts where this one stopped looking, before its oldest line.
		assertEquals(1_699_999_612_345_678L, page.before)
		assertTrue(page.before!! < page.lines.first().c)
		// A line the journal has no unit for.
		val kernel = page.lines.last()
		assertNull(kernel.unit)
		assertEquals("kernel", kernel.id)
		// The end of the journal: no `before`.
		assertNull(ProtocolJson.decodeFromString(LogPage.serializer(), """{"widget":"journal/system","lines":[],"before":null}""").before)
	}

	@Test
	fun olderLinesAreAskedForByWidgetAndFilter() {
		assertEquals(
			"/api/v1/log?widget=journal%2Fsystem&before=1699999612345678&level=error&search=no%20space",
			logPath("journal/system", 1_699_999_612_345_678L, "error", "no space"),
		)
		// The newest, by the widget's own level, without a search.
		assertEquals("/api/v1/log?widget=journal%2Fsystem", logPath("journal/system", null, null, ""))
	}

	@Test
	fun aLogsFilterIsToldAsTheFixture() {
		// The journal shows errors; the app's log is to show errors that say "timeout", the journal its own again.
		val ui = ProtocolJson.decodeFromJsonElement(UiDoc.serializer(), fixture("0.8/ui-sources"))
		val app = findWidget(ui, "sources/app_log")!!
		val journal = findWidget(ui, "sources/journal")!!
		val sent = Want("sources", 1000).filtered(journal, level = "error")
		val want = sent.filtered(app, level = "error", search = "timeout").filtered(journal, level = journal.level)
		val change = assertIs<Tell.Change>(tell(sent, want, ui, Limits.streamReadTimeoutMs(1000))).change
		assertEquals(fixture("0.8/subscribe-log"), ProtocolJson.encodeToJsonElement(Subscribe.serializer(), change))
	}
}

/** [e] as a service before 0.7 sends it: without the `ranges` it added to graphs. */
internal fun withoutRanges(e: JsonElement): JsonElement = when (e) {
	is JsonObject -> JsonObject(e.filterKeys { it != "ranges" }.mapValues { withoutRanges(it.value) })
	is JsonArray -> JsonArray(e.map { withoutRanges(it) })
	else -> e
}
