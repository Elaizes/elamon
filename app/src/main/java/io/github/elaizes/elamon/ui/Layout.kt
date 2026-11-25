package io.github.elaizes.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.union
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.layout.AlignmentLine
import androidx.compose.ui.layout.FirstBaseline
import androidx.compose.ui.layout.IntrinsicMeasurable
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import kotlin.math.ceil
import kotlin.math.roundToInt
import io.github.elaizes.elamon.data.Grid

/*
 * What the layout's rules (Tokens.kt) are kept with: rows of the one grid, text that gets smaller
 * before it is cut, and the room at the screen's sides.
 */

/**
 * A row of the grid: [content]'s children are the widgets of [line], in order, [gap] apart and, with
 * [even], all as tall as the tallest. Each is given its width (and height) to fill.
 */
@Composable
fun GridLine(line: Grid.Line, modifier: Modifier = Modifier, gap: Dp = Space.gap, even: Boolean = true, content: @Composable () -> Unit) {
	Layout(content, modifier.fillMaxWidth()) { cells, constraints ->
		val between = gap.roundToPx()
		val width = if (constraints.hasBoundedWidth) constraints.maxWidth else cells.sumOf { it.maxIntrinsicWidth(constraints.maxHeight) }
		var column = 0
		val extents = line.cells.map { (_, span) -> Grid.extent(column, span, line.columns, width, between).also { column += span } }
		val tall = if (even && cells.size > 1) cells.indices.maxOf { cells[it].minIntrinsicHeight(extents[it].width.coerceAtLeast(0)) } else null
		val placed = cells.mapIndexed { i, cell ->
			val w = extents[i].width.coerceAtLeast(0)
			cell.measure(if (tall == null) Constraints.fixedWidth(w) else Constraints.fixed(w, tall))
		}
		layout(width, placed.maxOfOrNull { it.height } ?: 0) {
			placed.forEachIndexed { i, p -> p.placeRelative(extents[i].start, 0) }
		}
	}
}

/** [items] of one kind, [most] to a row at most, in rows as even as they go, each row from edge to edge. */
@Composable
fun <T> EvenRows(items: List<T>, most: Int, modifier: Modifier = Modifier, gap: Dp = Space.inner, item: @Composable (T) -> Unit) {
	Column(modifier, verticalArrangement = Arrangement.spacedBy(gap)) {
		for (line in Grid.even(items.size, most)) GridLine(line, gap = gap) { for ((i, _) in line.cells) item(items[i]) }
	}
}

/** How wide the cell of the grid is that a widget is drawn in: what it has room for beside each other. */
val LocalCellWidth = compositionLocalOf { Dp.Infinity }

/** The width of a cell [span] wide in [line], in a row [width] wide. */
fun cellWidth(line: Grid.Line, span: Int, width: Dp, gap: Dp = Space.gap): Dp = (width + gap) * span / line.columns - gap

/** Whether the cell this is in is narrower than [least], which grows with the phone's text. */
@Composable
fun narrowCell(least: Dp): Boolean = LocalCellWidth.current < least.forText()

/** A length made for words, as much longer as the phone's text is larger. */
@Composable
fun Dp.forText(): Dp = this * with(LocalDensity.current) { TextSizes.words.toDp().value / TextSizes.words.value }

/** Whether text is large on this screen, at the phone's text size or the app's scale: what is one line has more then. */
@Composable
fun largeText(): Boolean = LocalDensity.current.fontScale > TextSizes.LARGE || contentWidth() < TextSizes.screenLeast

/** The width the screen's cards have: all of it but its sides. */
@Composable
fun contentWidth(): Dp {
	val px = LocalWindowInfo.current.containerSize.width
	return with(LocalDensity.current) { px.toDp() } - (Space.screen + sideInset()) * 2
}

/**
 * What both sides of the screen keep clear of besides [Space.screen]: the camera and a navigation
 * bar at the side of a turned phone. The same on both, so what is between is in the middle.
 */
@Composable
fun sideInset(): Dp {
	val density = LocalDensity.current
	val direction = LocalLayoutDirection.current
	val insets = WindowInsets.displayCutout.union(WindowInsets.navigationBars)
	return with(density) { maxOf(insets.getLeft(density, direction), insets.getRight(density, direction)).toDp() }
}

/**
 * One line that gets smaller where it doesn't fit its room, rather than lose its end: a figure, a
 * short label. Down to [min] of its size; what still doesn't fit is cut short there.
 */
fun Modifier.shrinkToFit(min: Float = Shrink.FIGURE): Modifier = shrink(min, line = true) { it.maxIntrinsicWidth(Constraints.Infinity) }

/** Text that gets smaller where its longest word doesn't fit its room, rather than break inside it. */
fun Modifier.wholeWords(): Modifier = shrink(Shrink.WORDS, line = false) { it.minIntrinsicWidth(Constraints.Infinity) }

/** A name of parts ("qemu-system-x86_64"), which may go on to the next line after one but not inside it. */
internal fun breakable(name: String): String = name.replace(PART_END, "$1\u200b")

