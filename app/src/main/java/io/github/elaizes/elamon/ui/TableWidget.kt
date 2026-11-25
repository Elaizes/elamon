package io.github.elaizes.elamon.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.findWidget
import io.github.elaizes.elamon.data.Column as TableColumn
import io.github.elaizes.elamon.data.Params
import io.github.elaizes.elamon.data.Row as TableRow
import io.github.elaizes.elamon.data.Tables
import io.github.elaizes.elamon.data.UniqueKeys
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.linkUrl
import io.github.elaizes.elamon.data.text

/**
 * What the tables of a screen keep while it lives, a rotation included ([MachineDialogs]): each
 * one's search, and the row whose sheet is open (with its last readings, should it leave the list).
 * The order asked for is the view model's, which the stream is told: it outlives the screen.
 */
class TableStates {
	private val queries = mutableStateMapOf<String, String>()

	/** The table and the row whose sheet is open. */
	var sheet by mutableStateOf<Pair<String, TableRow>?>(null)
		private set

	fun query(table: String): String = queries[table].orEmpty()

	fun setQuery(table: String, q: String) {
		queries[table] = q
	}

	fun open(table: String, row: TableRow) {
		sheet = table to row
	}

	fun close() {
		sheet = null
	}
}

/** The rows of [w] as shown: searched, and in the order asked for until the service sends them so. */
fun tableRows(scope: DashScope, w: Widget): List<TableRow> {
	val rows = scope.state(w)?.rows.orEmpty()
	val searched = if (w.search == true) Tables.searchIn(rows, Tables.searchFields(w), scope.tables.query(w.id)) else rows
	val asked = scope.sorts[w.id]
	return if (asked != null && asked != w.sort?.field) Tables.sort(searched, asked, desc = true) else searched
}

/** The column whose text takes the room left: the one that grows, or the first. */
private fun growColumn(w: Widget): TableColumn? = w.columns.orEmpty().let { c -> c.firstOrNull { it.grow } ?: c.firstOrNull() }

private fun barColumn(w: Widget): TableColumn? = w.columns?.firstOrNull { it.bar != null }

/** A cell's text: its value by its column's format and unit; a dash while it is missing. */
private fun cell(row: TableRow, c: TableColumn): String = Fmt.format(row.f[c.field], c.format, c.unit, c.field) ?: DASH

/** A cell's [text] with its column's label after it: "120 GiB free". */
private fun named(text: String, c: TableColumn): String = if (c.label != null) "$text ${c.label}" else text

/** Rows of processes and links get a letter avatar, as their lists had in 0.5. */
private fun avatars(w: Widget) = w.source == "processes" || w.source == "links"

/** How a table's columns stand: how wide each is but the one that takes the room left, and whether they go [under] its text. */
class ColumnFit(val widths: Map<TableColumn, Dp>, val under: Boolean)

/**
 * The columns of [w], in a row of the list's own ([card]) or in a card: a figure's as wide as its
 * longest figure would be, whatever it reads now, so it doesn't move as they change; one of words as
 * its longest among all the [rows], a search's hits or not, so a short one leaves the rows' names
 * their room. Heads fit; with [labelled], the label after each figure. Where the names would have a
 * few letters' room, all of it goes under them, in the row's width.
 */
@Composable
private fun columnFit(w: Widget, rows: List<TableRow>, card: Boolean, labelled: Boolean = false): ColumnFit {
	val grow = growColumn(w)
	val bar = barColumn(w)
	val measurer = rememberTextMeasurer()
	val density = LocalDensity.current
	val figure = MaterialTheme.typography.bodySmall
	val barFigure = MaterialTheme.typography.titleSmall
	val head = MaterialTheme.typography.labelSmall
	val avatar = if (!avatars(w)) Space.none else (if (card) Sizes.avatar else Sizes.avatarS) + Space.m
	val row = LocalCellWidth.current - (if (card) Space.m else cardPadding()) * 2
	val least = TextSizes.nameLeast.forText()
	// The longest few as written, the widest of which is the widest of all but for a hair.
	val words = remember(w, rows, labelled) {
		w.columns.orEmpty().filter { it != grow || it == bar }.associateWith { c ->
			val texts = Fmt.longest(c.format, c.unit)?.let(::listOf) ?: rows.mapTo(HashSet()) { cell(it, c) }.sortedByDescending { it.length }.take(LONGEST_MEASURED)
			if (labelled && c != bar) texts.map { named(it, c) } else texts
		}
	}
	return remember(words, density, row, least, figure, barFigure, head) {
		fun width(text: String, style: TextStyle) = measurer.measure(text, style, maxLines = 1, softWrap = false).size.width
		val widths = words.mapValues { (c, texts) ->
			val px = maxOf(texts.maxOfOrNull { width(it, if (c == bar) barFigure else figure) } ?: 0, c.label?.takeUnless { labelled }?.let { width(it, head) } ?: 0)
			// A pixel more than its text: rounded down on the way, it would be cut.
			with(density) { (px + 1).toDp() }
		}
		fun Collection<Dp>.total() = fold(Space.none) { sum, d -> sum + d }
		val under = row - avatar - widths.values.total() - Space.m * widths.size < least
		if (!under) return@remember ColumnFit(widths, false)
		val (ofWords, ofFigures) = widths.entries.partition { (c, _) -> Fmt.longest(c.format, c.unit) == null }
		ColumnFit(widths + columnsWithin(ofWords.associate { it.key to it.value }, row - Space.m * widths.size - ofFigures.map { it.value }.total()), true)
	}
}

