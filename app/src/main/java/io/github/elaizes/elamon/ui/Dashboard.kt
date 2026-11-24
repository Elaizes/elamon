package io.github.elaizes.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.vectorResource
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import io.github.elaizes.elamon.Hist
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.Points
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.rangeChips
import io.github.elaizes.elamon.data.Grid
import io.github.elaizes.elamon.data.LogFilter
import io.github.elaizes.elamon.data.Series
import io.github.elaizes.elamon.data.UniqueKeys
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.data.shown

/** What a dashboard's widgets get from the screen around them. */
class DashScope(
	val machineId: String,
	val ui: UiDoc,
	val dashboard: String,
	/** Its widgets' states, by id, from the last data line; empty until the first. */
	val states: Map<String, WidgetState>,
	val vm: MachineViewModel,
	val machine: MachineState,
	/** The computer's name as shown, for what asks before acting on it. */
	val name: String,
	/** Opens a dashboard as a page over this one. */
	val open: (String) -> Unit,
	val actions: ActionLauncher,
	val folds: Folds,
	val tables: TableStates,
	/** The tables in another order than their own, as the stream was asked: the field, by the table's id. */
	val sorts: Map<String, String>,
	/** The logs filtered otherwise than by their own level, as the stream was asked, by the log's id. */
	val logs: Map<String, LogFilter>,
	/** The points of a graph or spark, by its id: the stream's, or (the editor's preview) the preview's own. */
	val hist: (String) -> Hist? = vm::hist,
) {
	fun state(w: Widget): WidgetState? = states[w.id]
}

/** Which foldable groups and tables of a computer are folded: as each started, until folded or opened, then kept. */
class Folds(private val settings: Settings, private val machine: String) {
	private val changed = mutableStateMapOf<String, Boolean>()

	fun folded(w: Widget): Boolean = w.collapsed != null && (changed[w.id] ?: settings.folded(machine, w.id, w.collapsed))

	fun toggle(w: Widget) {
		val f = !folded(w)
		changed[w.id] = f
		settings.setFolded(machine, w.id, f)
	}
}

/** Padding for a list under a see-through bar: the big title takes the top, whatever covers the bottom the bottom; both sides alike. */
@Composable
fun listPadding(): PaddingValues {
	val d = LocalDensity.current
	val status = with(d) { WindowInsets.statusBars.getTop(d).toDp() }
	val side = Space.screen + sideInset()
	return PaddingValues(start = side, end = side, top = status, bottom = bottomReserve() + Space.xl)
}

/** The grid's columns on this screen: 4, or 2 where it is narrow. */
@Composable
fun gridColumns(): Int {
	val px = LocalWindowInfo.current.containerSize.width
	val dp = with(LocalDensity.current) { px.toDp().value.toInt() }
	return Grid.columns(dp)
}

/**
 * A dashboard as the service describes it: its widgets in a grid of [gridColumns], in order, each
 * as wide as its span, a table with a search field taking rows of its own in the list.
 */
@Composable
fun DashboardList(scope: DashScope, list: LazyListState, header: @Composable () -> Unit) {
	val dash = scope.ui.dashboard(scope.dashboard)
	val columns = gridColumns()
	val widgets = dash?.widgets.orEmpty().filter { shown(it, scope.state(it)) }
	val rows = Grid.place(widgets.map { it.span }, columns)
	val controlOff = !scope.ui.control.enabled && widgets.any(::acts)
	// A table with a search field, alone in its row, gives its rows to the list.
	val tables = rows.map { row ->
		widgets[row.cells[0].first].takeIf { row.cells.size == 1 && it.type == "table" && it.search == true }?.let { key(it.id) { listTable(scope, it) } }
	}
	CompositionLocalProvider(LocalCellWidth provides contentWidth()) {
		LazyColumn(
			Modifier.fillMaxSize(),
			state = list,
			contentPadding = listPadding(),
			verticalArrangement = Arrangement.spacedBy(Space.gap),
		) {
			// Two widgets with one id, or two rows with one key, from the service are two items here too.
			val keys = UniqueKeys()
			item(key = keys.of("header")) { header() }
			if (controlOff) item(key = keys.of("control-off")) { ControlOff(scope.ui.control.offReason) }
			rows.forEachIndexed { i, row ->
				val first = widgets[row.cells[0].first]
				val table = tables[i]
				if (table != null) {
					tableItems(scope, table, keys)
				} else {
					item(key = keys.of("row/" + first.id)) { GridRow(scope, widgets, row, columns) }
				}
			}
		}
	}
}