private val PART_END = Regex("([-_./])(?=[^-_./])")

/** The least text gets smaller to, of its size. */
object Shrink {
	const val FIGURE = 0.45f

	/** Words that may go on to the next line. */
	const val WORDS = 0.6f

	/** Labels side by side, which read as one kind: hardly smaller than each other. */
	const val LABEL = 0.8f

	/** A tab's, at the phone's usual text; at a larger one as small as there. */
	const val TAB = 0.8f
}

/** How much smaller what is [need] wide is drawn in [room], down to [min]. */
internal fun shrinkTo(room: Int, need: Int, min: Float): Float = if (need <= room || need <= 0) 1f else (room.toFloat() / need).coerceAtLeast(min)

/** With [line], one line keeps its height and its baseline, smaller on it: what stands beside or under it stays in place. */
private fun Modifier.shrink(min: Float, line: Boolean, needs: (IntrinsicMeasurable) -> Int): Modifier = layout { measurable, constraints ->
	val f = if (constraints.hasBoundedWidth) shrinkTo(constraints.maxWidth, needs(measurable), min) else 1f
	if (f == 1f) {
		val p = measurable.measure(constraints)
		return@layout layout(p.width, p.height) { p.place(0, 0) }
	}
	fun larger(px: Int) = if (px == Constraints.Infinity) px else ceil(px / f).toInt()
	val p = measurable.measure(Constraints(larger(constraints.minWidth), larger(constraints.maxWidth), larger(constraints.minHeight), larger(constraints.maxHeight)))
	fun smaller(px: Int) = (px * f).roundToInt()
	val base = p[FirstBaseline]
	val y = if (!line) 0 else if (base == AlignmentLine.Unspecified) (p.height - smaller(p.height)) / 2 else base - smaller(base)
	val lines: Map<AlignmentLine, Int> = if (base == AlignmentLine.Unspecified) emptyMap() else mapOf(FirstBaseline to if (line) base else smaller(base))
	layout(smaller(p.width).coerceAtMost(constraints.maxWidth), if (line) p.height else smaller(p.height), lines) {
		p.placeWithLayer(0, y) {
			scaleX = f
			scaleY = f
			transformOrigin = TransformOrigin(0f, 0f)
		}
	}
}

/** On a line [height] tall with its baseline [baseline] down from its top, whatever its own size: a figure, or a word in its place. */
fun Modifier.onLine(height: TextUnit, baseline: TextUnit): Modifier = layout { measurable, constraints ->
	val p = measurable.measure(constraints.copy(minHeight = 0))
	val tall = height.roundToPx()
	val down = baseline.roundToPx()
	val base = p[FirstBaseline]
	val y = (if (base == AlignmentLine.Unspecified) (tall - p.height) / 2 else down - base).coerceAtLeast(0)
	layout(p.width, maxOf(tall, y + p.height), mapOf(FirstBaseline to maxOf(down, base))) { p.place(0, y) }
}

/** Its baseline where text in [style] has it, whatever moves in it: a rolling digit carries its own along. */
@Composable
fun Modifier.baselineOf(style: TextStyle): Modifier {
	val measurer = rememberTextMeasurer()
	val base = remember(measurer, style) { measurer.measure("0", style).firstBaseline.roundToInt() }
	return layout { measurable, constraints ->
		val p = measurable.measure(constraints)
		layout(p.width, p.height, mapOf(FirstBaseline to base)) { p.place(0, 0) }
	}
}

/** What is over the other, at its top, and what is at its bottom, however much taller it is made: [last] as its [main] ends where they have no more room. */
@Composable
fun Footed(modifier: Modifier, gap: Dp, main: @Composable () -> Unit, last: @Composable () -> Unit) {
	Layout(listOf(main, last), modifier) { (first, under), constraints ->
		val wide = constraints.copy(minHeight = 0)
		val footer = under.firstOrNull()?.measure(wide)
		val below = footer?.let { it.height + gap.roundToPx() } ?: 0
		val body = first.first().measure(wide.copy(maxHeight = if (wide.hasBoundedHeight) (wide.maxHeight - below).coerceAtLeast(0) else wide.maxHeight))
		val height = maxOf(constraints.minHeight, body.height + below)
		layout(maxOf(body.width, footer?.width ?: 0), height) {
			body.placeRelative(0, 0)
			footer?.placeRelative(0, height - footer.height)
		}
	}
}

/** Takes the room of its picture alone, [empty] less on both sides than its touch takes: a round button in line with what is beside it. */
fun Modifier.flush(empty: Dp): Modifier = layout { measurable, constraints ->
	val side = empty.roundToPx()
	val p = measurable.measure(constraints)
	layout((p.width - side * 2).coerceAtLeast(0), p.height) { p.place(-side, 0) }
}

/** Moves what ends in an icon's own empty margin out to the line its picture should end on. */
fun Modifier.optical(by: Dp): Modifier = layout { measurable, constraints ->
	val p = measurable.measure(constraints)
	layout(p.width, p.height) { p.placeRelative(by.roundToPx(), 0) }
}