/** [widths] within [room] together: the widest made narrower, down to as much as the others have. */
internal fun <T> columnsWithin(widths: Map<T, Dp>, room: Dp): Map<T, Dp> {
	if (widths.isEmpty() || widths.values.fold(Space.none) { sum, v -> sum + v } <= room) return widths
	// The narrow ones keep theirs, the rest share what is left of the room evenly.
	var left = room
	var open = widths.size
	val kept = mutableMapOf<T, Dp>()
	for ((k, v) in widths.entries.sortedBy { it.value }) {
		val share = left / open
		kept[k] = minOf(v, maxOf(share, Sizes.columnLeast))
		left -= kept.getValue(k)
		open--
	}
	return kept
}

/** How many of a column's texts are measured: its longest by their letters. */
private const val LONGEST_MEASURED = 8

/** An expanded field longer than this is set as a block of its own, like a command line. */
private const val LONG_FIELD = 40

/** The most rows a table in a card draws; those past them are counted under it. */
private const val CARD_ROWS = 100

/**
 * Rows from a collection, in a card: a heading for the columns that have labels, each row's text
 * and figures, and a bar under a row where a column is one (the sensors, the disks). On a tab, one
 * that opens a page is a tile, as the overview's storage was.
 */
@Composable
fun TableWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val rows = tableRows(scope, w)
	val foldable = w.collapsed != null
	val folded = scope.folds.folded(w)
	val accent = accent(w.color)
	val open = w.open?.let { d -> { scope.open(d) } }
	val tile = open != null && scope.dashboard in scope.ui.tabs
	val fit = columnFit(w, s?.rows.orEmpty(), card = false, labelled = tile)
	val rowsShown: @Composable () -> Unit = {
		if (w.search == true) TableControls(scope, w, s?.total, rows.size)
		// A tile names its figures beside them ("120 GiB free"), a card over them.
		if (!tile) TableHead(w, fit.widths)
		if (rows.isEmpty()) {
			Text(stringResource(if (s == null) R.string.loading else R.string.table_empty), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		}
		// A card is drawn whole, every line: so many rows and no more (a table with a search field has the list's own).
		val drawn = rows.take(CARD_ROWS)
		val fractions = barColumn(w)?.let { Tables.barFractions(rows, it) }
		// Rows to touch are as tall as a finger, and so as far apart; others as far apart as a card's lines.
		val touch = opensRow(w)
		Column(verticalArrangement = Arrangement.spacedBy(if (touch) Space.none else Space.m)) {
			drawn.forEachIndexed { i, r -> TableRowView(scope, w, r, fractions?.get(i), fit, card = false, labelled = tile) }
		}
		val more = rows.size - drawn.size
		if (more > 0) {
			Text(pluralStringResource(R.plurals.table_more, more, more), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		}
	}
	if (tile) {
		WidgetTile(scope, w, accent, modifier, captionOf(scope, w, s)) { if (!folded) rowsShown() }
	} else {
		WidgetSection(
			scope,
			w,
			accent,
			modifier,
			trailing = if (foldable) ({ FoldMark(folded) }) else null,
			onTitleClick = if (foldable) ({ scope.folds.toggle(w) }) else null,
		) {
			// Under a card's title, as the Displays section's summary was.
			captionOf(scope, w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
			if (!folded) rowsShown()
		}
	}
}

/** A table whose rows are the screen's list's own: they, and how its columns stand. */
class ListTable(val w: Widget, val rows: List<TableRow>, val fit: ColumnFit)

@Composable
fun listTable(scope: DashScope, w: Widget): ListTable {
	return ListTable(w, tableRows(scope, w), columnFit(w, scope.state(w)?.rows.orEmpty(), card = true))
}

/**
 * A table with a search field (the processes, the activity log): its search and orders, then its
 * rows as the list's own, each a card, as 0.5's Processes tab.
 */
fun LazyListScope.tableItems(scope: DashScope, table: ListTable, keys: UniqueKeys) {
	val w = table.w
	val rows = table.rows
	val s = scope.state(w)
	item(key = keys.of("table/${w.id}")) {
		Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
			w.label?.let { Text(it, style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(horizontal = Space.card)) }
			captionOf(scope, w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = Space.card)) }
			TableControls(scope, w, s?.total, rows.size, inset = Space.card)
			if (s == null || rows.isEmpty()) {
				Text(stringResource(if (s == null) R.string.loading else R.string.table_empty), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = Space.card))
			}
		}
	}
	val rowKeys = rows.map { keys.of("table/${w.id}/${it.key}") }
	val fractions = barColumn(w)?.let { Tables.barFractions(rows, it) }
	items(rows.size, key = { rowKeys[it] }) { i -> TableRowView(scope, w, rows[i], fractions?.get(i), table.fit, card = true) }
}

