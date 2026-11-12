package io.github.elaizes.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import io.github.elaizes.elamon.Hist
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.Points
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.data.Grid
import io.github.elaizes.elamon.data.Series
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.data.shown

/** What a dashboard's widgets get from the screen around them. */
class DashScope(
	val machineId: String,
	val ui: UiDoc,
	/** The dashboard on screen. */
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

/** Padding for a list under a see-through bar: the big title takes the top, whatever covers the bottom the bottom. */
@Composable
fun listPadding(): PaddingValues {
	val d = LocalDensity.current
	val status = with(d) { WindowInsets.statusBars.getTop(d).toDp() }
	return PaddingValues(start = Space.l, end = Space.l, top = status, bottom = bottomReserve() + Space.xxl)
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
	LazyColumn(
		Modifier.fillMaxSize(),
		state = list,
		contentPadding = listPadding(),
		verticalArrangement = Arrangement.spacedBy(Space.m),
	) {
		item(key = "header") { header() }
		if (controlOff) item(key = "control-off") { ControlOff(scope.ui.control.offReason) }
		for (row in rows) {
			val first = widgets[row[0].first]
			if (row.size == 1 && first.type == "table" && first.search == true) {
				tableItems(scope, first)
			} else {
				item(key = "row/" + first.id) { GridRow(scope, row.map { (i, s) -> widgets[i] to s }, columns) }
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

/** A row of the grid: [cells] side by side, each as wide as its span of [columns], as tall as the tallest. */
@Composable
fun GridRow(scope: DashScope, cells: List<Pair<Widget, Int>>, columns: Int) {
	val used = cells.sumOf { it.second }
	val several = cells.size > 1
	Row(
		Modifier.fillMaxWidth().then(if (several) Modifier.height(IntrinsicSize.Min) else Modifier),
		horizontalArrangement = Arrangement.spacedBy(Space.m),
	) {
		for ((w, span) in cells) {
			WidgetView(scope, w, Modifier.weight(span.toFloat()).then(if (several) Modifier.fillMaxHeight() else Modifier), columns)
		}
		if (used < columns) Spacer(Modifier.weight((columns - used).toFloat()))
	}
}

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
		else -> Notice(scope, w.label, stringResource(R.string.widget_unknown, w.type), "warning", modifier)
	}
}

// ---- colours and icons ---------------------------------------------------------------------

/** A colour the settings name (docs/config.md, "Colours"); the primary for none, or one this app doesn't know. */
@Composable
fun accent(name: String?): Color {
	val c = Metric.colors
	val s = MaterialTheme.colorScheme
	return when (name) {
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
}

/** An icon the settings name (docs/config.md, "Icons"); `overview` for one this app doesn't know. */
fun icon(name: String?): ImageVector = when (name) {
	"battery" -> ElIcons.Battery
	"bolt" -> ElIcons.Bolt
	"chip" -> ElIcons.Chip
	"clock" -> ElIcons.Clock
	"control" -> ElIcons.Control
	"eye" -> ElIcons.Eye
	"fan" -> ElIcons.Fan
	"gauge" -> ElIcons.Gauge
	"globe" -> ElIcons.Globe
	"gpu" -> ElIcons.Gpu
	"help" -> ElIcons.Help
	"laptop" -> ElIcons.Laptop
	"leaf" -> ElIcons.Leaf
	"lock" -> ElIcons.Lock
	"memory" -> ElIcons.Memory
	"monitor" -> ElIcons.Monitor
	"moon" -> ElIcons.Moon
	"music" -> ElIcons.Music
	"network" -> ElIcons.Network
	"next" -> ElIcons.Next
	"pause" -> ElIcons.Pause
	"play" -> ElIcons.Play
	"power" -> ElIcons.Power
	"previous" -> ElIcons.Previous
	"processes" -> ElIcons.Processes
	"restart" -> ElIcons.Restart
	"rocket" -> ElIcons.Rocket
	"storage" -> ElIcons.Storage
	"terminal" -> ElIcons.Terminal
	"thermometer" -> ElIcons.Thermometer
	"volume" -> ElIcons.Volume
	else -> ElIcons.Overview
}

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

/** A tile's or a card's line under it: its own text, or the state's where it holds values. */
fun captionOf(w: Widget, s: WidgetState?): String? = w.captionText ?: s?.caption?.ifEmpty { null }

@Composable
fun TileFooter(text: String) {
	Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
}

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
	Tile(modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }) {
		TileLabel(w.label.orEmpty(), icon(w.icon), accent)
		val color = levelColor(s?.level, s?.blend, figureGood(w.format))
		Figure(w, s, color ?: MaterialTheme.colorScheme.onSurface, large = true)
		if (w.spark == true) {
			val h = scope.vm.hist(w.id)
			Sparkline(
				h?.points(0) ?: NoPoints(scope.ui.service.historySeconds * MS_PER_SECOND),
				color ?: accent,
				maxValue = (s?.max ?: w.max)?.toFloat() ?: if (w.format == "percent") Thresholds.PERCENT else null,
				binary = w.format == "bytes" || w.format == "rate",
				name = w.label,
				value = { Fmt.number(it.toDouble(), w.format, w.unit) },
			)
		}
		captionOf(w, s)?.let { TileFooter(it) }
	}
}

/**
 * A widget's value large: the figure with its unit set smaller, or the word the service gave
 * instead (a mapped value, or what shows while it is missing), or a dash.
 */
@Composable
fun Figure(w: Widget, s: WidgetState?, color: Color, large: Boolean) {
	val text = s?.text
	val n = s?.number
	when {
		text != null -> Text(text, style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
		n != null -> {
			val (v, u) = Fmt.parts(n, w.format, w.unit)
			BigValue(v, u, color = color, large = large)
		}
		else -> {
			val v = Fmt.format(s?.v, w.format, w.unit)
			Text(v ?: DASH, style = if (large) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineSmall, color = color, maxLines = 2, overflow = TextOverflow.Ellipsis)
		}
	}
}

/** How full a gauge or meter is: its value between its min and its max (the state's, where it changes). */
private fun fraction(w: Widget, s: WidgetState?): Float {
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
	Tile(modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }) {
		TileLabel(w.label.orEmpty(), icon(w.icon), accent)
		Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
			RingGauge(
				fraction(w, s),
				levelColor(s?.level, s?.blend, accent) ?: accent,
				size = Gauge.tile,
				description = listOfNotNull(w.label, text).joinToString(" "),
			) {
				RollingText(text, MaterialTheme.typography.headlineMedium)
			}
		}
		captionOf(w, s)?.let { TileFooter(it) }
	}
}

/** A bar, as the battery tile had. */
@Composable
private fun MeterTile(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val color = levelColor(s?.level, s?.blend, accent) ?: accent
	Tile(modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }) {
		TileLabel(w.label.orEmpty(), icon(w.icon), accent)
		Figure(w, s, levelColor(s?.level, s?.blend, figureGood(w.format)) ?: MaterialTheme.colorScheme.onSurface, large = true)
		UsageBar(fraction(w, s), color, height = Sizes.barTile)
		captionOf(w, s)?.let { TileFooter(it) }
	}
}

