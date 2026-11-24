package io.github.elaizes.elamon.data

import kotlin.math.abs
import kotlin.math.floor
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

/**
 * What the screens do with the dashboards the service describes, apart from drawing them: where
 * each widget goes in the grid, what a table shows, and what an action is sent. Pure, so it is tested.
 */
object Grid {
	/** The grid's columns on a wide screen; a narrow one has half. */
	const val COLUMNS = 4

	/** Narrower than this (dp), a screen is narrow: a phone held upright. */
	const val NARROW_DP = 600

	/**
	 * Narrower than this (dp), one column: two would leave a tile too narrow for a name beside its
	 * icon. No phone is, at the app's own scale of 100%; a phone at a large one may be.
	 */
	const val ONE_COLUMN_DP = 320

	fun columns(widthDp: Int): Int = when {
		widthDp < ONE_COLUMN_DP -> 1
		widthDp < NARROW_DP -> COLUMNS / 2
		else -> COLUMNS
	}

	/** A widget's [span] in a grid of [columns]: halved, rounding up, where there are half as many. */
	fun span(span: Int, columns: Int): Int {
		val s = span.coerceIn(1, COLUMNS)
		return ((s * columns + COLUMNS - 1) / COLUMNS).coerceIn(1, columns)
	}

	/**
	 * A row of widgets: each [cells]' place in the list it was made from and how many of the row's
	 * [columns] it is wide. The columns are the grid's where the widgets fill them; a row that they
	 * leave incomplete has one for each, so they share its width evenly and it still runs from edge
	 * to edge, the same on both sides.
	 */
	data class Line(val cells: List<Pair<Int, Int>>, val columns: Int)

	/**
	 * The widgets of [spans], in order, row by row: each as wide as its span in [columns], one that
	 * doesn't fit beside the one before starting a row ([Line]).
	 */
	fun place(spans: List<Int>, columns: Int): List<Line> {
		val rows = mutableListOf<MutableList<Pair<Int, Int>>>()
		var used = columns
		spans.forEachIndexed { i, raw ->
			val s = span(raw, columns)
			if (used + s > columns) {
				rows += mutableListOf<Pair<Int, Int>>()
				used = 0
			}
			rows.last() += i to s
			used += s
		}
		return rows.map { row -> if (row.sumOf { it.second } == columns) Line(row, columns) else shared(row.map { it.first }) }
	}

	/** [count] things of one kind, as many as [most] to a row, in rows as even as they go: four by three are two and two. */
	fun even(count: Int, most: Int): List<Line> {
		if (count <= 0) return emptyList()
		val rows = (count + most - 1) / most
		var next = 0
		return List(rows) { i ->
			val n = count / rows + if (i < count % rows) 1 else 0
			shared(List(n) { next++ })
		}
	}

	/** A row [of] those places, each as wide as the others. */
	private fun shared(of: List<Int>) = Line(of.map { it to 1 }, of.size)

	/**
	 * From where to where a widget reaches that takes [span] of a row's [columns] from [column] on, in
	 * whole pixels of a row [width] wide with [gap] between columns: its mirror image, as many columns
	 * from the other end, is as wide and as far from the edge; a pixel left over widens a gap.
	 */
	fun extent(column: Int, span: Int, columns: Int, width: Int, gap: Int): Extent {
		fun start(c: Int) = (c * (width + gap) + columns - 1) / columns
		return Extent(start(column), width - start(columns - column - span))
	}

	/** Where a widget starts in its row and where it ends, in pixels. */
	data class Extent(val start: Int, val end: Int) {
		val width get() = end - start
	}
}

/** What a widget needs from the state to be drawn at all: a table that hides while empty, hidden then. */
fun shown(w: Widget, state: WidgetState?): Boolean =
	!(w.type == "table" && w.hideEmpty == true && state != null && state.rows.isNullOrEmpty())

/** The tabs of [ui] there is something to show for: those its dashboards have. None, and the screen says so. */
fun tabsShown(ui: UiDoc?): List<String> = ui?.tabs.orEmpty().filter { ui?.dashboard(it) != null }.distinct()

/**
 * Keys for the items of a lazy list, each given once: one asked for again gets a number after it.
 * A list given the same key twice throws, and a service may send two rows with one key, or two
 * widgets with one id.
 */
class UniqueKeys {
	private val used = HashSet<String>()

	fun of(key: String): String {
		if (used.add(key)) return key
		var n = 2
		while (!used.add("$key#$n")) n++
		return "$key#$n"
	}
}

// ---- tables ----------------------------------------------------------------------------------

/** A row action as a row's sheet offers it: the page it [opens], or the action it [runs]. */
data class RowButton(val rowAction: RowAction, val opens: String?, val runs: Action?)