/** The search field, the orders the service offers, and how many rows there are, that [inset] from the sides. */
@Composable
private fun TableControls(scope: DashScope, w: Widget, total: Int?, shown: Int, inset: Dp = Space.none) {
	val query = scope.tables.query(w.id)
	Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
		SearchField(query, stringResource(R.string.table_search)) { scope.tables.setQuery(w.id, it) }
		val sorts = (listOfNotNull(w.sort?.field) + w.sorts.orEmpty()).distinct()
		if (sorts.size > 1) {
			val current = scope.sorts[w.id] ?: w.sort?.field
			ChipRow {
				for (f in sorts) {
					FilterChip(
						selected = f == current,
						onClick = { if (f != current) scope.vm.sort(w, f) },
						label = { Text(fieldLabel(w, f)) },
						shape = PillShape,
					)
				}
			}
		}
		if (total != null) {
			Text(
				if (query.isBlank() || shown == total) pluralStringResource(R.plurals.table_total, total, total)
				else pluralStringResource(R.plurals.table_found, shown, shown, total),
				style = MaterialTheme.typography.labelMedium,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
				modifier = Modifier.padding(horizontal = inset),
			)
		}
	}
}

/** A search field, as rounded as a card; its clearing button once something is typed. */
@Composable
fun SearchField(query: String, hint: String, onChange: (String) -> Unit) {
	val fill = cardColor()
	OutlinedTextField(
		value = query,
		onValueChange = onChange,
		modifier = Modifier.fillMaxWidth(),
		singleLine = true,
		shape = CardShape,
		placeholder = { Text(hint, maxLines = 1, overflow = TextOverflow.Ellipsis) },
		leadingIcon = { Icon(Icons.Default.Search, null) },
		trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { onChange("") }) { Icon(Icons.Default.Clear, stringResource(R.string.processes_clear)) } },
		colors = OutlinedTextFieldDefaults.colors(unfocusedContainerColor = fill, focusedContainerColor = fill, unfocusedBorderColor = fill),
	)
}

/** What a field is called: its column's label, else its expanded field's, else its name. */
private fun fieldLabel(w: Widget, field: String): String =
	w.sortLabels?.get(field) ?: w.columns?.find { it.field == field }?.label ?: w.expand?.find { it.field == field }?.label ?: field.replaceFirstChar { it.uppercase() }

/** The columns' labels over the rows, where any has one. */
@Composable
private fun TableHead(w: Widget, widths: Map<TableColumn, Dp>) {
	val cols = w.columns.orEmpty()
	if (cols.none { it.label != null }) return
	val grow = growColumn(w)
	val bar = barColumn(w)
	Row {
		if (avatars(w)) Spacer(Modifier.width(Sizes.avatarS + Space.m))
		HeadCell(grow?.label, TextAlign.Start, Modifier.weight(1f))
		for (c in cols.filter { it != grow && it != bar } + listOfNotNull(bar)) HeadCell(c.label, TextAlign.End, Modifier.padding(start = Space.m).width(widths.getValue(c)))
	}
}

