package io.github.elaizes.elamon.widget

import java.io.File
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonPrimitive
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.ItemState
import io.github.elaizes.elamon.data.ListItem
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.readDashboard

/** The home-screen widget: what it shows of the profile's `widget` dashboard (fixtures/0.10), and when it is read. */
class WidgetTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "0.10/ui-displays.json").readText())
	private val snapshot = ProtocolJson.decodeFromString(DataLine.serializer(), File(fixtures, "0.10/snapshot-widget.json").readText())

	@Test
	fun theProfilesDashboardAsLines() {
		assertEquals("phone", widgetDashboard(ui))
		val lines = widgetLines(ui, "phone", snapshot.widgets)
		assertEquals(listOf("Battery", "CPU", "Monitors"), lines.map { it.label })
		// Formatted as the app formats them, with the level the service gave.
		assertEquals(listOf("80%", "56°C", "2 connected · 1 on"), lines.map { it.text })
		assertEquals(listOf("normal", "warning", "normal"), lines.map { it.level })
		assertEquals(0.122, lines[1].blend)
		// A temperature is green while normal, as its tile; the text's words go across the widget.
		assertEquals(listOf(false, true, false), lines.map { it.green })
		assertEquals(listOf(false, false, true), lines.map { it.words })
	}

	@Test
	fun groupsOpenOutListsGiveTheirItemsAndTheRestIsLeftOut() {
		val doc = ui.copy(
			dashboards = ui.dashboards + io.github.elaizes.elamon.data.Dashboard(
				"w",
				widgets = listOf(
					Widget("w/g", "group", widgets = listOf(Widget("w/s", "status", label = "UPS"), Widget("w/graph", "graph", label = "Load"))),
					Widget("w/l", "list", items = listOf(ListItem("Kernel"), ListItem("Uptime", format = "duration"), ListItem("None"))),
					Widget("w/t", "text", label = "Note"),
					Widget("w/b", "button", label = "Lock", action = "power.lock"),
				),
			),
		)
		val states = mapOf(
			"w/s" to WidgetState(v = JsonPrimitive("OL"), text = "On line", level = "normal"),
			"w/l" to WidgetState(items = listOf(ItemState(JsonPrimitive("6.18")), ItemState(JsonPrimitive(3600)), ItemState())),
		)
		val lines = widgetLines(doc, "w", states)
		// The status (green while normal), the list's items that have a value; no graph, no text without words, no button.
		assertEquals(listOf("UPS" to "On line", "Kernel" to "6.18", "Uptime" to "1 h 0 min"), lines.map { it.label to it.text })
		assertTrue(lines[0].green)
		// A figure not read yet is a dash, as on its tile.
		assertEquals("—", widgetLines(ui, "phone", emptyMap()).first().text)
		assertEquals(emptyList(), widgetLines(ui, "gone", snapshot.widgets))
	}

	@Test
	fun noWidgetNamedOrNoneSeen() {
		assertNull(widgetDashboard(ui.copy(profile = ui.profile.copy(android = ui.profile.android.copy(widget = "")))))
		assertNull(widgetDashboard(ui.copy(dashboards = ui.dashboards.filter { it.id != "phone" })))
	}

	@Test
	fun aKeptUiThatNamesNoWidgetIsReadAgain() = runTest {
		// The profile names one since the ui was kept: the computer's own is asked for, once.
		val kept = ui.copy(profile = ui.profile.copy(android = ui.profile.android.copy(widget = "")))
		val asked = mutableListOf<Boolean>()
		val r = readDashboard(ui = { fresh -> asked += fresh; if (fresh) ui else kept }, pick = ::widgetDashboard, snapshot = { snapshot }, freshWithout = true)
		assertEquals(listOf(false, true), asked)
		assertEquals("phone", r.line?.dashboard)
		// Still none: no reading, and no third ask.
		asked.clear()
		val none = readDashboard(ui = { fresh -> asked += fresh; kept }, pick = ::widgetDashboard, snapshot = { snapshot }, freshWithout = true)
		assertNull(none.line)
		assertEquals(listOf(false, true), asked)
	}

	@Test
	fun theComputerAWidgetShows() {
		val a = Machine("a".repeat(32), "Desk", listOf("192.0.2.1"), 47823, "f")
		val b = Machine("b".repeat(32), "Laptop", listOf("192.0.2.2"), 47823, "f")
		assertEquals(b, shownMachine(b.id, listOf(a, b)))
		// None chosen (one computer isn't asked for), or one forgotten since: the first paired.
		assertEquals(a, shownMachine(null, listOf(a, b)))
		assertEquals(a, shownMachine("c".repeat(32), listOf(a, b)))
		assertNull(shownMachine(a.id, emptyList()))
	}

	@Test
	fun readAsOftenAsTheProfileAsksAndAndroidAllows() {
		assertEquals(30, widgetMinutes(30))
		assertEquals(MIN_MINUTES, widgetMinutes(5))
		assertEquals(MAX_MINUTES, widgetMinutes(100_000))
		val now = TimeUnit.HOURS.toMillis(100)
		assertTrue(WidgetWork.stale(WidgetShown(), now))
		assertFalse(WidgetWork.stale(WidgetShown(readAt = now - TimeUnit.MINUTES.toMillis(29), minutes = 30), now))
		assertTrue(WidgetWork.stale(WidgetShown(readAt = now - TimeUnit.MINUTES.toMillis(30), minutes = 30), now))
	}

	@Test
	fun whatTheWidgetSaysOfItsReading() {
		val read = WidgetShown(readAt = 1, fetch = Fetch.Ok)
		assertEquals(Note.At, widgetNote(read, reading = false))
		assertEquals(Note.Reading, widgetNote(read, reading = true))
		// Out of reach: the last values stay, said to be old; never made up.
		val offline = read.tried(Fetch.Offline)
		assertEquals(read.lines, offline.lines)
		assertEquals(read.readAt, offline.readAt)
		assertEquals(Note.Offline, widgetNote(offline, reading = false))
		assertEquals(Note.Unpaired, widgetNote(read.tried(Fetch.Unpaired), reading = false))
		assertEquals(Note.None, widgetNote(WidgetShown(fetch = Fetch.NoDashboard), reading = false))
	}

	@Test
	fun asManyLinesAsFit() {
		// 110 dp: the name, and two lines; larger at the app's scale, fewer; one at least.
		assertEquals(2, linesThatFit(110f, 1f))
		assertEquals(6, linesThatFit(180f, 1f))
		assertEquals(1, linesThatFit(110f, 1.4f))
		assertEquals(1, linesThatFit(40f, 1f))
	}
}