object Tables {
	/**
	 * What the sheet of [w]'s [row] offers of its row actions: the page one opens,
	 * `<open>.<the row's key>`, while [ui] has it, or the action one runs, while [ui] has that. One
	 * with neither (a page or an action this phone isn't given, a kind this app doesn't know) isn't.
	 */
	fun rowButtons(ui: UiDoc, w: Widget, row: Row): List<RowButton> = w.rowActions.orEmpty().mapNotNull { ra ->
		val opens = ra.open?.let { "$it.${row.key}" }?.takeIf { ui.dashboard(it) != null }
		val runs = if (ra.open == null) ui.action(ra.action) else null
		if (opens == null && runs == null) null else RowButton(ra, opens, runs)
	}

	/**
	 * [text], the [field] of a row of [w], as its sheet shows it: the link a tap on the row opens
	 * ([Widget.rowOpen]) as the phone opens it, at [host], the address it reaches the computer at
	 * (`http://{host}:8096` as `http://192.0.2.10:8096`); anything else, or with no address yet, as it is.
	 */
	fun shownField(w: Widget, field: String, text: String, host: String?): String =
		if (host != null && field == w.rowOpen) linkUrl(text, host) else text

	/** The text columns a table's search looks in: those that hold words, not figures. */
	fun textColumns(columns: List<Column>): List<Column> = columns.filter { it.format == null || it.format == "text" }

	/** [rows] whose text columns hold [query], any case; all of them for a blank one. */
	fun search(rows: List<Row>, columns: List<Column>, query: String): List<Row> =
		searchIn(rows, textColumns(columns).map { it.field }, query)

	/** The fields [w]'s search looks in: those the service names, or its text columns. */
	fun searchFields(w: Widget): List<String> = w.searchFields ?: textColumns(w.columns.orEmpty()).map { it.field }

	/** [rows] where one of [fields], as text, holds [query], any case; all of them for a blank one. */
	fun searchIn(rows: List<Row>, fields: List<String>, query: String): List<Row> {
		val q = query.trim()
		if (q.isEmpty()) return rows
		return rows.filter { r -> fields.any { f -> r.f[f].text()?.contains(q, ignoreCase = true) == true } }
	}

	/**
	 * [rows] by [field], largest first with [desc]: numbers as numbers, words as words, a row
	 * without the field last. Stable, so rows the field doesn't tell apart keep the service's order.
	 */
	fun sort(rows: List<Row>, field: String, desc: Boolean): List<Row> {
		val present = rows.filter { it.f[field].let { v -> v != null && v !is JsonNull } }
		val absent = rows - present.toSet()
		val numeric = present.all { it.f[field].number() != null }
		val sorted = if (numeric) {
			if (desc) present.sortedByDescending { it.f[field].number() } else present.sortedBy { it.f[field].number() }
		} else {
			val c = compareBy<Row> { it.f[field].text().orEmpty().lowercase() }
			present.sortedWith(if (desc) c.reversed() else c)
		}
		return sorted + absent
	}

	/** How much of the bar of [column] is filled in [row], 0 to 1, among [rows]; null when it can't be told. */
	fun barFraction(row: Row, column: Column, rows: List<Row>): Float? = fraction(row, column) { largest(column, rows) }

	/** [barFraction] of each of [rows]: the column's largest, for "top", looked for once, not once a row. */
	fun barFractions(rows: List<Row>, column: Column): List<Float?> {
		val top by lazy { largest(column, rows) }
		return rows.map { fraction(it, column) { top } }
	}

	/** What the bar stands for when full: a number, a field of the row, or a quarter above the column's largest. */
	fun barTop(row: Row, column: Column, rows: List<Row>): Double? = barTop(row, column) { largest(column, rows) }

	private inline fun fraction(row: Row, column: Column, top: () -> Double?): Float? {
		val v = row.f[column.field].number() ?: return null
		val full = barTop(row, column, top) ?: return null
		if (full <= 0.0) return null
		return (v / full).toFloat().coerceIn(0f, 1f)
	}

	private inline fun barTop(row: Row, column: Column, top: () -> Double?): Double? {
		val max = column.bar?.max ?: return null
		max.number()?.let { return it }
		val name = max.text() ?: return null
		if (name == TOP) return top()
		return row.f[name].number()
	}

	/** "top": a quarter above the column's largest. */
	private fun largest(column: Column, rows: List<Row>): Double? =
		rows.mapNotNull { it.f[column.field].number() }.maxOrNull()?.let { it * TOP_HEADROOM }

	private const val TOP = "top"

	/** "top": a quarter above the largest. */
	private const val TOP_HEADROOM = 1.25
}

// ---- actions -----------------------------------------------------------------------------------

/** What a text typed before an action is: the station's command itself, a monitor's name, or another (a newer service's). */
enum class Typed { Command, MonitorName, Other }

object Params {
	/** A whole "{field}", filled from a row. */
	private val FIELD = Regex("""^\{([a-z0-9_]+)\}$""")

