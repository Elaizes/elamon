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
		// The gauge is a ring, as full as its value is between its min and max; nothing else is.
		assertEquals(listOf(0.8f, null, null), lines.map { it.ring })
	}

	@Test
	fun aGaugesRingByItsRangeAndItsColour() {
		val doc = ui.copy(
			dashboards = ui.dashboards + io.github.elaizes.elamon.data.Dashboard(
				"w",
				widgets = listOf(
					Widget("w/cpu", "gauge", label = "CPU", format = "percent", color = "cpu"),
					Widget("w/temp", "gauge", label = "Temp", format = "temp", min = 20.0, max = 100.0),
					Widget("w/load", "gauge", label = "Load"),
					Widget("w/disk", "meter", label = "Disk", format = "percent", color = "memory"),
				),
			),
		)
		val states = mapOf(
			"w/cpu" to WidgetState(v = JsonPrimitive(150)),
			"w/temp" to WidgetState(v = JsonPrimitive(60)),
			"w/load" to WidgetState(v = JsonPrimitive(3), max = 12.0),
			"w/disk" to WidgetState(v = JsonPrimitive(40)),
		)
		val lines = widgetLines(doc, "w", states)
		// A percent is of 100 and never past it; a range of the widget's own; a max the state gives; a meter stays a line.
		assertEquals(listOf(1f, 0.5f, 0.25f, null), lines.map { it.ring })
		assertEquals(listOf("cpu", null, null, null), lines.map { it.color })
		// Not read yet: an empty ring and a dash.
		assertEquals(0f to "—", widgetLines(doc, "w", emptyMap()).first().let { it.ring to it.text })
	}

	@Test
	fun gaugesAsRingsWhereTheyFit() {
		fun gauge(n: Int) = WidgetLine("G$n", "$n%", ring = n / 10f)
		fun line(n: Int) = WidgetLine("L$n", "$n")
		fun WidgetLayout.labels() = rings.map { r -> r.map { it.label } }
		val four = (1..4).map(::gauge)
		// A square widget: two rows of two, as large as its height leaves them, and nothing under them.
		with(widgetLayout(four, 170f, 170f, 1f)) {
			assertEquals(listOf(listOf("G1", "G2"), listOf("G3", "G4")), labels())
			assertEquals(54f, ringDp)
			assertEquals(emptyList(), lines)
		}
		// A wide one: all four in a row and the rest's lines under them, the gauges in their order, the lines in theirs.
		with(widgetLayout(listOf(line(1), gauge(1), gauge(2), line(2), gauge(3), gauge(4), line(3)), 340f, 180f, 1f)) {
			assertEquals(listOf(listOf("G1", "G2", "G3", "G4")), labels())
			assertEquals(73f, ringDp)
			assertEquals(listOf("L1", "L2"), lines.map { it.label })
		}
		// No larger than the most, however much room there is.
		assertEquals(RING_MAX_DP, widgetLayout(listOf(gauge(1)), 400f, 400f, 1f).ringDp)
		// Three across would leave one alone: two rows of two instead.
		assertEquals(listOf(2, 2), widgetLayout(four, 220f, 200f, 1f).rings.map { it.size })
		// A row's height only: the row that fits; the other gauges are left out.
		with(widgetLayout(four, 170f, 110f, 1f)) {
			assertEquals(listOf(listOf("G1", "G2")), labels())
			assertEquals(RING_MIN_DP + 8f, ringDp)
		}
		// Too low for even one ring: the gauges are lines like the rest.
		with(widgetLayout(four + line(5), 340f, 90f, 1f)) {
			assertEquals(emptyList(), rings)
			assertEquals(listOf("G1"), lines.map { it.label })
		}
		// No gauges: lines, as before.
		assertEquals(linesThatFit(180f, 1f), widgetLayout((1..9).map(::line), 170f, 180f, 1f).lines.size)
		// Larger at the app's scale: fewer across, still as even as they go.
		assertEquals(listOf(4), widgetLayout(four, 264f, 260f, 1f).rings.map { it.size })
		assertEquals(listOf(2, 2), widgetLayout(four, 264f, 260f, 1.4f).rings.map { it.size })
	}

	@Test
	fun ringsInRowsAsEvenAsTheyGo() {
		fun sizes(n: Int, width: Float, height: Float) = widgetLayout((1..n).map { WidgetLine("G$it", "$it%", ring = 0.5f) }, width, height, 1f).rings.map { it.size }
		assertEquals(listOf(3, 2), sizes(5, 200f, 200f))
		assertEquals(listOf(3, 2, 2), sizes(7, 200f, 260f))
		assertEquals(listOf(5), sizes(5, 340f, 110f))
	}

	@Test
	fun aRingsTextStaysBetweenItsSides() {
		// A short one has its share of the ring; a long one is smaller, but never too small to read.
		assertEquals(84 * 0.24f, ringText("7%", 84f, 0.24f))
		assertTrue(ringText("11.7 GiB", 48f, 0.24f) < 48 * 0.24f)
		assertEquals(7f, ringText("A label too long for any ring", 48f, 0.16f))
		for (text in listOf("100%", "Memory", "Temp", "Battery")) assertTrue(ringText(text, 84f, 0.24f) * 0.6f * text.length <= 84 * 0.7f + 0.01f, text)
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