/** A state as a coloured word. */
@Composable
private fun StatusTile(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	val text = s?.text ?: Fmt.format(s?.v, w.format, w.unit) ?: DASH
	Tile(modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }) {
		TileLabel(w.label.orEmpty(), icon(w.icon), accent)
		Pill(text, levelColor(s?.level, s?.blend, Metric.colors.good) ?: MaterialTheme.colorScheme.secondary, dot = true)
		captionOf(w, s)?.let { TileFooter(it) }
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
	val h: Hist? = scope.vm.hist(w.id)
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
		Tile(modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }) {
			TileLabel(w.label.orEmpty(), icon(w.icon), accent)
			if (empty != null) Text(empty, style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.secondary)
			Row(horizontalArrangement = Arrangement.spacedBy(Space.xxlPlus)) {
				series.forEachIndexed { i, ser ->
					Column {
						if (series.size > 1 || ser.label != null) Text(ser.label.orEmpty(), style = MaterialTheme.typography.labelMedium, color = colors[i], maxLines = 1)
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
			captionOf(w, s)?.let { TileFooter(it) }
		}
		return
	}
	val single = series.size == 1
	Section(
		w.label.orEmpty(),
		icon(w.icon),
		modifier,
		accent = accent,
		trailing = if (single) {
			{ now.firstOrNull()?.let { RollingText(Fmt.number(it, first.format, first.unit), MaterialTheme.typography.titleLarge, color = colors[0]) } }
		} else null,
		onClick = w.open?.let { d -> { scope.open(d) } },
	) {
		if (empty != null) Text(empty, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		LineChart(
			chart,
			maxValue = maxValue,
			minScale = floor,
			binary = binary,
			height = if (w.height == "large") ChartStyle.large else ChartStyle.detail,
			label = { v -> Fmt.number(v.toDouble(), first.format, first.unit) },
		)
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
@OptIn(ExperimentalLayoutApi::class)
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
	Section(w.label.orEmpty(), icon(w.icon), modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }) {
		CoreBars(values.map { ((it ?: 0.0) / max).toFloat() }, colors, height = ChartStyle.coresDetail, description = description)
		FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s), verticalArrangement = Arrangement.spacedBy(Space.s), maxItemsInEachRow = BAR_CELLS) {
			values.forEachIndexed { i, v ->
				Column(
					Modifier.weight(1f).clip(RoundedCornerShape(Radius.mPlus)).background(MaterialTheme.colorScheme.surfaceContainerHigh).padding(Space.sPlus),
				) {
					Text(s?.labels?.getOrNull(i) ?: i.toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
					Text(v?.let { Fmt.number(it, w.format, w.unit) } ?: DASH, style = MaterialTheme.typography.titleSmall, color = colors[i])
				}
			}
			// The last row's cells as wide as the others.
			repeat((BAR_CELLS - values.size % BAR_CELLS) % BAR_CELLS) { Spacer(Modifier.weight(1f)) }
		}
	}
}

/** Bars' figures to a row. */
private const val BAR_CELLS = 4

/** Rows of a label and a value, as the details cards had; a row with nothing to show is left out. */
@Composable
private fun ListWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val accent = accent(w.color)
	Section(w.label.orEmpty(), icon(w.icon), modifier, accent = accent, onClick = w.open?.let { d -> { scope.open(d) } }) {
		w.items.orEmpty().forEachIndexed { i, item ->
			val st = s?.items?.getOrNull(i)
			val text = st?.text ?: Fmt.format(st?.v, item.format, item.unit) ?: return@forEachIndexed
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
			modifier = modifier.padding(horizontal = Space.xl, vertical = Space.xs),
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
			CompositionLocalProvider(LocalInGroup provides true) {
				Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
					for (row in rows) GridRow(scope, row.map { (i, span) -> children[i] to span }, inner)
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
	)
}

/** While a value is missing. */
const val DASH = "—"

private const val MS_PER_SECOND = 1000L
