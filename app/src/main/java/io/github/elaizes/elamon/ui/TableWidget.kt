package io.github.elaizes.elamon.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Column as TableColumn
import io.github.elaizes.elamon.data.Params
import io.github.elaizes.elamon.data.Row as TableRow
import io.github.elaizes.elamon.data.Tables
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.linkUrl
import io.github.elaizes.elamon.data.text

/**
 * What the tables of a screen keep while it lives: each one's search, the order asked for, and
 * the row whose sheet is open (with its last readings, should it leave the list).
 */
class TableStates {
	private val queries = mutableStateMapOf<String, String>()
	private val sorts = mutableStateMapOf<String, String>()

	/** The table and the row whose sheet is open. */
	var sheet by mutableStateOf<Pair<String, TableRow>?>(null)
		private set

	fun query(table: String): String = queries[table].orEmpty()

	fun setQuery(table: String, q: String) {
		queries[table] = q
	}

	/** The order asked for the table; null while it is the one the service sent. */
	fun sort(table: String): String? = sorts[table]

	fun setSort(vm: MachineViewModel, table: String, field: String) {
		sorts[table] = field
		vm.sort(table, field)
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
	val searched = if (w.search == true) Tables.search(rows, w.columns.orEmpty(), scope.tables.query(w.id)) else rows
	val asked = scope.tables.sort(w.id)
	return if (asked != null && asked != w.sort?.field) Tables.sort(searched, asked, desc = true) else searched
}

/** The column whose text takes the room left: the one that grows, or the first. */
private fun growColumn(w: Widget): TableColumn? = w.columns.orEmpty().let { c -> c.firstOrNull { it.grow } ?: c.firstOrNull() }

/** A cell's text: its value by its column's format and unit; a dash while it is missing. */
private fun cell(row: TableRow, c: TableColumn): String = Fmt.format(row.f[c.field], c.format, c.unit, c.field) ?: DASH

/** Rows of processes and links get a letter avatar, as their lists had in 0.5. */
private fun avatars(w: Widget) = w.source == "processes" || w.source == "links"

/** How wide a column that doesn't grow is, by what it holds, so the rows line up. */
private fun cellWidth(c: TableColumn): Dp = when (c.format) {
	"percent", "temp" -> CELL_NARROW
	"integer", "number", "load", "bool" -> CELL_NUMBER
	"time" -> CELL_TIME
	else -> CELL_WIDE
}

private val CELL_NARROW = 56.dp
private val CELL_NUMBER = 64.dp
private val CELL_WIDE = 88.dp
private val CELL_TIME = 120.dp

/** An expanded field longer than this is set as a block of its own, like a command line. */
private const val LONG_FIELD = 40

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
	val body: @Composable () -> Unit = {
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
		if (!folded) {
			if (w.search == true) TableControls(scope, w, s?.total, rows.size)
			TableHead(w)
			if (rows.isEmpty()) {
				Text(stringResource(if (s == null) R.string.loading else R.string.table_empty), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
			for (r in rows) TableRowView(scope, w, r, rows, card = false)
		}
	}
	if (open != null && scope.dashboard in scope.ui.tabs) {
		Tile(modifier, accent = accent, onClick = open) {
			TileLabel(w.label.orEmpty(), icon(w.icon), accent)
			body()
		}
	} else {
		Section(
			w.label.orEmpty(),
			icon(w.icon),
			modifier,
			accent = accent,
			trailing = if (foldable) ({ FoldMark(folded) }) else null,
			onClick = open,
			onTitleClick = if (foldable) ({ scope.folds.toggle(w) }) else null,
		) { body() }
	}
}

/**
 * A table with a search field (the processes, the activity log): its search and orders, then its
 * rows as the list's own, each a card, as 0.5's Processes tab.
 */
fun LazyListScope.tableItems(scope: DashScope, w: Widget) {
	val s = scope.state(w)
	val rows = tableRows(scope, w)
	item(key = "table/${w.id}") {
		Column(verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
			w.label?.let { Text(it, style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(start = Space.xs)) }
			captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
			TableControls(scope, w, s?.total, rows.size)
			if (s != null && rows.isEmpty()) {
				Text(stringResource(R.string.table_empty), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = Space.xs))
			}
		}
	}
	items(rows, key = { "table/${w.id}/${it.key}" }) { r -> TableRowView(scope, w, r, rows, card = true) }
}

/** The search field, the orders the service offers, and how many rows there are. */
@Composable
private fun TableControls(scope: DashScope, w: Widget, total: Int?, shown: Int) {
	val query = scope.tables.query(w.id)
	Column(verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
		OutlinedTextField(
			value = query,
			onValueChange = { scope.tables.setQuery(w.id, it) },
			modifier = Modifier.fillMaxWidth(),
			singleLine = true,
			shape = RoundedCornerShape(Radius.field),
			placeholder = { Text(stringResource(R.string.table_search)) },
			leadingIcon = { Icon(Icons.Default.Search, null) },
			trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { scope.tables.setQuery(w.id, "") }) { Icon(Icons.Default.Clear, stringResource(R.string.processes_clear)) } },
			colors = OutlinedTextFieldDefaults.colors(
				unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
				focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
				unfocusedBorderColor = MaterialTheme.colorScheme.surfaceContainerLow,
			),
		)
		val sorts = (listOfNotNull(w.sort?.field) + w.sorts.orEmpty()).distinct()
		if (sorts.size > 1) {
			val current = scope.tables.sort(w.id) ?: w.sort?.field
			Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
				for (f in sorts) {
					FilterChip(
						selected = f == current,
						onClick = { if (f != current) scope.tables.setSort(scope.vm, w.id, f) },
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
				modifier = Modifier.padding(start = Space.xs),
			)
		}
	}
}

