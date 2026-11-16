package io.github.elaizes.elamon

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.concurrent.atomic.AtomicInteger
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.LogLine
import io.github.elaizes.elamon.data.LogPage
import io.github.elaizes.elamon.data.WidgetState

/** What a log shows, oldest first: a line of the journal, or how many were left out where it stands. */
sealed interface LogItem {
	/** Tells the items of a log apart, for its list. */
	val key: Long

	data class Line(override val key: Long, val line: LogLine) : LogItem

	data class More(override val key: Long, val count: Int) : LogItem
}

/**
 * A log's lines as the app keeps them, oldest first: those the stream sent since its last reset,
 * after the pages of older ones loaded from `/log`. The newest [keep] live lines stay, and as many
 * more as the pages brought ([loaded]), [Limits.LOG_LINES_MAX] in all; past that, the oldest go.
 * Each change is a new one.
 */
data class LogLines(
	val items: List<LogItem> = emptyList(),
	/** The widget's `lines`: how many of the stream's it keeps. */
	val keep: Int = Limits.LOG_LINES_DEFAULT,
	/** Lines the pages brought since the last reset: room kept for them beside the live ones. */
	val loaded: Int = 0,
	/** Where the next older page starts, as the last one said; null for the oldest line held. */
	val cursor: Long? = null,
	/** The journal has nothing older than these, or the service no older lines to give. */
	val atStart: Boolean = false,
	/** Why there may be more lines than these, or none, as the service says. */
	val note: String? = null,
	/** Tells the resets apart, those of every log: a page asked for before the last one is late, even for a log shown anew since. */
	val generation: Int = 0,
	/** An older page is on its way; the last one asked for didn't come. */
	val loadingOlder: Boolean = false,
	val olderFailed: Boolean = false,
	private val nextKey: Long = 0,
) {
	val lineCount: Int get() = items.count { it is LogItem.Line }

	/** The oldest line held. */
	val oldest: LogLine? get() = (items.firstOrNull { it is LogItem.Line } as LogItem.Line?)?.line

	/** `before` for the next older page: where the last one ended, or the oldest line held; null (the newest) with none. */
	val before: Long? get() = cursor ?: oldest?.c

	/** Whether there may be older lines to load, and room for them. */
	val canLoadOlder: Boolean get() = !atStart && lineCount < Limits.LOG_LINES_MAX

	/**
	 * With the state [s] the stream sent, for a widget that keeps [keep] lines: its lines in place of
	 * these on a reset, else after them, "… n more" before them where it left some out.
	 */
	fun take(s: WidgetState, keep: Int): LogLines {
		// A note stays until a reset comes without one: the service may send it with the reset alone.
		val base = if (s.reset) LogLines(keep = keep, note = s.note, generation = generations.incrementAndGet()) else copy(keep = keep, note = s.note ?: note)
		var key = nextKey
		val items = base.items.toMutableList()
		if (s.more > 0) {
			// Left out again with no line since: one "… n more" for both.
			val last = items.lastOrNull()
			if (last is LogItem.More) items[items.lastIndex] = last.copy(count = last.count + s.more) else items += LogItem.More(key++, s.more)
		}
		for (l in s.lines.orEmpty()) items += LogItem.Line(key++, l)
		return base.copy(items = items, nextKey = key).trimmed()
	}

	/** Asking for an older page. */
	fun asking(): LogLines = copy(loadingOlder = true, olderFailed = false)

	/** The older page didn't come. */
	fun failed(): LogLines = copy(loadingOlder = false, olderFailed = true)

	/** There are no older lines to give: the service has no `/log` (before 0.8). */
	fun noOlder(): LogLines = copy(loadingOlder = false, olderFailed = false, atStart = true)

	/**
	 * With the older [page] before these: its lines older than the oldest held (one the stream sent
	 * meanwhile isn't there twice), as many as there is room for, the newest of them kept.
	 */
	fun older(page: LogPage): LogLines {
		val first = oldest?.c
		val fresh = page.lines.filter { first == null || it.c < first }
		val kept = fresh.takeLast((Limits.LOG_LINES_MAX - lineCount).coerceAtLeast(0))
		var key = nextKey
		val added = kept.map { LogItem.Line(key++, it) }
		// The page has what was left out before the first line.
		val rest = items.dropWhile { it is LogItem.More }
		return copy(
			items = added + rest,
			loaded = loaded + kept.size,
			cursor = page.before,
			atStart = page.before == null,
			loadingOlder = false,
			olderFailed = false,
			nextKey = key,
		)
	}

	/** Without the oldest lines past what is kept; the next older page then starts at the oldest left. */
	private fun trimmed(): LogLines {
		var drop = lineCount - minOf(Limits.LOG_LINES_MAX, keep + loaded)
		if (drop <= 0) return this
		var i = 0
		while (drop > 0) {
			if (items[i] is LogItem.Line) drop--
			i++
		}
		// "… n more" with nothing above it any more stands for nothing.
		while (i < items.size && items[i] is LogItem.More) i++
		return copy(items = items.drop(i), cursor = null, atStart = false)
	}
}

/** Counts the resets of every log, so that one left and shown anew doesn't take the generation it had before. */
private val generations = AtomicInteger()

/** How a line's level looks: critical and error alike, debug dimmed. */
enum class LogTone { Error, Warning, Notice, Normal, Dimmed }

fun logTone(level: String?): LogTone = when (level) {
	"critical", "error" -> LogTone.Error
	"warning" -> LogTone.Warning
	"notice" -> LogTone.Notice
	"debug" -> LogTone.Dimmed
	else -> LogTone.Normal
}

/** The levels a log offers to filter by, least first: critical only for a log that starts there. */
fun logLevels(own: String): List<String> = if (own == "critical") LOG_CHIPS + own else LOG_CHIPS

private val LOG_CHIPS = listOf("debug", "info", "notice", "warning", "error")

private val CLOCK = DateTimeFormatter.ofPattern("HH:mm:ss")

/** A line's time of day on the phone's clock, [t] in Unix ms. */
fun logClock(t: Long, zone: ZoneId = ZoneId.systemDefault()): String = CLOCK.format(Instant.ofEpochMilli(t).atZone(zone))

/** A line as it is copied: "14:02:01 warning sshd: Connection closed", its unit where it has no program's name. */
fun logCopy(line: LogLine, zone: ZoneId = ZoneId.systemDefault()): String =
	listOfNotNull(logClock(line.t, zone), line.level, line.id ?: line.unit).joinToString(" ") + ": " + line.text