/**
 * Whether [w], or one in it, is a control that runs an action: drawn disabled while control is off,
 * under a card that says so. A table's row actions, in its rows' sheets, don't bring the card, as
 * 0.5 showed it on the Control tab only.
 */
private fun acts(w: Widget): Boolean =
	w.action != null || w.type == "media" || w.widgets.orEmpty().any(::acts)

/** A row of the grid: those of [widgets] that [line] has, side by side, as tall as the tallest; [gap] apart. */
@Composable
fun GridRow(scope: DashScope, widgets: List<Widget>, line: Grid.Line, columns: Int, gap: Dp = Space.gap) {
	val width = LocalCellWidth.current
	// As tall as the tallest asks each how tall it would be, which a log's own list can't say.
	GridLine(line, gap = gap, even = line.cells.none { (i, _) -> hasLog(widgets[i]) }) {
		for ((i, span) in line.cells) {
			CompositionLocalProvider(LocalCellWidth provides cellWidth(line, span, width, gap)) { WidgetView(scope, widgets[i], Modifier, columns) }
		}
	}
}

internal fun hasLog(w: Widget): Boolean = w.type == "log" || w.widgets.orEmpty().any(::hasLog)

/** One widget, by its type; one this app doesn't know says so. */
@Composable
fun WidgetView(scope: DashScope, w: Widget, modifier: Modifier, columns: Int) {
	when (w.type) {
		"value" -> ValueTile(scope, w, modifier)
		"gauge" -> GaugeTile(scope, w, modifier)
		"meter" -> MeterTile(scope, w, modifier)
		"status" -> StatusTile(scope, w, modifier)
		"graph" -> GraphWidget(scope, w, modifier)
		"bars" -> BarsWidget(scope, w, modifier)
		"list" -> ListWidget(scope, w, modifier)
		"table" -> TableWidget(scope, w, modifier)
		"button" -> ButtonWidget(scope, w, modifier)
		"toggle" -> ToggleWidget(scope, w, modifier)
		"select" -> SelectWidget(scope, w, modifier)
		"slider" -> SliderWidget(scope, w, modifier)
		"text" -> TextWidget(scope, w, modifier)
		"group" -> GroupWidget(scope, w, modifier, columns)
		"media" -> MediaWidget(scope, w, modifier)
		"timeline" -> TimelineWidget(scope, w, modifier)
		"log" -> LogWidget(scope, w, modifier)
		else -> Notice(scope, w.label, stringResource(R.string.widget_unknown, w.type), "warning", modifier)
	}
}

// ---- colours and icons ---------------------------------------------------------------------

/** A colour the settings name (docs/config.md, "Colours"); the primary for none, or one this app doesn't know. */
@Composable
fun accent(name: String?): Color = accentOf(name, MaterialTheme.colorScheme, Metric.colors)

/** [accent] in the scheme [s] and the metric colours [c]: the home-screen widget's, outside the app's theme. */
fun accentOf(name: String?, s: ColorScheme, c: MetricColors): Color = when (name) {
	"cpu" -> c.cpu
	"memory" -> c.memory
	"swap" -> c.swap
	"gpu" -> c.gpu
	"rx" -> c.rx
	"tx" -> c.tx
	"read" -> c.read
	"write" -> c.write
	"battery" -> c.battery
	"good" -> c.good
	"warn" -> c.warn
	"hot" -> c.hot
	"secondary" -> s.secondary
	"tertiary" -> s.tertiary
	else -> s.primary
}

