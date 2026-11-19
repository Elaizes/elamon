package io.github.elaizes.elamon.widget

import kotlinx.serialization.Serializable
import io.github.elaizes.elamon.data.DEFAULT_WIDGET_MINUTES
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.ui.DASH
import io.github.elaizes.elamon.ui.Fmt

/**
 * A line of the home-screen widget: a widget of the profile's `widget` dashboard as it read, its
 * label and its value as the app writes it, and its level for the colour.
 */
@Serializable
data class WidgetLine(
	val label: String,
	val text: String,
	/** "normal", "warning" or "critical", as the service says; null for none. */
	val level: String? = null,
	val blend: Double? = null,
	/** Green while normal, as a temperature's figure and a status are on their tiles; the text's own colour otherwise. */
	val green: Boolean = false,
	/** Words rather than a figure: a `text` widget's, drawn across the widget. */
	val words: Boolean = false,
)

/** What the last try to read a computer for its widget found. */
enum class Fetch {
	/** Not tried yet. */
	None,
	Ok,

	/** None of its addresses answered. */
	Offline,

	/** It turned this phone away, or the phone can't read its own token. */
	Unpaired,

	/** Its service speaks another API. */
	Outdated,

	/** Its profile names no dashboard for the widget, or this phone's role sees none of it. */
	NoDashboard,

	/** It answered, but not with a reading. */
	Failed,
}

/**
 * What a computer's widget shows: the [lines] last read at [readAt] (the phone's clock, Unix ms; 0
 * before the first), kept while it can't be reached, and what the last try found ([fetch]).
 */
@Serializable
data class WidgetShown(
	val lines: List<WidgetLine> = emptyList(),
	val readAt: Long = 0,
	/** When it was last tried, read or not (the phone's clock); 0 never: what decides whether it is due. */
	val triedAt: Long = 0,
	val fetch: Fetch = Fetch.None,
	/** The dashboard's title. */
	val title: String? = null,
	/** The profile the computer gives this phone: where `widget` is set. */
	val profile: String? = null,
	/** How often it is read in the background, as the profile asks. */
	val minutes: Int = DEFAULT_WIDGET_MINUTES,
) {
	/** As it is, after a try at [at] that found [fetch]: what was read before stays, to be shown as old. */
	fun tried(fetch: Fetch, at: Long = System.currentTimeMillis()): WidgetShown = copy(fetch = fetch, triedAt = at)
}

private const val TEMP = "temp"

/** The types the widget draws; any other (a graph, a table, a button) is left out. */
private val FIGURES = setOf("value", "gauge", "meter", "status")

/**
 * The lines of [dashboard] of [ui] in the reading [states]: each value, gauge, meter and status as
 * its label and its value, each text as its words, each list's items as theirs, groups opened out
 * in place; other types left out, and a text or an item with nothing to say.
 */
fun widgetLines(ui: UiDoc, dashboard: String, states: Map<String, WidgetState>): List<WidgetLine> =
	ui.dashboard(dashboard)?.widgets.orEmpty().flatMap { linesOf(it, states) }

private fun linesOf(w: Widget, states: Map<String, WidgetState>): List<WidgetLine> {
	val s = states[w.id]
	return when (w.type) {
		in FIGURES -> listOf(
			WidgetLine(w.label.orEmpty(), s?.text ?: Fmt.format(s?.v, w.format, w.unit) ?: DASH, s?.level, s?.blend, green = w.type == "status" || w.format == TEMP),
		)
		"text" -> listOfNotNull((s?.text ?: w.text)?.takeIf { it.isNotBlank() }?.let { WidgetLine(w.label.orEmpty(), it, s?.level ?: w.level, words = true) })
		"list" -> w.items.orEmpty().mapIndexedNotNull { i, item ->
			val st = s?.items?.getOrNull(i)
			val text = st?.text ?: Fmt.format(st?.v, item.format, item.unit) ?: return@mapIndexedNotNull null
			WidgetLine(item.label.orEmpty(), text, st?.level, green = item.format == TEMP)
		}
		"group" -> w.widgets.orEmpty().flatMap { linesOf(it, states) }
		else -> emptyList()
	}
}

/** The dashboard the widget shows: the profile's `widget`, where the ui has it (the role may see none of it). */
fun widgetDashboard(ui: UiDoc): String? = ui.profile.android.widget.takeIf { it.isNotEmpty() && ui.dashboard(it) != null }

/** The least minutes between background readings Android allows, and the most the profile may ask. */
const val MIN_MINUTES = 15
const val MAX_MINUTES = 1440

/** How often a widget's computer is read in the background: the profile's minutes, as Android allows them. */
fun widgetMinutes(asked: Int): Int = asked.coerceIn(MIN_MINUTES, MAX_MINUTES)
