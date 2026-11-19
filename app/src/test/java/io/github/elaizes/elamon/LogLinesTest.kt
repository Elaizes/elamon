package io.github.elaizes.elamon

import java.time.ZoneOffset
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.LogLine
import io.github.elaizes.elamon.data.LogPage
import io.github.elaizes.elamon.data.WidgetState

/** A log's lines as the stream and `/log` give them: replaced, added to, loaded before, and kept within bounds. */
class LogLinesTest {
	/** A line at [c] µs. */
	private fun line(c: Long, level: String = "info") = LogLine(c = c, t = c / 1000, level = level, text = "line $c")

	private fun lines(range: LongRange) = range.map { line(it) }

	private fun state(range: LongRange, reset: Boolean = false, more: Int = 0, note: String? = null) =
		WidgetState(lines = lines(range), reset = reset, more = more, note = note)

	private fun LogLines.cs(): List<Long?> = items.map { (it as? LogItem.Line)?.line?.c }

	@Test
	fun aResetReplacesTheRestAddsAfterAndCountsWhatWasLeftOut() {
		val first = LogLines().take(state(1L..3L, reset = true), keep = 10)
		assertEquals(listOf<Long?>(1, 2, 3), first.cs())
		// Lines since, then a gap: "… 12 more" before the lines that came after it.
		val next = first.take(state(4L..5L), keep = 10).take(state(20L..21L, more = 12), keep = 10)
		assertEquals(listOf<Long?>(1, 2, 3, 4, 5, null, 20, 21), next.cs())
		assertEquals(12, (next.items[5] as LogItem.More).count)
		assertEquals(first.generation, next.generation)
		// Nothing new: nothing changes but the keys to come.
		assertEquals(next.items, next.take(WidgetState(lines = emptyList()), keep = 10).items)
		// The filter changed: what there was goes.
		val again = next.take(state(30L..31L, reset = true), keep = 10)
		assertEquals(listOf<Long?>(30, 31), again.cs())
		assertNotEquals(next.generation, again.generation)
		// Every item has a key of its own, across resets too.
		val keys = (next.items + again.items).map { it.key }
		assertEquals(keys.size, keys.toSet().size)
	}

	@Test
	fun aLogShownAnewIsNotTheOneBefore() {
		// Left and come back to, a log starts from nothing: a page asked for before is late all the same.
		val before = LogLines().take(state(1L..1L, reset = true), keep = 5)
		val anew = LogLines().take(state(1L..1L, reset = true), keep = 5)
		assertNotEquals(before.generation, anew.generation)
	}

	@Test
	fun theNewestAreKeptAsManyAsTheWidgetSays() {
		val l = LogLines().take(state(1L..8L, reset = true), keep = 10).take(state(9L..15L), keep = 10)
		assertEquals((6L..15L).toList(), l.cs())
		// The next older page starts at the oldest kept.
		assertNull(l.cursor)
		assertEquals(6L, l.before)
		// "… n more" with no line above it any more goes with them.
		val gap = LogLines().take(state(1L..2L, reset = true), keep = 3).take(state(10L..11L, more = 7), keep = 3)
		assertEquals(listOf<Long?>(2, null, 10, 11), gap.cs())
		val further = gap.take(state(12L..12L), keep = 3)
		assertEquals(listOf<Long?>(10, 11, 12), further.cs())
	}

	@Test
	fun whatIsLeftOutTimeAfterTimeIsOneRow() {
		// Errors shown, a flood of info: each state leaves lines out and has none to show.
		val flood = WidgetState(lines = emptyList(), more = 5)
		var l = LogLines().take(state(1L..2L, reset = true), keep = 10)
		repeat(1000) { l = l.take(flood, keep = 10) }
		assertEquals(listOf<Long?>(1, 2, null), l.cs())
		assertEquals(5000, (l.items.last() as LogItem.More).count)
		// A gap before a line adds to it too; the next, after the line, is a row of its own.
		l = l.take(state(3L..3L, more = 1), keep = 10).take(state(4L..4L, more = 2), keep = 10)
		assertEquals(listOf<Long?>(1, 2, null, 3, null, 4), l.cs())
		assertEquals(listOf(5001, 2), l.items.filterIsInstance<LogItem.More>().map { it.count })
		// With no line at all, still one.
		var none = LogLines().take(WidgetState(lines = emptyList(), reset = true), keep = 10)
		repeat(1000) { none = none.take(flood, keep = 10) }
		assertEquals(listOf<Long?>(null), none.cs())
	}

	@Test
	fun olderPagesGoBeforeAndMakeRoomForThemselves() {
		val live = LogLines().take(state(100L..104L, reset = true), keep = 5)
		assertTrue(live.canLoadOlder)
		assertEquals(100L, live.before)
		val asked = live.asking()
		assertTrue(asked.loadingOlder)
		val paged = asked.older(LogPage("w", lines(90L..92L), before = 80))
		assertEquals(listOf<Long?>(90, 91, 92, 100, 101, 102, 103, 104), paged.cs())
		assertFalse(paged.loadingOlder)
		assertEquals(3, paged.loaded)
		// The next page from where this one stopped looking, not from its oldest line.
		assertEquals(80L, paged.before)
		// New lines push the oldest out only past what the widget keeps and the pages brought.
		val more = paged.take(state(105L..106L), keep = 5)
		assertEquals(listOf<Long?>(92, 100, 101, 102, 103, 104, 105, 106), more.cs())
		// Lines went from the top: the next page starts at the oldest left.
		assertEquals(92L, more.before)
	}