/** An icon the settings name (docs/config.md, "Icons"); `overview` for one this app doesn't know. */
@Composable fun icon(name: String?): ImageVector = ImageVector.vectorResource(iconRes(name))

/** The service's level as a colour: [good] while normal, turning amber, then red. Null without a level. */
@Composable
fun levelColor(level: String?, blend: Double?, good: Color): Color? {
	val c = Metric.colors
	return levelOf(level, blend)?.color(good, c.warn, c.hot)
}

/**
 * What a figure's normal level looks like: green for a temperature, as 0.5 drew them, the text's
 * own colour for anything else (a load or a size is no better for being low).
 */
@Composable
fun figureGood(format: String?): Color = if (format == "temp") Metric.colors.good else MaterialTheme.colorScheme.onSurface

/**
 * A tile's or a card's line under it: its own text, or the state's where it holds values; before the
 * first state, one of those is an empty line, so what is under it stays where it is as it comes.
 */
fun captionOf(w: Widget, s: WidgetState?): String? = w.captionText ?: if (s == null && w.caption != null) NO_WORDS else s?.caption?.ifEmpty { null }

/** A line with nothing on it yet. */
private const val NO_WORDS = "\u00a0"

/** No points: a graph before its first line. */
private class NoPoints(override val windowMs: Long) : Points {
	override val size = 0
	override fun t(i: Int) = 0L
	override fun v(i: Int) = Float.NaN
	override val gapMs = 0L
}

// ---- figures ------------------------------------------------------------------------------

/** A number or text, large; with `spark`, a small graph of it under it. */
@Composable
private fun ValueTile(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	WidgetTile(scope, w, accent, modifier, captionOf(w, s)) {
		val color = levelColor(s?.level, s?.blend, figureGood(w.format))
		Figure(w, s, color ?: MaterialTheme.colorScheme.onSurface)
		if (w.spark == true) {
			val h = scope.hist(w.id)
			Sparkline(
				h?.points(0) ?: NoPoints(scope.ui.service.historySeconds * MS_PER_SECOND),
				color ?: accent,
				maxValue = (s?.max ?: w.max)?.toFloat() ?: if (w.format == "percent") Thresholds.PERCENT else null,
				binary = w.format == "bytes" || w.format == "rate",
				name = w.label,
				value = { Fmt.number(it.toDouble(), w.format, w.unit) },
			)
		}
	}
}

/**
 * A widget's value large: the figure with its unit set smaller, or the word the service gave
 * instead (a mapped value, or what shows while it is missing), or a dash; all on one line.
 */
@Composable
fun Figure(w: Widget, s: WidgetState?, color: Color) {
	val text = s?.text
	val n = s?.number
	when {
		n != null && text == null -> {
			val (v, u) = Fmt.parts(n, w.format, w.unit)
			BigValue(v, u, Modifier.figureLine(), color = color)
		}
		else -> {
			val words = text ?: Fmt.format(s?.v, w.format, w.unit)
			Text(
				words ?: DASH,
				style = if (text == null) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineSmall,
				color = if (text == null) color else MaterialTheme.colorScheme.secondary,
				maxLines = 1,
				overflow = TextOverflow.Ellipsis,
				modifier = Modifier.figureLine().shrinkToFit(),
			)
		}
	}
}

/** How full a gauge or meter is: its value between its min and its max (the state's, where it changes). */
internal fun fraction(w: Widget, s: WidgetState?): Float {
	val v = s?.number ?: return 0f
	val min = w.min ?: 0.0
	val max = s.max ?: w.max ?: if (w.format == "percent") Thresholds.PERCENT.toDouble() else return 0f
	if (max <= min) return 0f
	return ((v - min) / (max - min)).toFloat().coerceIn(0f, 1f)
}

