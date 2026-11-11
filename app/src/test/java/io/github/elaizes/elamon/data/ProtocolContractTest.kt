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
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject

/**
 * The app against elamond/fixtures, which the service's own tests write: every shape it sends, with
 * every optional field filled. A field the service sends that the app doesn't model fails here, and
 * so does a field the app models that the service never sends (a typo in a @SerialName). A field
 * sent under its new name and its old one (links and services) is read by either.
 */
class ProtocolContractTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	/** Every fixture and what the app reads it into. */
	private val shapes: Map<String, KSerializer<*>> = mapOf(
		"hello" to Hello.serializer(),
		"pair" to PairResponse.serializer(),
		"info" to Info.serializer(),
		"sample" to Sample.serializer(),
		"sample-glance" to Sample.serializer(),
		"history" to History.serializer(),
		"control" to ControlInfo.serializer(),
		"action-result" to ActionResult.serializer(),
		"detected" to ListSerializer(Listener.serializer()),
		"units" to ListSerializer(UnitInfo.serializer()),
		"log" to ListSerializer(LogEntry.serializer()),
	)

	/**
	 * Fields the service sends that the app leaves alone on purpose, as "fixture.path.field" (a list's
	 * items as "[]") to the reason. Empty while the app models everything.
	 */
	private val ignored: Map<String, String> = emptyMap()

	/**
	 * Fields the app reads that a fixture leaves out on purpose, as "fixture.path.field" to the reason;
	 * another fixture has each of them.
	 */
	private val leftOut: Map<String, String> = mapOf(
		"sample-glance.media" to "a glance phone gets no media",
		"sample-glance.process_count" to "nor processes",
		"sample-glance.processes" to "nor processes",
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

	private fun withoutIgnored(e: JsonElement, path: String): JsonElement = when (e) {
		is JsonObject -> JsonObject(e.filterKeys { "$path.$it" !in ignored }.mapValues { (k, v) -> withoutIgnored(v, "$path.$k") })
		is JsonArray -> JsonArray(e.map { withoutIgnored(it, "$path[]") })
		else -> e
	}

	/** Keys in [app] that [sent] lacks, through objects and the first item of lists. */
	private fun missing(app: JsonElement, sent: JsonElement, path: String, out: MutableList<String>) {
		when {
			app is JsonObject && sent is JsonObject -> for ((k, v) in app) {
				val a = sent[k]
				if (a == null) out += "$path.$k" else missing(v, a, "$path.$k", out)
			}
			app is JsonArray && sent is JsonArray && app.isNotEmpty() && sent.isNotEmpty() -> missing(app[0], sent[0], "$path[]", out)
		}
	}

	private fun <T> roundTrip(name: String, serializer: KSerializer<T>): List<String> {
		val raw = fixture(name)
		val decoded = try {
			strict.decodeFromJsonElement(serializer, withoutIgnored(raw, name))
		} catch (e: SerializationException) {
			return listOf("$name: the app can't read what the service sends: ${e.message}")
		}
		val back = everything.encodeToJsonElement(serializer, decoded)
		val out = mutableListOf<String>()
		missing(back, raw, name, out)
		return out.filter { it !in leftOut }.map { "$it: the app reads it, the service never sends it" }
	}

	/** Fixtures that aren't a body the app decodes, and the test that reads them. */
	private val checkedElsewhere = mapOf("errors" to "ErrorsTest: every error code and its status")

	@Test
	fun everyFixtureHasAShape() {
		val files = dir.listFiles { f -> f.extension == "json" }.orEmpty().map { it.nameWithoutExtension }.toSet()
		assertTrue(files.isNotEmpty(), "no fixtures in $dir")
		assertEquals(files.sorted(), (shapes.keys + checkedElsewhere.keys).sorted(), "a fixture without a shape in the app, or a shape without a fixture")
	}

	@Test
	fun fixturesMatchTheAppBothWays() {
		val problems = shapes.flatMap { (name, serializer) -> roundTrip(name, serializer) }
		if (problems.isNotEmpty()) fail(problems.joinToString("\n"))
	}

	@Test
	fun ignoredFieldsStillExist() {
		// An entry for a field the service dropped would hide nothing; keep the list honest.
		for (path in ignored.keys) {
			val name = path.substringBefore('.')
			var e: JsonElement? = fixture(name)
			for (part in path.substringAfter('.').split('.')) {
				val key = part.removeSuffix("[]")
				e = (e as? JsonObject)?.get(key)
				if (part.endsWith("[]")) e = (e as? JsonArray)?.firstOrNull()
			}
			assertTrue(e != null, "$path is allowed to be ignored but the service doesn't send it")
		}
	}

	@Test
	fun leftOutFieldsAreLeftOutAndSentElsewhere() {
		for (path in leftOut.keys) {
			val name = path.substringBefore('.')
			val field = path.substringAfter('.')
			assertTrue(field !in fixture(name).jsonObject, "$path is said to be left out, but $name has it")
			assertTrue(field in fixture("sample").jsonObject, "$path is in no fixture at all")
		}
	}

	@Test
	fun aGlancePhonesSampleHasTheBasicsOnly() {
		val glance = ProtocolJson.decodeFromJsonElement(Sample.serializer(), fixture("sample-glance"))
		assertNull(glance.processes)
		assertNull(glance.processCount)
		assertNull(glance.media)
		assertEquals(emptyList(), glance.displays)
		assertEquals(emptyList(), glance.links)
		assertTrue(glance.gpus.isNotEmpty() && glance.disks.isNotEmpty() && glance.net.isNotEmpty())
	}

	@Test
	fun rolesAndTheLog() {
		val pair = ProtocolJson.decodeFromJsonElement(PairResponse.serializer(), fixture("pair"))
		assertEquals("admin", pair.role)
		val control = ProtocolJson.decodeFromJsonElement(ControlInfo.serializer(), fixture("control"))
		assertEquals("admin", control.role)
		assertEquals("processes", control.allowed?.first())
		// What elamond before 0.5 sends: no role, nothing allowed by name.
		val old = ProtocolJson.decodeFromJsonElement(ControlInfo.serializer(), without("control", "role", "allowed"))
		assertNull(old.role)
		assertNull(old.allowed)
		assertNull(ProtocolJson.decodeFromJsonElement(PairResponse.serializer(), without("pair", "role")).role)
		val log = ProtocolJson.decodeFromJsonElement(ListSerializer(LogEntry.serializer()), fixture("log"))
		assertEquals(listOf("ok", "not_allowed"), log.map { it.result })
		assertTrue(log.first().t > log.last().t, "the log comes newest first")
		assertNull(log.last().message)
		// An entry without its time is broken, not from 1970.
		val timeless = JsonArray(listOf(JsonObject(fixture("log").jsonArray.first().jsonObject - "t")))
		assertFailsWith<SerializationException> {
			ProtocolJson.decodeFromJsonElement(ListSerializer(LogEntry.serializer()), timeless)
		}
	}

	@Test
	fun strictDecodingCatchesAnUnknownField() {
		val sample = fixture("sample").jsonObject
		val extra = JsonObject(sample + ("brand_new" to JsonPrimitive(1)))
		assertFailsWith<SerializationException> { strict.decodeFromJsonElement(Sample.serializer(), extra) }
	}

	@Test
	fun productionToleratesNewFields() {
		for ((name, serializer) in shapes) {
			val raw = fixture(name)
			ProtocolJson.decodeFromJsonElement(serializer, addField(raw))
		}
	}

	private fun addField(e: JsonElement): JsonElement = when (e) {
		is JsonObject -> JsonObject(e.mapValues { addField(it.value) } + ("added_later" to JsonPrimitive("x")))
		is JsonArray -> JsonArray(e.map(::addField))
		else -> e
	}

	private fun dropping(name: String, path: String): JsonElement {
		// "temps[].value": into the list, then the field of each item.
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
			"sample" to listOf("t", "uptime", "gpus[].id", "temps[].value", "temps[].kind", "net[].kind", "net[].name", "disks[].mount", "displays[].connector", "processes[].pid", "links[].id", "services[].id", "media.players[].id"),
			"history" to listOf("t", "gpus[].id"),
			"info" to listOf("id", "api", "cpu_logical_cores", "memory_total", "boot_time", "gpus[].id"),
			"hello" to listOf("app", "api", "id", "version"),
			"control" to listOf("enabled", "actions[].id"),
		)
		for ((name, paths) in required) {
			for (path in paths) {
				val broken = dropping(name, path)
				assertFailsWith<SerializationException>("$name.$path missing should fail, not read as a default") {
					ProtocolJson.decodeFromJsonElement(shapes.getValue(name), broken)
				}
			}
		}
	}

	@Test
	fun olderServicesGetFallbacks() {
		// What 0.2 and 0.3 sent: no min_app, interval_ms or history_seconds.
		val info = dropping("info", "min_app").jsonObject - "interval_ms" - "history_seconds"
		val decoded = ProtocolJson.decodeFromJsonElement(Info.serializer(), JsonObject(info))
		assertNull(decoded.minApp)
		assertEquals(DEFAULT_INTERVAL_MS, decoded.intervalMs)
		assertEquals(DEFAULT_HISTORY_SECONDS, decoded.historySeconds)
		val hello = ProtocolJson.decodeFromJsonElement(Hello.serializer(), dropping("hello", "min_app"))
		assertNull(hello.minApp)
	}

	private fun without(name: String, vararg keys: String) = JsonObject(fixture(name).jsonObject - keys.toSet())

	@Test
	fun renamedFieldsAreReadByEitherName() {
		val info = { e: JsonElement -> ProtocolJson.decodeFromJsonElement(Info.serializer(), e) }
		val sample = { e: JsonElement -> ProtocolJson.decodeFromJsonElement(Sample.serializer(), e) }
		// elamond 0.5 sends both names; 0.4 only the old ones, and a later API only the new.
		assertEquals("9.9.9", info(fixture("info")).elamondVersion)
		assertEquals("9.9.9", info(without("info", "service_version")).elamondVersion)
		assertEquals("9.9.9", info(without("info", "service_version")).elamondVersion)
		assertFailsWith<SerializationException>("info without a version should fail") {
			info(without("info", "service_version", "service_version"))
		}
		for (kept in listOf(fixture("sample"), without("sample", "links"), without("sample", "services"))) {
			assertEquals(listOf("Some link"), sample(kept).links.map { it.name })
		}
		assertEquals(emptyList(), sample(without("sample", "links", "services")).links)
	}

	private fun line(type: String, name: String): String =
		JsonObject(mapOf("type" to JsonPrimitive(type)) + fixture(name).jsonObject).toString()

	@Test
	fun streamLinesDecodeByType() {
		assertIs<Info>(decodeStreamLine(line("info", "info")))
		assertIs<History>(decodeStreamLine(line("history", "history")))
		val sample = assertIs<Sample>(decodeStreamLine(line("sample", "sample")))
		assertEquals(1, sample.processes?.size)
		// The type need not come first.
		val late = JsonObject(fixture("info").jsonObject + ("type" to JsonPrimitive("info"))).toString()
		assertIs<Info>(decodeStreamLine(late))
	}

	@Test
	fun unknownStreamLinesAreSkipped() {
		assertNull(decodeStreamLine("""{"type":"notice","text":"hi"}"""))
		assertNull(decodeStreamLine("""{"text":"no type"}"""))
	}

	@Test
	fun brokenKnownStreamLinesFail() {
		assertFailsWith<SerializationException> { decodeStreamLine("""{"type":"sample","uptime":5}""") }
		assertFailsWith<SerializationException> { decodeStreamLine("""{"type":"sample",""") }
	}
}