	@Test
	fun noLinesIsSaidOnlyOnceNoneAreOnTheirWay() {
		// A108: an empty list of events showed its spinner and "No lines yet" together.
		val reset = LogLines().take(WidgetState(lines = emptyList(), reset = true), keep = 5)
		assertTrue(reset.saysEmpty)
		val asking = reset.asking()
		assertFalse(asking.saysEmpty)
		// None older either: now it says so; or the page failed, and it says that too.
		assertTrue(asking.older(LogPage("w", emptyList(), before = null)).saysEmpty)
		assertTrue(asking.failed().saysEmpty)
		assertFalse(reset.take(state(1L..1L), keep = 5).saysEmpty)
	}

	@Test
	fun theStartOfTheJournalHidesTheButton() {
		val end = LogLines().take(state(10L..11L, reset = true), keep = 5).older(LogPage("w", lines(1L..2L), before = null))
		assertTrue(end.atStart)
		assertFalse(end.canLoadOlder)
		// An empty page still has a `before`: there may be more before it.
		val empty = LogLines().take(state(10L..11L, reset = true), keep = 5).older(LogPage("w", emptyList(), before = 3))
		assertTrue(empty.canLoadOlder)
		assertEquals(3L, empty.before)
		// A service without `/log`: no button.
		assertFalse(LogLines().take(state(1L..1L, reset = true), keep = 5).asking().noOlder().canLoadOlder)
		// One that failed: the button again, saying so.
		val failed = LogLines().take(state(1L..1L, reset = true), keep = 5).asking().failed()
		assertTrue(failed.olderFailed && failed.canLoadOlder && !failed.loadingOlder)
		// A reset starts over: older lines can be loaded again.
		assertTrue(end.take(state(20L..20L, reset = true), keep = 5).canLoadOlder)
	}

	@Test
	fun aPageHasNothingTwiceAndFillsWhatWasLeftOut() {
		// Asked with none held (the newest): a line the stream sent meanwhile isn't added again.
		val l = LogLines().take(WidgetState(lines = emptyList(), reset = true), keep = 5)
		assertNull(l.before)
		val raced = l.take(state(50L..50L), keep = 5).older(LogPage("w", lines(48L..50L), before = 40))
		assertEquals(listOf<Long?>(48, 49, 50), raced.cs())
		// "… n more" over the first line stood for what the page brings.
		val gap = LogLines().take(state(10L..11L, more = 4), keep = 5)
		assertEquals(listOf<Long?>(null, 10, 11), gap.cs())
		assertEquals(listOf<Long?>(6, 7, 8, 9, 10, 11), gap.older(LogPage("w", lines(6L..9L), before = 5)).cs())
	}

	@Test
	fun noMoreThanTheMostInAll() {
		val max = Limits.LOG_LINES_MAX.toLong()
		var l = LogLines().take(state(max * 10..max * 10 + 999, reset = true), keep = 1000)
		var c = max * 10
		while (l.canLoadOlder) {
			l = l.older(LogPage("w", lines(c - 300 until c), before = c - 300))
			c -= 300
		}
		assertEquals(Limits.LOG_LINES_MAX, l.lineCount)
		assertFalse(l.canLoadOlder)
		// Full, a page's newest lines are kept, next to those held.
		assertEquals(l.cs().filterNotNull(), l.cs().filterNotNull().sorted())
		// New lines keep it full, the oldest going.
		val on = l.take(state(max * 20..max * 20 + 9), keep = 1000)
		assertEquals(Limits.LOG_LINES_MAX, on.lineCount)
		assertEquals(max * 20 + 9, (on.items.last() as LogItem.Line).line.c)
	}

	@Test
	fun theNoteStaysUntilAResetSaysOtherwise() {
		val l = LogLines().take(state(1L..1L, reset = true, note = "Only this user's journal."), keep = 5)
		assertEquals("Only this user's journal.", l.note)
		assertEquals("Only this user's journal.", l.take(state(2L..2L), keep = 5).note)
		assertEquals("journalctl ended", l.take(state(2L..2L, note = "journalctl ended"), keep = 5).note)
		assertNull(l.take(state(3L..3L, reset = true), keep = 5).note)
	}

	@Test
	fun levelsLookAsTheyWeigh() {
		assertEquals(LogTone.Error, logTone("critical"))
		assertEquals(LogTone.Error, logTone("error"))
		assertEquals(LogTone.Warning, logTone("warning"))
		assertEquals(LogTone.Notice, logTone("notice"))
		assertEquals(LogTone.Normal, logTone("info"))
		assertEquals(LogTone.Dimmed, logTone("debug"))
		// A level from a newer service, or none, is plain.
		assertEquals(LogTone.Normal, logTone("emergency"))
		assertEquals(LogTone.Normal, logTone(null))
		// The chips: five, and critical only for a log that starts there.
		assertEquals(listOf("debug", "info", "notice", "warning", "error"), logLevels("warning"))
		assertEquals(listOf("debug", "info", "notice", "warning", "error", "critical"), logLevels("critical"))
	}

	@Test
	fun aLineIsCopiedWithItsTimeLevelAndSource() {
		val utc = ZoneOffset.UTC
		// 2023-11-14 22:13:15.234 UTC.
		val l = LogLine(c = 1_699_999_995_234_567, t = 1_699_999_995_234, level = "warning", unit = "example.service", id = "example", text = "Slow request")
		assertEquals("22:13:15", logClock(l.t, utc))
		assertEquals("22:13:15 warning example: Slow request", logCopy(l, utc))
		assertEquals("22:13:15 warning example.service: Slow request", logCopy(l.copy(id = null), utc))
		assertEquals("22:13:15 warning: Slow request", logCopy(l.copy(unit = null, id = null), utc))
		// The phone's own clock, 24 hours.
		assertEquals("01:13:15", logClock(l.t, ZoneOffset.ofHours(3)))
	}
}