/** A ring, as the CPU and memory tiles had. */
@Composable
private fun GaugeTile(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val text = s?.text ?: Fmt.format(s?.v, w.format, w.unit) ?: DASH
	// Its ring as large as its tile has room for; a small one thinner, and its figure smaller, as it is.
	val size = minOf(Gauge.tile, LocalCellWidth.current - cardPadding() * 2)
	val share = size / Gauge.tile
	WidgetTile(scope, w, accent, modifier, captionOf(w, s)) {
		Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
			RingGauge(
				fraction(w, s),
				levelColor(s?.level, s?.blend, accent) ?: accent,
				size = size,
				thickness = Gauge.thickness * share,
				description = listOfNotNull(w.label, text).joinToString(" "),
			) {
				val style = MaterialTheme.typography.headlineMedium
				RollingText(text, style.copy(fontSize = style.fontSize * share), Modifier.shrinkToFit())
			}
		}
	}
}

/** A bar, as the battery tile had. */
@Composable
private fun MeterTile(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val color = levelColor(s?.level, s?.blend, accent) ?: accent
	WidgetTile(scope, w, accent, modifier, captionOf(w, s)) {
		Figure(w, s, levelColor(s?.level, s?.blend, figureGood(w.format)) ?: MaterialTheme.colorScheme.onSurface)
		UsageBar(fraction(w, s), color, height = Sizes.barTile)
	}
}

/** A tile for [w]: its icon and label over [content], [caption] its last line; a button when [w] opens a page. */
@Composable
fun WidgetTile(scope: DashScope, w: Widget, accent: Color, modifier: Modifier, caption: String?, content: @Composable ColumnScope.() -> Unit) {
	Tile(modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }, caption = caption) {
		TileLabel(w.label.orEmpty(), icon(w.icon), accent)
		content()
	}
}

/** A card for [w]: its icon and label as the title, a button when [w] opens a page. */
@Composable
fun WidgetSection(
	scope: DashScope,
	w: Widget,
	accent: Color,
	modifier: Modifier,
	trailing: (@Composable () -> Unit)? = null,
	onTitleClick: (() -> Unit)? = null,
	content: @Composable ColumnScope.() -> Unit,
) {
	Section(w.label.orEmpty(), icon(w.icon), modifier, accent = accent, trailing = trailing, onClick = w.open?.let { d -> { scope.open(d) } }, onTitleClick = onTitleClick, content = content)
}

/** A state as a coloured word. */
@Composable
private fun StatusTile(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val text = s?.text ?: Fmt.format(s?.v, w.format, w.unit) ?: DASH
	WidgetTile(scope, w, accent, modifier, captionOf(w, s)) {
		Pill(text, levelColor(s?.level, s?.blend, Metric.colors.good) ?: MaterialTheme.colorScheme.secondary, dot = true)
	}
}

// ---- graphs -------------------------------------------------------------------------------

/**
 * Lines over time with the newest values: small, as a tile without a grid (the overview's network);
 * medium or large, as a card with a grid that a finger can read (a detail page's).
 */