	/**
	 * [template]'s values with each that is exactly "{field}" taken from [row] as the JSON value it
	 * is there (a number stays a number); one whose field the row lacks is left out.
	 */
	fun fill(template: JsonObject, row: JsonObject): JsonObject = buildJsonObject {
		for ((k, v) in template) {
			val field = (v as? JsonPrimitive)?.takeIf { it.isString }?.let { FIELD.matchEntire(it.content)?.groupValues?.get(1) }
			if (field == null) {
				put(k, v)
				continue
			}
			val value = row[field]
			if (value != null && value !is JsonNull) put(k, value)
		}
	}

	/**
	 * The text to type before [action] runs with [params]: that of the choice the value of a `choice`
	 * parameter is, if it carries one. A station's command is itself; a monitor's is its name, not
	 * the value (`off`).
	 */
	fun typedText(action: Action, params: JsonObject): String? = typedChoice(action, params)?.confirmText

	/** What the text [typedText] asks for is, for the words that ask for it; null where none is asked. */
	fun typedKind(action: Action, params: JsonObject): Typed? {
		val c = typedChoice(action, params) ?: return null
		return when {
			action.kind == DISPLAY -> Typed.MonitorName
			c.confirmText == c.value -> Typed.Command
			else -> Typed.Other
		}
	}

	private fun typedChoice(action: Action, params: JsonObject): Choice? = action.params.firstNotNullOfOrNull { p ->
		val v = (params[p.id] as? JsonPrimitive)?.takeIf { it.isString }?.content
		p.choices?.firstOrNull { it.value == v && it.confirmText != null }
	}

	/** The kind of a monitor's actions (elamond 0.10). */
	private const val DISPLAY = "display"

	/** Whether what was typed is the text asked for: exactly, nothing trimmed. */
	fun typedMatches(typed: String, text: String): Boolean = typed == text

	/** The parameters of [action] the form asks for: those [fixed] leaves open that must be given (a default prefills them). */
	fun asked(action: Action, fixed: JsonObject): List<Param> = action.params.filter { it.id !in fixed && !it.optional }

	/** Whether [p] is the one parameter [asked] and is called what the form's [title] says, so its own name would repeat it. */
	fun namedByTitle(asked: List<Param>, p: Param, title: String): Boolean =
		asked.size == 1 && (p.label ?: p.id).trim().equals(title.trim(), ignoreCase = true)

	/** What the form starts with for [p]: its default, else the first choice, the range's start, or off. */
	fun initial(p: Param): JsonElement? = p.default?.takeIf { it !is JsonNull } ?: when (p.kind) {
		"choice" -> p.choices?.firstOrNull()?.let { JsonPrimitive(it.value) }
		"range" -> p.min?.let { number(it, p) }
		"bool" -> JsonPrimitive(false)
		else -> null
	}

	/** [v] as the JSON number [p] takes: an integer where its step and start are whole. */
	fun number(v: Double, p: Param): JsonPrimitive =
		if (whole(p.step ?: 1.0) && whole(p.min ?: 0.0) && whole(v)) JsonPrimitive(v.toLong()) else JsonPrimitive(v)

	/** [v] on [p]'s range and step, the nearest it takes. */
	fun snap(v: Double, p: Param): Double {
		val min = p.min ?: return v
		val max = p.max ?: Double.MAX_VALUE
		val step = p.step?.takeIf { it > 0 }
		val on = if (step == null) v else min + Math.round((v - min) / step) * step
		return on.coerceIn(min, max)
	}

	/** Whether [v] is a value [p] takes, as the service checks it. */
	fun valid(p: Param, v: JsonElement?): Boolean {
		if (v == null || v is JsonNull) return p.optional || p.default != null
		val prim = v as? JsonPrimitive ?: return false
		return when (p.kind) {
			"choice" -> prim.isString && p.choices.orEmpty().any { it.value == prim.content }
			"bool" -> !prim.isString && prim.bool() != null
			"string" -> prim.isString
			"integer" -> prim.number()?.let { whole(it) } == true
			"number" -> prim.number() != null
			"range" -> {
				val n = prim.number() ?: return false
				val min = p.min ?: Double.NEGATIVE_INFINITY
				val max = p.max ?: Double.POSITIVE_INFINITY
				if (n < min - EPSILON || n > max + EPSILON) return false
				val step = p.step?.takeIf { it > 0 } ?: return true
				val k = (n - (p.min ?: 0.0)) / step
				abs(k - Math.round(k)) < EPSILON
			}
			// A kind from a newer service: the service checks it.
			else -> true
		}
	}

	/** Every parameter of [action] given a value it takes, or optional; [params] has nothing it lacks. */
	fun complete(action: Action, params: JsonObject): Boolean =
		params.keys.all { k -> action.params.any { it.id == k } } &&
			action.params.all { p -> valid(p, params[p.id]) }

	private fun whole(v: Double) = v == floor(v) && !v.isInfinite()

	private const val EPSILON = 1e-9
}
