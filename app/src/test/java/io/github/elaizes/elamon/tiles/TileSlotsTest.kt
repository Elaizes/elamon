package io.github.elaizes.elamon.tiles

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import io.github.elaizes.elamon.Message
import io.github.elaizes.elamon.data.Dashboard
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.ui.tapOf

/** The Quick Settings tiles: what each of the four runs, from the profile's `tiles` dashboard (fixtures/0.10). */
class TileSlotsTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "0.10/ui-displays.json").readText())

	@Test
	fun theFirstFourButtonsAndTogglesInOrder() {
		val slots = tileSlots(ui)
		assertEquals(listOf("tiles/server", "tiles/desktop", "tiles/saver", "tiles/panel"), slots.map { it.widget.id })
		// Named and drawn as the widget, else as its action.
		assertEquals(listOf("Server", "Desktop", "Power saver", "Panel"), slots.map { it.label })
		assertEquals(listOf("leaf", "laptop", "bolt", "monitor"), slots.map { it.icon })
		assertEquals(listOf(false, false, true, true), slots.map { it.toggle })
	}

	@Test
	fun noDashboardNoActions() {
		assertEquals(emptyList(), tileSlots(ui.copy(profile = ui.profile.copy(android = ui.profile.android.copy(tiles = "")))))
		// Named, but not one this phone's role sees.
		assertEquals(emptyList(), tileSlots(ui.copy(dashboards = ui.dashboards.filter { it.id != "tiles" })))
	}

	@Test
	fun aPageToOpenOrAnActionNotOfferedIsNoTile() {
		val more = listOf(
			Widget("t/open", "button", label = "More", open = "control"),
			Widget("t/gone", "button", label = "Gone", action = "nothing.here"),
			Widget("t/g", "group", widgets = listOf(Widget("t/lock", "button", label = "Lock", action = "power.lock"))),
		) + (1..4).map { Widget("t/$it", "button", label = "Extra $it", action = "power.suspend") }
		val doc = ui.copy(dashboards = ui.dashboards.map { if (it.id == "tiles") Dashboard("tiles", widgets = more) else it })
		// Groups opened out in place; four at most.
		assertEquals(listOf("t/lock", "t/1", "t/2", "t/3"), tileSlots(doc).map { it.widget.id })
	}

	@Test
	fun aReadingFromBeforeAnActionDoesntTurnItsToggleBack() {
		val a = "a".repeat(32)
		val key = tileKey(a, "tiles/panel")
		// One computer's toggle is never another's, the same dashboard as it may be.
		assertNotEquals(key, tileKey("b".repeat(32), "tiles/panel"))
		assertTrue(TileMemory.readable(key, 100))
		// While its action runs, and from a reading begun before it ended, its state isn't taken.
		TileMemory.running += key
		assertFalse(TileMemory.readable(key, 100))
		TileMemory.running -= key
		TileMemory.notes[key] = TileNote(Message.Res(0), at = 200, failed = false)
		assertFalse(TileMemory.readable(key, 150))
		assertTrue(TileMemory.readable(key, 250))
		TileMemory.notes.remove(key)
	}

	@Test
	fun whatAsksFirstOpensTheAppsAskingTheRestRunsAtOnce() {
		val (server, _, saver, panel) = tileSlots(ui)
		// The mode is confirmed and unlocked for, as the computer says.
		assertTrue(asksFirst(server, tapOf(server.widget, server.action, on = false).params))
		// The power profile asks nothing: it runs from the tile.
		assertFalse(asksFirst(saver, tapOf(saver.widget, saver.action, on = false).params))
		// Turning a monitor off or on is confirmed.
		assertTrue(asksFirst(panel, tapOf(panel.widget, panel.action, on = true).params))
		// Values the widget leaves open are asked for.
		val open = server.copy(widget = server.widget.copy(params = null))
		assertTrue(asksFirst(open.copy(action = open.action.copy(confirm = false, unlock = false)), JsonObject(emptyMap())))
		assertFalse(asksFirst(open.copy(action = open.action.copy(confirm = false, unlock = false)), JsonObject(mapOf("mode" to JsonPrimitive("server")))))
	}
}