@Composable
private fun GraphWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val series = w.series?.takeIf { it.isNotEmpty() } ?: listOf(Series())
	val h: Hist? = scope.hist(w.id)
	val window = (w.range?.takeIf { it > 0 } ?: scope.ui.service.historySeconds) * MS_PER_SECOND
	val colors = series.map { accent(it.color ?: w.color) }
	val now = s?.numbers.orEmpty()
	val chart = series.mapIndexed { i, ser ->
		ChartSeries(
			h?.points(i) ?: NoPoints(window),
			colors[i],
			name = ser.label,
			value = { v -> Fmt.number(v.toDouble(), ser.format, ser.unit) },
			format = { v -> listOfNotNull(ser.label?.takeIf { series.size > 1 }, Fmt.number(v.toDouble(), ser.format, ser.unit)).joinToString(" ") },
		)
	}
	val first = series.first()
	val binary = first.format == "bytes" || first.format == "rate"
	val maxValue = (s?.max ?: w.max)?.toFloat()
	val floor = w.floor?.toFloat() ?: 1f
	val empty = s?.text
	if (w.height == "small") {
		WidgetTile(scope, w, accent, modifier, captionOf(w, s)) {
			if (empty != null) Text(empty, style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.secondary)
			// Each in a share of its own, so a figure that grows a digit doesn't move the next one.
			Row(horizontalArrangement = Arrangement.spacedBy(Space.gap)) {
				series.forEachIndexed { i, ser ->
					Column(Modifier.weight(1f)) {
						if (series.size > 1 || ser.label != null) Text(ser.label.orEmpty(), style = MaterialTheme.typography.labelMedium, color = colors[i], maxLines = 1, overflow = TextOverflow.Ellipsis)
						val v = now.getOrNull(i)
						if (v != null) {
							val (n, u) = Fmt.parts(v, ser.format, ser.unit)
							BigValue(n, u, large = false)
						} else {
							Text(DASH, style = MaterialTheme.typography.headlineSmall)
						}
					}
				}
			}
			LineChart(chart, maxValue = maxValue, minScale = floor, binary = binary, height = ChartStyle.tile, grid = false, interactive = false)
			}
		return
	}
	val single = series.size == 1
	// Over a long range, the computer's history of it, the stream's points joining it; live otherwise,
	// and where the computer couldn't give it.
	val chips = rangeChips(window, w.ranges)
	val view = scope.machine.ranges[w.id]?.takeIf { v -> chips.any { it.range == v.range } }
	val long = view?.takeUnless { it.failed }
	val lh = if (long != null) scope.vm.longHist(w.id) else null
	val shown = if (long == null) chart else chart.mapIndexed { i, c ->
		ChartSeries(lh?.points(i) ?: NoPoints(long.range.ms ?: window), c.color, name = c.name, value = c.value, format = c.format)
	}
	WidgetSection(
		scope,
		w,
		accent,
		modifier,
		trailing = if (single) {
			{ now.firstOrNull()?.let { RollingText(Fmt.number(it, first.format, first.unit), MaterialTheme.typography.titleLarge, color = colors[0]) } }
		} else null,
	) {
		if (chips.isNotEmpty()) RangeChips(chips, long?.range) { scope.vm.showRange(w, it) }
		if (empty != null) Text(empty, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		RangeNote(view, chips, has = lh != null)
		LineChart(
			shown,
			maxValue = maxValue,
			minScale = floor,
			binary = binary,
			height = if (w.height == "large") ChartStyle.large else ChartStyle.detail,
			label = { v -> Fmt.number(v.toDouble(), first.format, first.unit) },
			span = long?.let { v -> rangeLabel(chips.first { it.range == v.range }) },
			moment = if (long != null) ::moment else null,
		)
		if (lh != null && lh.size > 0) TimeAxis(lh.lastTime - lh.windowMs, lh.lastTime, inset = ChartStyle.halo)
		if (!single) {
			@OptIn(ExperimentalLayoutApi::class)
			FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.l), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
				series.forEachIndexed { i, ser ->
					val v = now.getOrNull(i)?.let { Fmt.number(it, ser.format, ser.unit) }
					Legend(colors[i], listOfNotNull(ser.label, v).joinToString(" "))
				}
			}
		}
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
	}
}

/** One bar per item, with each item's figure under them, as the CPU page's cores. */
@Composable
private fun BarsWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val values = s?.numbers.orEmpty()
	val max = (w.max ?: s?.max ?: Thresholds.PERCENT.toDouble()).takeIf { it > 0 } ?: 1.0
	val colors = values.indices.map { i -> levelColor(s?.levels?.getOrNull(i), null, accent) ?: accent }
	val res = resources()
	val description = if (values.isEmpty()) null else res.getQuantityString(
		R.plurals.a11y_bars, values.size, w.label.orEmpty(), values.size,
		Fmt.number(values.filterNotNull().average(), w.format, w.unit), Fmt.number(values.filterNotNull().maxOrNull() ?: 0.0, w.format, w.unit),
	)
	WidgetSection(scope, w, accent, modifier) {
		CoreBars(values.map { ((it ?: 0.0) / max).toFloat() }, colors, height = ChartStyle.coresDetail, description = description)
		EvenRows(values.indices.toList(), BAR_CELLS) { i ->
			Column(Modifier.clip(InnerShape).background(cardColor()).padding(Space.s)) {
				Text(s?.labels?.getOrNull(i) ?: i.toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text(values[i]?.let { Fmt.number(it, w.format, w.unit) } ?: DASH, style = MaterialTheme.typography.titleSmall, color = colors[i], maxLines = 1, modifier = Modifier.shrinkToFit())
			}
		}
	}
}