/** What a field is called: its column's label, else its expanded field's, else its name. */
private fun fieldLabel(w: Widget, field: String): String =
	w.columns?.find { it.field == field }?.label ?: w.expand?.find { it.field == field }?.label ?: field.replaceFirstChar { it.uppercase() }

/** The columns' labels over the rows, where any has one. */
@Composable
private fun TableHead(w: Widget) {
	val cols = w.columns.orEmpty()
	if (cols.none { it.label != null }) return
	val grow = growColumn(w)
	val bar = cols.firstOrNull { it.bar != null }
	Row {
		if (avatars(w)) Spacer(Modifier.width(Sizes.avatarS + Space.m))
		Text(grow?.label.orEmpty(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f), maxLines = 1)
		for (c in cols.filter { it != grow && it != bar }) {
			Text(c.label.orEmpty(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(cellWidth(c)), textAlign = TextAlign.End, maxLines = 1)
		}
		if (bar != null) Text(bar.label.orEmpty(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(cellWidth(bar)), textAlign = TextAlign.End, maxLines = 1)
	}
}

/**
 * One row: its growing column's text, the other columns' figures, and the bar column's figure in
 * the row's level's colour with its bar under the row ([card]: a list's own card, the bar under
 * the figure). A tap opens what it links to, or its sheet; a long press, the sheet.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TableRowView(scope: DashScope, w: Widget, row: TableRow, rows: List<TableRow>, card: Boolean) {
	val ctx = LocalContext.current
	val haptics = LocalHapticFeedback.current
	val prefs = LocalPrefs.current
	val cols = w.columns.orEmpty()
	val grow = growColumn(w)
	val bar = cols.firstOrNull { it.bar != null }
	val others = cols.filter { it != grow && it != bar }
	val hasSheet = !w.expand.isNullOrEmpty() || !w.rowActions.isNullOrEmpty()
	val url = w.rowOpen?.let { row.f[it].text() }
	val accent = accent(w.color)
	val barColor = bar?.let { levelColor(row.level, row.blend, if (it.format == "temp") Metric.colors.good else accent) ?: accent }
	val fraction = bar?.let { Tables.barFraction(row, it, rows) }
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
		Column(verticalArrangement = Arrangement.spacedBy(Space.xsPlus)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				if (avatars(w)) {
					LetterAvatar(name, if (card) Sizes.avatar else Sizes.avatarS)
					Spacer(Modifier.width(Space.m))
				}
				Text(name, style = if (card) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
				for (c in others) {
					Text(cell(row, c), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(cellWidth(c)), textAlign = TextAlign.End, maxLines = 1, overflow = TextOverflow.Ellipsis)
				}
				if (bar != null && barColor != null) {
					Column(Modifier.width(cellWidth(bar)), horizontalAlignment = Alignment.End) {
						Text(cell(row, bar), style = MaterialTheme.typography.titleSmall, color = barColor, maxLines = 1)
						if (card) {
							Spacer(Modifier.size(Space.xs))
							UsageBar(fraction ?: 0f, barColor, height = Sizes.barHair)
						}
					}
				}
			}
			if (!card && bar != null && barColor != null) UsageBar(fraction ?: 0f, barColor, height = Sizes.barThin)
		}
	}
	if (card) {
		Surface(Modifier.fillMaxWidth().padding(vertical = Space.xsPlus / 2), shape = RoundedCornerShape(Radius.xl), color = MaterialTheme.colorScheme.surfaceContainerLow) {
			Column(click.padding(horizontal = Space.mPlus, vertical = Space.m)) { content() }
		}
	} else {
		Column(Modifier.fillMaxWidth().then(click).padding(vertical = Space.xxs)) { content() }
	}
}

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
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun TableSheet(scope: DashScope) {
	val (table, last) = scope.tables.sheet ?: return
	val w = scope.ui.dashboards.firstNotNullOfOrNull { d -> io.github.elaizes.elamon.allWidgets(d.widgets).find { it.id == table } } ?: return
	val state = scope.states[table]
	val live = state?.rows?.find { it.key == last.key }
	val row = live ?: last
	val gone = state != null && live == null
	val ctx = LocalContext.current
	val cols = w.columns.orEmpty()
	val grow = growColumn(w)
	val name = grow?.let { cell(row, it) }.orEmpty()
	val accent = accent(w.color)
	ModalBottomSheet(onDismissRequest = { scope.tables.close() }, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(start = Space.xxl, end = Space.xxl, bottom = Space.xxxl), verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				if (avatars(w)) {
					LetterAvatar(name, Sizes.avatarSheet)
					Spacer(Modifier.width(Space.mPlus))
				}
				Text(name, style = MaterialTheme.typography.headlineSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
			}
			if (gone) Text(stringResource(R.string.table_row_gone), style = MaterialTheme.typography.bodySmall, color = Metric.colors.warn)
			val figures = cols.filter { it != grow }
			if (figures.isNotEmpty()) {
				FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.xxl), verticalArrangement = Arrangement.spacedBy(Space.s), modifier = Modifier.padding(vertical = Space.xsPlus)) {
					for (c in figures) {
						val color = if (c.bar != null) levelColor(row.level, row.blend, if (c.format == "temp") Metric.colors.good else accent) ?: accent else MaterialTheme.colorScheme.onSurface
						BigStat(c.label ?: fieldLabel(w, c.field), cell(row, c), color = color)
					}
				}
			}
			for (e in w.expand.orEmpty()) {
				val v = Fmt.format(row.f[e.field], e.format, e.unit, e.field)?.ifBlank { null } ?: continue
				val label = e.label ?: e.field
				if (v.length > LONG_FIELD) {
					Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
					SelectionContainer { Text(v, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace) }
				} else {
					StatRow(label, v)
				}
			}
			val url = w.rowOpen?.let { row.f[it].text() }
			if (url != null) {
				Button(onClick = { scope.tables.close(); openRowLink(ctx, scope, url) }, modifier = Modifier.fillMaxWidth()) {
					Icon(ElIcons.Globe, null, Modifier.size(Sizes.iconM))
					Spacer(Modifier.width(Space.s))
					Text(stringResource(R.string.control_open))
				}
			}
			val actions = w.rowActions.orEmpty().mapNotNull { ra -> scope.actions.action(ra.action)?.let { ra to it } }
			if (actions.isNotEmpty()) {
				FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sPlus), verticalArrangement = Arrangement.spacedBy(Space.s), modifier = Modifier.padding(top = Space.s), maxItemsInEachRow = ROW_ACTIONS_IN_A_ROW) {
					for ((ra, action) in actions) {
						val params = Params.fill(ra.params, row.f)
						val label = ra.label ?: action.name
						val kill = params["signal"].text() == "kill"
						FilledTonalButton(
							onClick = { scope.actions.launch(action, params, "$label $name", key = action.id + "/" + row.key, button = label, afterAsking = { scope.tables.close() }) },
							enabled = scope.actions.enabled(action, null) && !gone,
							modifier = Modifier.weight(1f),
						) {
							Text(label, color = if (kill) Metric.colors.hot else Color.Unspecified, maxLines = 1, overflow = TextOverflow.Ellipsis)
						}
					}
				}
			}
		}
	}
}

private const val ROW_ACTIONS_IN_A_ROW = 3
