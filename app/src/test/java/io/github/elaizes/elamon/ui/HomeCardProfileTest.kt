package io.github.elaizes.elamon.ui

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.HistoryEntry
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.WidgetState

/** A computer's card when its profile names one (`[profiles.<id>.android] card`, elamond 0.10). */
class HomeCardProfileTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "0.10/ui-displays.json").readText())

	/** A reading of the fixture's card: the CPU with its spark's points, the memory, the lock button. */
	private val card = DataLine(
		t = 1_700_000_000_000,
		dashboard = "card",
		widgets = mapOf(
			"card/cpu" to WidgetState(v = JsonPrimitive(12.5), level = "normal"),
			"card/memory" to WidgetState(v = JsonPrimitive(8_000_000_000.0), max = 16_000_000_000.0),
			"card/lock" to WidgetState(enabled = true),
		),
		history = mapOf("card/cpu" to HistoryEntry(listOf(1_699_999_999_000, 1_700_000_000_000), listOf(listOf(10.0, 12.5)))),
	)

	@Test
	fun theProfilesCardHasItsFiguresAndQuickActions() = runTest {
		val read = mutableListOf<String>()
		val live = assertIs<Live.Online>(cardReading(ui = { ui }, snapshot = { read += it; card }))
		assertEquals(listOf("card"), read)
		assertEquals(listOf("CPU", "Memory"), live.stats.map { it.label })
		assertEquals(listOf("13%", "7.45 GiB"), live.stats.map { it.text })
		// The value with `spark` brings its points; the meter its bar.
		assertEquals(listOf(10.0, 12.5), live.stats[0].spark!!.v.single())
		assertNull(live.stats[1].spark)
		assertEquals(0.5f, live.stats[1].fraction)
		val quick = assertNotNull(live.quick)
		assertEquals(ui, quick.ui)
		assertEquals(listOf("power.lock"), quick.items.map { it.action.id })
		assertEquals(true, quick.items.single().enabled)
	}

	@Test
	fun withoutACardTheFirstTabAsBefore() = runTest {
		// A profile without `card`, or one whose card this phone's role sees none of: the first tab's figures, no quick actions.
		for (doc in listOf(ui.copy(profile = ui.profile.copy(android = ui.profile.android.copy(card = ""))), ui.copy(dashboards = ui.dashboards.filter { it.id != "card" }))) {
			val read = mutableListOf<String>()
			val live = assertIs<Live.Online>(cardReading(ui = { doc }, snapshot = { read += it; card.copy(dashboard = it, widgets = emptyMap()) }))
			assertEquals(listOf("control"), read)
			assertNull(live.quick)
		}
	}

	@Test
	fun aCardsTapRunsAsItsWidgetWould() {
		val tiles = ui.dashboard("tiles")!!.widgets
		val saver = tiles.first { it.id == "tiles/saver" }
		val profile = ui.action("profile.set")!!
		// A toggle that is off turns on, and the other way.
		assertEquals(Tap(JsonObject(mapOf("profile" to JsonPrimitive("power-saver"))), "Power saver"), tapOf(saver, profile, on = false))
		assertEquals(Tap(JsonObject(mapOf("profile" to JsonPrimitive("balanced"))), "Power saver: Balanced"), tapOf(saver, profile, on = true))
		// A button, its fixed values.
		val server = tiles.first { it.id == "tiles/server" }
		assertEquals(Tap(JsonObject(mapOf("mode" to JsonPrimitive("server"))), "Server"), tapOf(server, ui.action("mode")!!, on = false))
		// The panel's toggle reads its state as its switch does.
		val panel = tiles.first { it.id == "tiles/panel" }
		assertEquals(true, toggledOn(panel, WidgetState(v = JsonPrimitive("on"))))
		assertEquals(false, toggledOn(panel, WidgetState(v = JsonPrimitive("off"))))
		assertEquals(false, toggledOn(panel, null))
		assertEquals(Tap(JsonObject(mapOf("state" to JsonPrimitive("off"))), "Panel: Off"), tapOf(panel, ui.action("display.output.eDP-1")!!, on = true))
	}
}
