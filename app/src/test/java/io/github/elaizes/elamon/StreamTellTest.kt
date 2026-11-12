package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import io.github.elaizes.elamon.data.Dashboard
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.ServiceInfo
import io.github.elaizes.elamon.data.Sort
import io.github.elaizes.elamon.data.Subscribe
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget

/** What an open stream is told, or whether it is opened anew, for the screen to get what it asks for. */
class StreamTellTest {
	/** Processes, largest first by CPU and also by three others; sensors by name, smallest first, or by value. */
	private val processes = Widget("p/table", "table", sort = Sort("cpu", desc = true), sorts = listOf("cpu", "mem", "io"))
	private val sensors = Widget("s/table", "table", sort = Sort("label"), sorts = listOf("value"))
	/** By name, smallest first, which is also one of the orders it takes (largest first, then). */
	private val links = Widget("l/table", "table", sort = Sort("name"), sorts = listOf("name", "port"))
	private val ui = UiDoc(
		version = "1",
		service = ServiceInfo(id = "0123456789abcdef0123456789abcdef"),
		tabs = listOf("p", "s"),
		dashboards = listOf(Dashboard("p", widgets = listOf(processes)), Dashboard("s", widgets = listOf(Widget("g", "group", widgets = listOf(sensors, links))))),
	)
	private val open = Limits.streamReadTimeoutMs(1000)

	@Test
	fun nothingWhileItSendsWhatIsWanted() {
		val w = Want("p", 1000, mapOf("p/table" to "mem"))
		assertEquals(Tell.Nothing, tell(w, w, ui, open))
	}

	@Test
	fun aNewStreamIsToldTheOrdersWanted() {
		// It opens with the dashboard and the interval, its tables in their own order: the one wanted
		// in another is told once it is open, not taken for told already.
		val wanted = Want("p", 1000, mapOf("p/table" to "mem"))
		val sent = wanted.copy(sorts = emptyMap())
		assertEquals(Tell.Change(Subscribe(sort = mapOf("p/table" to "mem"))), tell(sent, wanted, ui, open))
	}

	@Test
	fun theDashboardAndTheInterval() {
		val sent = Want("p", 1000)
		assertEquals(Tell.Change(Subscribe(dashboard = "s")), tell(sent, sent.copy(dashboard = "s"), ui, open))
		// Lines every 2 s: the connection still waits long enough for them.
		assertEquals(Tell.Change(Subscribe(dashboard = "s", interval = 2000)), tell(sent, Want("s", 2000), ui, open))
	}

	@Test
	fun linesRarerThanTheConnectionWaitsForOpenAnew() {
		// Opened at 1 s, it gives up after 15 s of silence: lines every 30 s need a stream that waits for them.
		val sent = Want("p", 1000)
		assertEquals(Tell.Reopen, tell(sent, sent.copy(intervalMs = 30_000), ui, open))
		// Opened for them, it is told nothing more.
		val slow = Want("p", 30_000)
		assertEquals(Tell.Nothing, tell(slow, slow, ui, Limits.streamReadTimeoutMs(30_000)))
		// And one opened for 30 s that is to send every second waits for that again.
		assertEquals(Tell.Reopen, tell(slow, slow.copy(intervalMs = 1000), ui, Limits.streamReadTimeoutMs(30_000)))
	}

	@Test
	fun ownOrderBackByName() {
		// Largest first by CPU is what asking for CPU gives.
		val sent = Want("p", 1000, mapOf("p/table" to "mem"))
		val back = sent.sorted(processes, "cpu")
		assertEquals(emptyMap(), back.sorts)
		assertEquals(Tell.Change(Subscribe(sort = mapOf("p/table" to "cpu"))), tell(sent, back, ui, open))
		// By name, which isn't among the others: asked for by name, the table keeps its own order, smallest first.
		val byValue = Want("s", 1000, mapOf("s/table" to "value"))
		assertEquals(Tell.Change(Subscribe(sort = mapOf("s/table" to "label"))), tell(byValue, byValue.sorted(sensors, "label"), ui, open))
	}

	@Test
	fun ownOrderSmallestFirstAmongTheOthersOpensAnew() {
		// Asked for by name it would come largest first: only a new stream has the table's own order.
		val sent = Want("s", 1000, mapOf("l/table" to "port"))
		assertEquals(Tell.Reopen, tell(sent, sent.sorted(links, "name"), ui, open))
	}

	@Test
	fun anOrderTheComputerNoLongerKeepsToIsLeft() {
		val sent = Want("s", 1000, mapOf("gone/table" to "mem", "s/table" to "rpm"))
		// The table is gone from the ui, or no longer lists the order: the service doesn't keep to either.
		assertEquals(Tell.Nothing, tell(sent, sent.copy(sorts = emptyMap()), ui, open))
	}

	@Test
	fun sortedKeepsTheOthers() {
		val w = Want("p", 1000).sorted(processes, "io").sorted(links, "port")
		assertEquals(mapOf("p/table" to "io", "l/table" to "port"), w.sorts)
		assertEquals(mapOf("l/table" to "port"), w.sorted(processes, "cpu").sorts)
		assertEquals(w.sorted(processes, "mem").sorts["p/table"], "mem")
	}

	@Test
	fun widgetsAreFoundInGroups() {
		assertEquals(links, findWidget(ui, "l/table"))
		assertEquals(null, findWidget(ui, "nothing"))
	}
}
