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

	fun columns(widthDp: Int): Int = if (widthDp < NARROW_DP) COLUMNS / 2 else COLUMNS

	/** A widget's [span] in a grid of [columns]: halved, rounding up, where there are half as many. */
	fun span(span: Int, columns: Int): Int {
		val s = span.coerceIn(1, COLUMNS)
		return ((s * columns + COLUMNS - 1) / COLUMNS).coerceIn(1, columns)
	}

	/**
	 * The widgets of [spans], in order, row by row: each as wide as its span in [columns], one that
	 * doesn't fit beside the one before starting a row, and the last of a row taking the columns
	 * left free, so no row ends in a hole (a lone temperature tile under two GPUs is as wide as the
	 * screen, as in 0.5). Each cell is (index into [spans], span).
	 */
	fun place(spans: List<Int>, columns: Int): List<List<Pair<Int, Int>>> {
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
		for (row in rows) {
			val free = columns - row.sumOf { it.second }
			if (free > 0) row[row.lastIndex] = row.last().let { (i, s) -> i to s + free }
		}
		return rows
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

object Tables {
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
	fun barFraction(row: Row, column: Column, rows: List<Row>): Float? {
		val v = row.f[column.field].number() ?: return null
		val top = barTop(row, column, rows) ?: return null
		if (top <= 0.0) return null
		return (v / top).toFloat().coerceIn(0f, 1f)
	}

	/** What the bar stands for when full: a number, a field of the row, or a quarter above the column's largest. */
	fun barTop(row: Row, column: Column, rows: List<Row>): Double? {
		val max = column.bar?.max ?: return null
		max.number()?.let { return it }
		val name = max.text() ?: return null
		if (name == TOP) return rows.mapNotNull { it.f[column.field].number() }.maxOrNull()?.let { it * TOP_HEADROOM }
		return row.f[name].number()
	}

	private const val TOP = "top"

	/** "top": a quarter above the largest. */
	private const val TOP_HEADROOM = 1.25
}

// ---- actions -----------------------------------------------------------------------------------

object Params {
	/** A whole "{field}", filled from a row. */
	private val FIELD = Regex("""^\{([a-z0-9_]+)}$""")

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

	/** The parameters of [action] the form asks for: those [fixed] leaves open that must be given (a default prefills them). */
	fun asked(action: Action, fixed: JsonObject): List<Param> = action.params.filter { it.id !in fixed && !it.optional }

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