@Composable
private fun HeadCell(label: String?, align: TextAlign, modifier: Modifier) {
	Text(label.orEmpty(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = align, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = modifier)
}

/**
 * One row: its growing column's text, the other columns' figures, and the bar column's figure in
 * the row's level's colour with its bar under the row ([card]: a list's own card, the bar under
 * the figure). A tap opens what it links to, or its sheet; a long press, the sheet.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TableRowView(scope: DashScope, w: Widget, row: TableRow, fraction: Float?, fit: ColumnFit, card: Boolean, labelled: Boolean = false) {
	val ctx = LocalContext.current
	val haptics = LocalHapticFeedback.current
	val prefs = LocalPrefs.current
	val cols = w.columns.orEmpty()
	val grow = growColumn(w)
	val bar = barColumn(w)
	val others = cols.filter { it != grow && it != bar }
	val hasSheet = !w.expand.isNullOrEmpty() || !w.rowActions.isNullOrEmpty()
	val url = w.rowOpen?.let { row.f[it].text() }
	val accent = accent(w.color)
	val barColor = bar?.let { levelColor(row.level, row.blend, if (it.format == "temp") Metric.colors.good else accent) ?: accent }
	val sheet = { scope.tables.open(w.id, row) }
	val tap: (() -> Unit)? = when {
		url != null -> { { openRowLink(ctx, scope, url) } }
		hasSheet -> sheet
		else -> null
	}
	val click = if (tap == null) Modifier else Modifier.combinedClickable(
		role = Role.Button,
		onClickLabel = stringResource(if (url != null) R.string.control_open else R.string.table_details),
		onLongClickLabel = if (url != null && hasSheet) stringResource(R.string.table_details) else null,
		onLongClick = if (url != null && hasSheet) {
			{
				if (prefs.haptics) haptics.performHapticFeedback(HapticFeedbackType.LongPress)
				sheet()
			}
		} else null,
		onClick = tap,
	)
	val name = grow?.let { cell(row, it) }.orEmpty()
	val content: @Composable () -> Unit = {
		val avatar = if (!avatars(w)) null else if (card) Sizes.avatar else Sizes.avatarS
		val figures: @Composable RowScope.() -> Unit = {
			for (c in others) {
				// In a tile the figure's name follows it.
				val text = cell(row, c).let { v -> if (labelled) named(v, c) else v }
				Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = Space.m).width(fit.widths.getValue(c)), textAlign = TextAlign.End, maxLines = 1, overflow = TextOverflow.Ellipsis)
			}
			if (bar != null && barColor != null) {
				Column(Modifier.padding(start = Space.m).width(fit.widths.getValue(bar)), horizontalAlignment = Alignment.End) {
					Text(cell(row, bar), style = MaterialTheme.typography.titleSmall, color = barColor, maxLines = 1)
					if (card) {
						Spacer(Modifier.height(Space.xs))
						UsageBar(fraction ?: 0f, barColor, height = Sizes.barThin)
					}
				}
			}
		}
		val style = if (card) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodyMedium
		Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				if (avatar != null) {
					LetterAvatar(name, avatar)
					Spacer(Modifier.width(Space.m))
				}
				Text(name, style = style, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
				if (!fit.under) figures()
			}
			if (fit.under) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically, content = figures)
			if (!card && bar != null && barColor != null) UsageBar(fraction ?: 0f, barColor, height = Sizes.bar)
		}
	}
	if (card) {
		CardSurface { Column(click.padding(Space.m)) { content() } }
	} else {
		Column(Modifier.fillMaxWidth().then(click).heightIn(min = if (tap == null) Space.none else Sizes.touchTarget), verticalArrangement = Arrangement.Center) { content() }
	}
}

/** Whether a row of [w] may be touched: it opens a link, or a sheet. */
private fun opensRow(w: Widget): Boolean = w.rowOpen != null || !w.expand.isNullOrEmpty() || !w.rowActions.isNullOrEmpty()

/** A row's link, at the address this phone reaches the computer at; a web page only. */
private fun openRowLink(ctx: android.content.Context, scope: DashScope, url: String) {
	val host = scope.machine.host ?: return
	openWebLink(ctx, linkUrl(url, host))
}

/**
 * The open row's sheet, as 0.5's process sheet: its figures, what `expand` shows, its link, and
 * its actions as buttons, their parameters filled from the row. A row that leaves the list keeps
 * its last readings, marked as such.
 */
@Composable
fun TableSheet(scope: DashScope) {
	val (table, last) = scope.tables.sheet ?: return
	val w = findWidget(scope.ui, table) ?: return
	val state = scope.states[table]
	val live = state?.rows?.find { it.key == last.key }
	val row = live ?: last
	val gone = state != null && live == null
	val ctx = LocalContext.current
	val cols = w.columns.orEmpty()
	val grow = growColumn(w)
	val name = grow?.let { cell(row, it) }.orEmpty()
	val accent = accent(w.color)
	GlassSheet(onDismiss = { scope.tables.close() }) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			if (avatars(w)) {
				LetterAvatar(name, Sizes.badgeL)
				Spacer(Modifier.width(Space.m))
			}
			Text(breakable(name), style = MaterialTheme.typography.headlineSmall, maxLines = if (largeText()) 4 else 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).wholeWords())
		}
		if (gone) Text(stringResource(R.string.table_row_gone), style = MaterialTheme.typography.bodySmall, color = Metric.colors.warn)
		val figures = cols.filter { it != grow }
		if (figures.isNotEmpty()) {
			val measurer = rememberTextMeasurer()
			val style = MaterialTheme.typography.titleLarge
			// As many side by side as are whole there.
			BoxWithConstraints {
				val widest = with(LocalDensity.current) { figures.maxOf { measurer.measure(cell(row, it), style, maxLines = 1, softWrap = false).size.width }.toDp() }
				val most = ((maxWidth + Space.m) / (widest + Space.m)).toInt().coerceIn(1, FIGURES_IN_A_ROW)
				EvenRows(figures, most, gap = Space.m) { c ->
					val color = if (c.bar != null) levelColor(row.level, row.blend, if (c.format == "temp") Metric.colors.good else accent) ?: accent else MaterialTheme.colorScheme.onSurface
					BigStat(c.label ?: fieldLabel(w, c.field), cell(row, c), color = color)
				}
			}
		}
		for (e in w.expand.orEmpty()) {
			// The link as the button opens it, not its template ("http://{host}:8096").
			val v = Fmt.format(row.f[e.field], e.format, e.unit, e.field)?.ifBlank { null }?.let { Tables.shownField(w, e.field, it, scope.machine.host) } ?: continue
			val label = e.label ?: e.field
			if (v.length > LONG_FIELD) {
				Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
					Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
					SelectionContainer { Text(v, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace) }
				}
			} else {
				StatRow(label, v)
			}
		}
		val url = w.rowOpen?.let { row.f[it].text() }
		if (url != null) {
			Button(onClick = { scope.tables.close(); openRowLink(ctx, scope, url) }, modifier = Modifier.fillMaxWidth().padding(top = Space.s)) {
				Icon(ElIcons.Globe, null, Modifier.size(Sizes.iconM))
				Spacer(Modifier.width(Space.s))
				Text(stringResource(R.string.control_open))
			}
		}
		// The actions this phone may run on the row, and the row's pages it has (a link's journal).
		val shown = Tables.rowButtons(scope.ui, w, row)
		if (shown.isNotEmpty()) {
			// While an action of this computer's runs, the others wait: a second tap would be turned away.
			val busy = scope.actions.running.running.isNotEmpty()
			EvenRows(shown, ROW_ACTIONS_IN_A_ROW) { (ra, target, action) ->
				if (target != null) {
					SheetButton(ra.label ?: scope.ui.dashboard(target)?.title ?: target) { scope.tables.close(); scope.open(target) }
				} else if (action != null) {
					val params = Params.fill(ra.params, row.f)
					val label = ra.label ?: action.name
					val kill = params["signal"].text() == "kill"
					// The tables' state alone goes with the request: it outlives this screen's scope.
					SheetButton(label, if (kill) Metric.colors.hot else Color.Unspecified, enabled = scope.actions.enabled(action, null) && !gone && !busy) {
						val tables = scope.tables
						scope.actions.launch(action, params, "$label $name", key = action.id + "/" + row.key, button = label, afterAsking = { tables.close() })
					}
				}
			}
		}
	}
}

/** One of a row's buttons, its words in the middle and whole. */
@Composable
private fun SheetButton(label: String, color: Color = Color.Unspecified, enabled: Boolean = true, onClick: () -> Unit) {
	FilledTonalButton(onClick = onClick, enabled = enabled, contentPadding = PaddingValues(horizontal = Space.m)) {
		Text(label, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.shrinkToFit())
	}
}

/** The most figures, and the most buttons, of a row's sheet side by side. */
private const val FIGURES_IN_A_ROW = 3
private const val ROW_ACTIONS_IN_A_ROW = 3