/** The most figures of a bars widget to a row. */
private const val BAR_CELLS = 4

/** Rows of a label and a value, as the details cards had; a row with nothing to show is left out. */
@Composable
private fun ListWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	WidgetSection(scope, w, accent, modifier) {
		w.items.orEmpty().forEachIndexed { i, item ->
			val st = s?.items?.getOrNull(i)
			// Before the first state every row keeps its place.
			val text = st?.text ?: Fmt.format(st?.v, item.format, item.unit) ?: if (s == null) DASH else return@forEachIndexed
			StatRow(item.label.orEmpty(), text, valueColor = levelColor(st?.level, null, figureGood(item.format)) ?: MaterialTheme.colorScheme.onSurface)
		}
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
	}
}

// ---- words and groups --------------------------------------------------------------------------

/** Words; at level warning or critical, a notice. */
@Composable
private fun TextWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val text = s?.text ?: w.text ?: return
	val level = s?.level ?: w.level
	if (level == "warning" || level == "critical") {
		Notice(scope, w.label, text, level, modifier)
	} else {
		Text(
			text,
			style = MaterialTheme.typography.bodyMedium,
			color = MaterialTheme.colorScheme.onSurfaceVariant,
			modifier = modifier.fillMaxWidth().padding(horizontal = Space.card, vertical = Space.xs),
		)
	}
}

/** A notice in a card: amber for a warning, red when critical. */
@Composable
private fun Notice(scope: DashScope, title: String?, text: String, level: String, modifier: Modifier) {
	val color = if (level == "critical") Metric.colors.hot else Metric.colors.warn
	Section(title ?: stringResource(R.string.widget_notice), Icons.Default.Warning, modifier, accent = color) {
		Text(text, style = MaterialTheme.typography.bodyMedium)
	}
}

/** A card around widgets, in a grid of its own; one that folds says so by its title. */
@Composable
private fun GroupWidget(scope: DashScope, w: Widget, modifier: Modifier, columns: Int) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val foldable = w.collapsed != null
	val folded = scope.folds.folded(w)
	Section(
		w.label.orEmpty(),
		icon(w.icon),
		modifier,
		accent = accent,
		trailing = if (foldable) ({ FoldMark(folded) }) else null,
		onTitleClick = if (foldable) ({ scope.folds.toggle(w) }) else null,
	) {
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
		if (!folded) {
			// Its widgets fill a grid as wide as the group, their spans halved the same way.
			val inner = Grid.span(w.span, columns).coerceAtLeast(1)
			val children = w.widgets.orEmpty().filter { shown(it, scope.state(it)) }
			val rows = Grid.place(children.map { it.span }, inner)
			CompositionLocalProvider(LocalCellWidth provides LocalCellWidth.current - Space.card * 2) {
				Column(verticalArrangement = Arrangement.spacedBy(Space.inner)) {
					for (row in rows) GridRow(scope, children, row, inner, gap = Space.inner)
				}
			}
		}
	}
}

/** Whether a card is folded, by its title. */
@Composable
fun FoldMark(folded: Boolean) {
	Icon(
		if (folded) Icons.Default.KeyboardArrowDown else Icons.Default.KeyboardArrowUp,
		stringResource(if (folded) R.string.widget_unfold else R.string.widget_fold),
		tint = MaterialTheme.colorScheme.onSurfaceVariant,
		modifier = Modifier.optical(Sizes.foldEmpty),
	)
}

/** While a value is missing. */
const val DASH = "—"

private const val MS_PER_SECOND = 1000L
