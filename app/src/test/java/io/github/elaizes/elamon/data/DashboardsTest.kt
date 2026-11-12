package io.github.elaizes.elamon.data

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Where widgets go, what tables show, and what actions are sent: what the screens do apart from drawing. */
class DashboardsTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "ui.json").readText())
	private val data = ProtocolJson.decodeFromString(StreamLine.serializer(), File(fixtures, "stream-data.json").readText()) as DataLine

	// ---- the grid ------------------------------------------------------------------------------

	@Test
	fun fourColumnsOrTwoOnANarrowScreen() {
		assertEquals(2, Grid.columns(411))
		assertEquals(2, Grid.columns(599))
		assertEquals(4, Grid.columns(600))
		assertEquals(4, Grid.columns(900))
	}

	@Test
	fun spansHalvedRoundingUp() {
		assertEquals(listOf(1, 1, 2, 2), (1..4).map { Grid.span(it, 2) })
		assertEquals(listOf(1, 2, 3, 4), (1..4).map { Grid.span(it, 4) })
		// Out of range is held to it.
		assertEquals(1, Grid.span(0, 4))
		assertEquals(4, Grid.span(9, 4))
		assertEquals(2, Grid.span(9, 2))
	}

	@Test
	fun aWidgetThatDoesntFitStartsARow() {
		// 2 + 2 | 4 | 1 + 1 + 2 | 3 + 1 | 4
		assertEquals(
			listOf(listOf(0 to 2, 1 to 2), listOf(2 to 4), listOf(3 to 1, 4 to 1, 5 to 2), listOf(6 to 3, 7 to 1), listOf(8 to 4)),
			Grid.place(listOf(2, 2, 4, 1, 1, 2, 3, 1, 4), 4),
		)
		// 3 then 2 doesn't fit: a row of its own.
		assertEquals(listOf(listOf(0 to 3), listOf(1 to 2)), Grid.place(listOf(3, 2), 4))
		assertEquals(emptyList(), Grid.place(emptyList(), 4))
	}

	@Test
	fun theOverviewOnAPhoneAsIn05() {
		// CPU and memory side by side, a GPU beside the temperature, the rest each a row; power buttons two to a row.
		val overview = listOf(2, 2, 2, 2, 4, 4, 4)
		assertEquals(
			listOf(listOf(0 to 1, 1 to 1), listOf(2 to 1, 3 to 1), listOf(4 to 2), listOf(5 to 2), listOf(6 to 2)),
			Grid.place(overview, 2),
		)
		assertEquals(listOf(listOf(0 to 1, 1 to 1), listOf(2 to 1, 3 to 1)), Grid.place(listOf(1, 1, 1, 1), 2))
		// On a wide screen the four power buttons share a row.
		assertEquals(listOf(listOf(0 to 1, 1 to 1, 2 to 1, 3 to 1)), Grid.place(listOf(1, 1, 1, 1), 4))
	}

	@Test
	fun theFixturesHomeInTheGrid() {
		val home = ui.dashboard("home")!!.widgets
		val rows = Grid.place(home.map { it.span }, 2)
		// Every widget placed once, in order.
		assertEquals(home.indices.toList(), rows.flatten().map { it.first })
		assertTrue(rows.all { r -> r.sumOf { it.second } <= 2 })
	}

	@Test
	fun aTableThatHidesWhileEmpty() {
		val disks = ui.dashboard("home")!!.widgets.first { it.id == "home/disks" }
		assertTrue(shown(disks, null))
		assertTrue(shown(disks, data.widgets["home/disks"]))
		assertFalse(shown(disks, WidgetState(rows = emptyList(), total = 0)))
		assertFalse(shown(disks, WidgetState()))
		// One that doesn't hide stays.
		val links = ui.dashboard("home")!!.widgets.first { it.id == "home/links" }
		assertTrue(shown(links, WidgetState(rows = emptyList())))
	}

	// ---- tables --------------------------------------------------------------------------------

	private fun row(key: String, vararg f: Pair<String, Any?>) = Row(key, f = buildJsonObject {
		for ((k, v) in f) when (v) {
			null -> put(k, JsonNull)
			is String -> put(k, v)
			is Number -> put(k, v)
			is Boolean -> put(k, v)
		}
	})

	private val procs = listOf(
		row("1", "name" to "firefox", "cpu" to 12.5, "mem" to 900, "user" to "alice"),
		row("2", "name" to "Code", "cpu" to 30.0, "mem" to 400, "user" to "alice"),
		row("3", "name" to "systemd", "cpu" to 0.0, "mem" to 50, "user" to "root"),
		row("4", "name" to "kworker/0:1", "mem" to 0),
	)
	private val columns = listOf(Column("name", grow = true), Column("cpu", format = "percent"), Column("mem", format = "bytes"), Column("user", format = "text"))

	@Test
	fun searchLooksInTheTextColumnsAnyCase() {
		assertEquals(listOf("name", "user"), Tables.textColumns(columns).map { it.field })
		assertEquals(listOf("1"), Tables.search(procs, columns, "FIRE").map { it.key })
		assertEquals(listOf("3"), Tables.search(procs, columns, "root").map { it.key })
		assertEquals(listOf("1", "2"), Tables.search(procs, columns, " alice ").map { it.key })
		// Figures aren't searched: 12.5 is no text.
		assertEquals(emptyList(), Tables.search(procs, columns, "12.5").map { it.key })
		assertEquals(procs, Tables.search(procs, columns, "  "))
	}

	@Test
	fun sortByNumbersOrWordsWhatLacksTheFieldLast() {
		assertEquals(listOf("2", "1", "3", "4"), Tables.sort(procs, "cpu", desc = true).map { it.key })
		assertEquals(listOf("3", "1", "2", "4"), Tables.sort(procs, "cpu", desc = false).map { it.key })
		assertEquals(listOf("1", "2", "3", "4"), Tables.sort(procs, "mem", desc = true).map { it.key })
		assertEquals(listOf("2", "1", "4", "3"), Tables.sort(procs, "name", desc = false).map { it.key })
		// Rows the field doesn't tell apart keep the service's order.
		assertEquals(listOf("1", "2", "3", "4"), Tables.sort(procs, "nothing", desc = true).map { it.key })
	}

	@Test
	fun barsFullAtANumberAFieldOrTheTop() {
		val disks = ui.dashboard("home")!!.widgets.first { it.id == "home/disks" }
		val percent = disks.columns!!.last()
		val root = data.widgets.getValue("home/disks").rows!!.single()
		assertEquals(0.8f, Tables.barFraction(root, percent, listOf(root)))
		// At a field of the row: a sensor at its critical limit.
		val temp = Column("value", format = "temp", bar = Bar(JsonPrimitive("crit")))
		val sensor = row("a", "value" to 42.0, "crit" to 84.0)
		assertEquals(0.5f, Tables.barFraction(sensor, temp, listOf(sensor)))
		assertNull(Tables.barFraction(row("b", "value" to 42.0), temp, emptyList()))
		// "top": a quarter above the largest in the column.
		val rpm = Column("rpm", bar = Bar(JsonPrimitive("top")))
		val fans = listOf(row("f1", "rpm" to 2000), row("f2", "rpm" to 1000))
		assertEquals(0.8f, Tables.barFraction(fans[0], rpm, fans))
		assertEquals(0.4f, Tables.barFraction(fans[1], rpm, fans))
		// Held within the bar; without a value, none.
		assertEquals(1f, Tables.barFraction(row("x", "percent" to 150.0), percent, emptyList()))
		assertNull(Tables.barFraction(row("y"), percent, emptyList()))
		assertNull(Tables.barFraction(fans[0], Column("rpm"), fans))
	}

	// ---- actions -------------------------------------------------------------------------------

	@Test
	fun rowParamsFilledWithTheirJsonTypes() {
		val quit = ui.dashboard("detail")!!.widgets.first { it.id == "detail/procs" }.rowActions!!.single()
		val proc = (ProtocolJson.decodeFromString(StreamLine.serializer(), File(fixtures, "stream-data-detail.json").readText()) as DataLine)
			.widgets.getValue("detail/procs").rows!!.single()
		val params = Params.fill(quit.params, proc.f)
		assertEquals(JsonObject(mapOf("pid" to JsonPrimitive(4242), "start" to JsonPrimitive(1700000100), "signal" to JsonPrimitive("term"))), params)
		assertTrue(Params.complete(ui.action("process.signal")!!, params))
		// A field the row lacks leaves the parameter out (start is optional).
		val noStart = Params.fill(quit.params, JsonObject(mapOf("pid" to JsonPrimitive(7))))
		assertEquals(setOf("pid", "signal"), noStart.keys)
		assertTrue(Params.complete(ui.action("process.signal")!!, noStart))
		// A link's id, a string, for the unit actions.
		val restart = ui.dashboard("home")!!.widgets.first { it.id == "home/links" }.rowActions!!.single()
		val web = data.widgets.getValue("home/links").rows!!.single()
		assertEquals(JsonObject(mapOf("link" to JsonPrimitive("web"))), Params.fill(restart.params, web.f))
		// Only a whole "{field}" is filled; anything else goes as it is.
		val mixed = JsonObject(mapOf("a" to JsonPrimitive("x {pid}"), "b" to JsonPrimitive(true), "c" to JsonPrimitive("{nothing}")))
		assertEquals(JsonObject(mapOf("a" to JsonPrimitive("x {pid}"), "b" to JsonPrimitive(true))), Params.fill(mixed, proc.f))
	}

	@Test
	fun theFormAsksForWhatAWidgetLeavesOpen() {
		val backup = ui.action("backup")!!
		// The button fixes verify: nothing to ask.
		assertEquals(emptyList(), Params.asked(backup, JsonObject(mapOf("verify" to JsonPrimitive(true)))).map { it.id })
		assertEquals(listOf("verify"), Params.asked(backup, JsonObject(emptyMap())).map { it.id })
		// Optional ones aren't asked: start, the volume's level and muted.
		assertEquals(listOf("pid", "signal"), Params.asked(ui.action("process.signal")!!, JsonObject(emptyMap())).map { it.id })
		assertEquals(emptyList(), Params.asked(ui.action("volume.set")!!, JsonObject(emptyMap())))
		// The form starts from the default, else the first choice, the range's start, or off.
		val mode = ui.action("mode")!!.params.single()
		assertEquals(JsonPrimitive("desktop"), Params.initial(mode))
		assertEquals(JsonPrimitive("power-saver"), Params.initial(ui.action("profile.set")!!.params.single()))
		assertEquals(JsonPrimitive(50), Params.initial(ui.action("fan")!!.params.single()))
		assertEquals(JsonPrimitive(0L), Params.initial(ui.action("volume.set")!!.params.first { it.id == "level" }.copy(default = null)))
		assertEquals(JsonPrimitive(false), Params.initial(Param("x", "bool")))
		assertNull(Params.initial(Param("x", "string")))
	}

	@Test
	fun valuesAreCheckedAsTheServiceChecksThem() {
		val speed = ui.action("fan")!!.params.single()
		assertTrue(Params.valid(speed, JsonPrimitive(55)))
		assertTrue(Params.valid(speed, JsonPrimitive(0)))
		assertTrue(Params.valid(speed, JsonPrimitive(100)))
		assertFalse(Params.valid(speed, JsonPrimitive(52)), "off the step")
		assertFalse(Params.valid(speed, JsonPrimitive(105)), "past the range")
		assertFalse(Params.valid(speed, JsonPrimitive("55")), "a string")
		// A default covers a missing value; nothing covers a required one.
		assertTrue(Params.valid(speed, null))
		val pid = ui.action("process.signal")!!.params.first { it.id == "pid" }
		assertFalse(Params.valid(pid, null))
		assertTrue(Params.valid(pid, JsonPrimitive(4242)))
		assertFalse(Params.valid(pid, JsonPrimitive(42.5)))
		val signal = ui.action("process.signal")!!.params.first { it.id == "signal" }
		assertTrue(Params.valid(signal, JsonPrimitive("kill")))
		assertFalse(Params.valid(signal, JsonPrimitive("hup")))
		assertFalse(Params.valid(signal, JsonPrimitive(1)))
		val muted = ui.action("volume.set")!!.params.first { it.id == "muted" }
		assertTrue(Params.valid(muted, JsonPrimitive(true)))
		assertFalse(Params.valid(muted, JsonPrimitive("true")))
		assertTrue(Params.valid(muted, null), "optional")
		val position = ui.action("media.seek")!!.params.first { it.id == "position" }
		assertTrue(Params.valid(position, JsonPrimitive(12.3)))
		val player = ui.action("media.seek")!!.params.first { it.id == "player" }
		assertTrue(Params.valid(player, JsonPrimitive("org.mpris.MediaPlayer2.some")))
		assertFalse(Params.valid(player, JsonPrimitive(3)))
		// A parameter the action hasn't, or one missing, makes the whole incomplete.
		val mode = ui.action("mode")!!
		assertTrue(Params.complete(mode, JsonObject(mapOf("mode" to JsonPrimitive("server")))))
		assertTrue(Params.complete(mode, JsonObject(emptyMap())), "mode has a default")
		assertFalse(Params.complete(mode, JsonObject(mapOf("mode" to JsonPrimitive("server"), "extra" to JsonPrimitive(1)))))
		assertFalse(Params.complete(ui.action("process.signal")!!, JsonObject(mapOf("signal" to JsonPrimitive("term")))))
	}

	@Test
	fun slidersSnapToTheStep() {
		val speed = ui.action("fan")!!.params.single()
		assertEquals(55.0, Params.snap(53.0, speed))
		assertEquals(50.0, Params.snap(52.4, speed))
		assertEquals(100.0, Params.snap(140.0, speed))
		assertEquals(0.0, Params.snap(-3.0, speed))
		// Whole steps from a whole start: sent as an integer.
		assertEquals(JsonPrimitive(55L), Params.number(55.0, speed))
		assertEquals(JsonPrimitive(2.5), Params.number(2.5, Param("x", "range", min = 0.0, max = 5.0, step = 0.5)))
	}
}
