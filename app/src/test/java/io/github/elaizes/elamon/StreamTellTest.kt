package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import io.github.elaizes.elamon.data.Dashboard
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.LogFilter
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
	/** The system's journal from warnings on, on a page of its own; a unit's, in a group of the sensors' tab. */
	private val journal = Widget("j/log", "log", level = "warning")
	private val unit = Widget("u/log", "log", level = "info")
	private val ui = UiDoc(
		version = "1",
		service = ServiceInfo(id = "0123456789abcdef0123456789abcdef"),
		tabs = listOf("p", "s"),
		dashboards = listOf(
			Dashboard("p", widgets = listOf(processes)),
			Dashboard("s", widgets = listOf(Widget("g", "group", widgets = listOf(sensors, links, unit)))),
			Dashboard("j", widgets = listOf(journal)),
		),
	)
	private val open = Limits.streamReadTimeoutMs(1000)

	@Test
	fun nothingWhileItSendsWhatIsWanted() {
		val w = Want("p", 1000, mapOf("p/table" to "mem"))
		assertEquals(Tell.Already, tell(w, w, ui, open))
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
		assertEquals(Tell.Already, tell(slow, slow, ui, Limits.streamReadTimeoutMs(30_000)))
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
		assertEquals(Tell.Already, tell(sent, sent.copy(sorts = emptyMap()), ui, open))
	}

	@Test
	fun sortedKeepsTheOthers() {
		val w = Want("p", 1000).sorted(processes, "io").sorted(links, "port")
		assertEquals(mapOf("p/table" to "io", "l/table" to "port"), w.sorts)
		assertEquals(mapOf("l/table" to "port"), w.sorted(processes, "cpu").sorts)
		assertEquals(w.sorted(processes, "mem").sorts["p/table"], "mem")
	}

	@Test
	fun aLogsFilterIsTakenWholeAndItsOwnIsNone() {
		val w = Want("j", 1000).filtered(journal, level = "error")
		assertEquals(mapOf("j/log" to LogFilter("error", "")), w.logs)
		// A search keeps the level, a level the search.
		val both = w.filtered(journal, search = "  ssh ").filtered(journal, level = "notice")
		assertEquals(LogFilter("notice", "ssh"), both.logs["j/log"])
		// No longer than the service takes.
		assertEquals(Limits.LOG_SEARCH_MAX, w.filtered(journal, search = "x".repeat(150)).logs.getValue("j/log").search.length)
		// Its own level and no search: its own filter, which isn't asked for.
		assertEquals(emptyMap(), both.filtered(journal, level = "warning", search = "").logs)
	}

	@Test
	fun aLogsFilterIsToldWithItsDashboardOnTheStream() {
		val onIt = Want("j", 1000)
		assertEquals(Tell.Change(Subscribe(log = mapOf("j/log" to LogFilter("error", "")))), tell(onIt, onIt.filtered(journal, level = "error"), ui, open))
		// Its own back: null.
		val filtered = onIt.filtered(journal, level = "error")
		assertEquals(Tell.Change(Subscribe(log = mapOf<String, LogFilter?>("j/log" to null))), tell(filtered, onIt, ui, open))
		// Wanted while the stream is on another dashboard, it waits: the service takes only the logs it sends.
		val elsewhere = Want("p", 1000)
		assertEquals(Tell.Already, tell(elsewhere, elsewhere.filtered(journal, level = "error"), ui, open))
		// Then it goes with its dashboard, and comes again with it after another, whatever was told before.
		val back = filtered.copy(dashboard = "j")
		assertEquals(Tell.Change(Subscribe(dashboard = "j", log = mapOf("j/log" to LogFilter("error", "")))), tell(elsewhere, back, ui, open))
		assertEquals(Tell.Change(Subscribe(dashboard = "j", log = mapOf("j/log" to LogFilter("error", "")))), tell(filtered.copy(dashboard = "p"), back, ui, open))
		// A log in a group is a log of its dashboard.
		val unitOn = Want("s", 1000)
		assertEquals(Tell.Change(Subscribe(log = mapOf("u/log" to LogFilter("info", "fail")))), tell(unitOn, unitOn.filtered(unit, search = "fail"), ui, open))
	}

	@Test
	fun whatTheServiceHasOnceTold() {
		val sent = Want("j", 1000, logs = mapOf("j/log" to LogFilter("error", ""), "u/log" to LogFilter("debug", "")))
		val want = Want("j", 1000)
		val change = (tell(sent, want, ui, open) as Tell.Change).change
		assertEquals(mapOf<String, LogFilter?>("j/log" to null), change.log)
		// The log on another dashboard keeps what it was told: the service keeps it by its id.
		assertEquals(mapOf("u/log" to LogFilter("debug", "")), want.told(change, sent.logs).logs)
		// A new stream has every log by its own filter: those wanted otherwise are told again.
		val wanted = Want("j", 1000).filtered(journal, level = "error")
		assertEquals(Tell.Change(Subscribe(log = mapOf("j/log" to LogFilter("error", "")))), tell(wanted.copy(logs = emptyMap()), wanted, ui, open))
	}

	@Test
	fun widgetsAreFoundInGroups() {
		assertEquals(links, findWidget(ui, "l/table"))
		assertEquals(null, findWidget(ui, "nothing"))
	}
}
