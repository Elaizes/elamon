package io.github.elaizes.elamon.ui

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/*
 * The app's measures in one place: spacing, corners, sizes, motion, see-through levels, blur and
 * the parts of charts. Colours live in Palette.kt; where readings turn amber and red, in
 * Thresholds.kt. No other file has a length of its own.
 *
 * The rules of the layout, which every screen keeps:
 *  1. Mirrored. What is left of a screen, card, tile, dialog, sheet, row or chip is as far from its
 *     edge as what is right of it; a control on the right ends on the line the text on the left
 *     starts on. A card's padding is the same on all its sides.
 *  2. One grid. Columns are of one width, as far from each other across as down; the tiles of a row
 *     are as tall as each other (GridLine, EvenRows).
 *  3. A row that isn't full is shared evenly by what is in it, from edge to edge (Grid.place).
 *  4. Centred is centred: an icon in its disc, a figure in its ring, a label in its button. Tiles of
 *     a kind are built alike, so label, value and caption stand at the same heights side by side.
 *  5. Figures that change keep their place: tabular digits, room for the longest, a unit on the
 *     number's baseline.
 *  6. A word is never broken: it gets smaller, then ends in an ellipsis (wholeWords). What stands
 *     beside a name leaves it its room.
 *  7. Spacing in steps of 4 dp and five corners; a corner inside another is the outer one less the
 *     padding between them.
 *  8. Bars keep the same distance from both sides, and the content clear of them, of the camera and
 *     of the system's bars, upright and turned (sideInset).
 *  9. Loading, empty, failed, unreachable and switched off look alike wherever they are, and leave
 *     what is around them where it is.
 * 10. Text reads on what is under it in every theme; amber and red are the same everywhere.
 * 11. Nothing to touch is smaller than Sizes.touchTarget, and large text or a large scale cuts
 *     nothing off.
 */

/** Spacing, in steps of 4 dp. */
object Space {
	val none = 0.dp

	/** Half a step: between the lines of a log, where a whole one would take every tenth line off the screen. */
	val xxs = 2.dp
	val xs = 4.dp
	val s = 8.dp
	val m = 12.dp
	val l = 16.dp
	val xl = 24.dp
	val xxl = 32.dp

	/** From the screen's sides to its cards, and from a card's edge to what it holds: where all text starts. */
	val screen = l
	val card = l

	/** Between cards, and between the tiles inside one: the same across and down. */
	val gap = m
	val inner = s
}

/** Corner radii. One inside another is the outer one less the padding between them. */
object Radius {
	/** A bar, a core, a timeline. */
	val xs = 4.dp

	/** A chart's tooltip. */
	val s = 8.dp

	/** Cards, sheets, dialogs, the search field and the bar of tabs. */
	val card = 28.dp

	/** The tiles, code and covers a card holds. */
	val inner = card - Space.card

	/** The selected tab in the bar of tabs. */
	val tab = card - Space.xs
}

val CardShape = RoundedCornerShape(Radius.card)

/** What sits inside a card: choices, power, actions, code, a cover. */
val InnerShape = RoundedCornerShape(Radius.inner)

/** What sits in a card inside a card: its padding leaves no more. */
val LeastShape = RoundedCornerShape(Radius.xs)

/** Fully round ends, for tags and chips. */
val PillShape = CircleShape

/** Sizes of things that aren't spacing: icons, discs, controls, bars. */
object Sizes {
	/** The least a control may be to the finger. */
	val touchTarget = 48.dp

	// ---- icons ------------------------------------------------------------------------------
	val iconS = 16.dp
	val iconM = 20.dp
	val icon = 24.dp

	/** The play button's icon, and the note standing in for a missing cover. */
	val iconXl = 32.dp

	/** What an icon's box, or a round button's, leaves empty beside its picture: how far it moves out to end on a line ([optical]). */
	val chevronEmpty = 8.dp
	val foldEmpty = 6.dp
	val buttonEmpty = (touchTarget - icon) / 2
	val smallButtonEmpty = (touchTarget - iconM) / 2

	/** A chip as drawn, in text of the usual size. */
	val chip = 32.dp

	/** The least a chip's label is wide, for one as wide as a finger needs. */
	val chipLabel = touchTarget - Space.l * 2

	// ---- discs and avatars --------------------------------------------------------------------
	/** A letter's disc in a table's row, in a list's, and in a row's sheet. */
	val avatarS = 32.dp
	val avatar = 40.dp

	/** A letter's size in its disc, of the disc's. */
	const val AVATAR_LETTER = 0.42f

	/** The disc with an icon by a card's title or a row's, a computer's, and a notice's; the icon is [BADGE_ICON] of it. */
	val badge = 36.dp
	val badgeL = 48.dp
	val badgeNotice = 72.dp
	const val BADGE_ICON = 5f / 9f

	/** A setup step's number. */
	val stepNumber = 28.dp

	/** A cover, as shown and as decoded. */
	val art = 72.dp
	val playButton = 64.dp

	/** A tag's dot, and a legend's. */
	val pillDot = 6.dp
	val legendDot = 8.dp

	// ---- bars ---------------------------------------------------------------------------------
	/** Under a figure, under a table's row, and in a tile. */
	val barThin = 4.dp
	val bar = 6.dp
	val barTile = 10.dp

	// ---- the rest -----------------------------------------------------------------------------
	/** The top bar under the status bar, before large text makes it taller. */
	val topBarMin = 64.dp

	/** A spinner's line, inside a button or a row. */
	val spinnerStroke = 2.dp

	/** The narrowest a table's column of words is made to leave the rows' names their room. */
	val columnLeast = 40.dp

	/** The bar of tabs' shadow, and that of a widget lifted in the editor. */
	val shadow = 16.dp
	val liftShadow = 8.dp

	/** A hairline round a record of the editor's, and the mark of where a widget is dropped. */
	val outline = 1.dp
	val mark = 2.dp

	/** How near the editor's top or bottom a dragged widget scrolls it, and how far a frame. */
	val dragEdge = 72.dp
	val dragStep = 12.dp

	/** Over and under a line of the home-screen widget, as tall as its text but for it. */
	val widgetLine = 1.dp

	/** The tallest an action's output and the licence text get before they scroll. */
	val outputMax = 360.dp
	val licenceMax = 420.dp

	/** The tallest a log's lines get in their card before they scroll on their own. */
	val logMax = 420.dp
}

/** Measures that grow with the phone's text. */
object TextSizes {
	/** Where a tile's figure stands on its line, and how tall that line is: a word in its place stands as high. */
	val figureBaseline = 32.sp
	val figureLine = 42.sp

	/** A usual text, by which lengths made for words grow ([forText]): Android grows small text more than a length in sp. */
	val words = 14.sp

	/** The least room a button's or a switch's name has beside its picture, alone and inside a card; with less, it goes under it. */
	val rowLeast = 200.dp
	val innerRowLeast = 120.dp

	/** The least room a row's name has beside its figures; with less, they go under it. */
	val nameLeast = 72.dp

	/** The least a choice takes: as many to a row as fit, three at most. */
	val choiceLeast = 72.dp

	/** From this scale of the phone's text on, and on cards narrower than this, text is large: see [largeText]. */
	const val LARGE = 1.3f
	val screenLeast = 280.dp
}

/** How long things move, in milliseconds; with animations off, all of them are instant. */
object Motion {
	/** From screen to screen, and how small the new one starts. */
	const val SCREEN_IN = 240
	const val SCREEN_OUT = 160
	const val SCREEN_SCALE = 0.96f

	/** From a computer's tabs to a detail page and back. */
	const val DETAIL_IN = 220
	const val DETAIL_OUT = 150
	const val DETAIL_SCALE = 0.97f

	/** From tab to tab. */
	const val TAB_IN = 200
	const val TAB_OUT = 120

	/** The top bar turning to glass, and its small title coming and going. */
	const val BAR_FADE = 220
	const val TITLE_IN = 200
	const val TITLE_OUT = 150

	/** A tab's highlight moving. */
	const val NAV = 200

	/** A gauge, a bar and a digit going to their new values; a digit's old value fades a little sooner. */
	const val GAUGE = 600
	const val BAR = 500
	const val DIGIT = 260
	const val DIGIT_FADE = 200
}

object Alpha {
	/** Bars where blur is off: almost opaque. */
	const val GLASS_SOLID = 0.98f

	/** The tint over blurred glass, dark and light. */
	const val GLASS_TINT_DARK = 0.62f
	const val GLASS_TINT_LIGHT = 0.7f

	/** Under the tab bar where blur is off. */
	const val SCRIM = 0.92f
	const val SHADOW = 0.3f

	/** Tinted discs, tags, banners and the buttons that do harm. */
	const val BADGE = 0.16f
	const val PILL = 0.15f
	const val BANNER = 0.14f
	const val DANGER = 0.12f

	/** A tile's tint, fading from its top to its bottom. */
	const val TILE_FROM = 0.13f
	const val TILE_TO = 0.02f

	/** A computer card's tint, online and not. */
	const val CARD_ONLINE = 0.12f
	const val CARD_OFFLINE = 0.04f

	/** Hairlines between rows. */
	const val DIVIDER = 0.4f

	/** What can't be used now. */
	const val DISABLED = 0.45f

	// ---- charts -------------------------------------------------------------------------------
	const val GRID = 0.45f
	const val CROSSHAIR = 0.6f

	/** Under a line, fading to nothing at the bottom. */
	const val FILL = 0.34f

	/** Around the newest point. */
	const val HALO = 0.25f

	/** A gauge's glow, and the start of its ring. */
	const val GLOW = 0.45f
	const val RING_START = 0.55f

	/** The empty end of a bar's gradient, and the far end of a core's or a monitor's. */
	const val BAR_START = 0.7f
	const val FADE_END = 0.6f
}

object Blur {
	val glass = 26.dp
	const val NOISE = 0.04f

	/** How much blur is left where a bar meets the content: it fades out toward it. */
	const val EDGE = 0.35f

	/** Behind dialogs, in pixels as the window takes it, and how much it dims. */
	const val BEHIND_PX = 48
	const val BEHIND_DIM = 0.35f
}

/** The parts of charts, gauges and the monitors' arrangement. */
object ChartStyle {
	/** A chart's height unless given. */
	val height = 120.dp

	/** A graph's heights: "medium", as on 0.5's detail pages, and "large". "small" is a tile's. */
	val detail = 200.dp
	val large = 280.dp

	/** In a tile, and as a sparkline. */
	val tile = 56.dp
	val sparkline = 36.dp

	/** A spark on a computer's card on the home screen. */
	val sparkMini = 24.dp

	/** Core bars in the CPU tile's place and on the CPU page, and the gap between them. */
	val cores = 44.dp
	val coresDetail = 56.dp
	val coreGap = 4.dp

	val line = 2.2.dp
	val grid = 1.dp
	val dashOn = 3.dp
	val dashOff = 5.dp
	val crosshair = 1.5.dp

	/** The newest point and the halo round it; the line is clipped this far outside so the halo shows. */
	val dot = 3.5.dp
	val halo = 7.dp
	val haloRoom = 8.dp

	/** The points under a finger. */
	val touchDot = 5.dp
	val touchDotCore = 2.dp
	val tooltipPadding = 6.dp

	/** The scale's label, in from the left edge. */
	val labelInset = 2.dp

	/** The least room each label of a time axis takes, and the gap that keeps two apart. */
	val axisSpacing = 64.dp
	val axisGap = 8.dp

	/** A timeline's bar, and the least a stretch of it is drawn, however short. */
	val timeline = 24.dp
	val stretchMin = 2.dp

	/** Less than this much of a pixel isn't drawn. */
	const val MIN_PX = 0.5f

	/** The scale's label. */
	val axisText: TextStyle
		@Composable @ReadOnlyComposable get() = MaterialTheme.typography.labelSmall

	/** What a finger reads off a chart. */
	val tooltipText: TextStyle
		@Composable @ReadOnlyComposable get() = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.SemiBold)
}

/** The three-quarter ring of the CPU and memory tiles. */
object Gauge {
	val size = 112.dp
	val tile = 116.dp
	val thickness = 11.dp

	/** Room between the ring and the edge, for its glow. */
	val inset = 4.dp
	val glow = 8.dp

	/** Where the ring starts (degrees from 3 o'clock) and how far round it goes. */
	const val START = 135f
	const val SWEEP = 270f

	/** How much of its square the ring takes from the top, its open end left out. */
	const val SHOWN = 0.88f

	/** How much of its width is between its sides, for what it holds. */
	const val ROOM = 0.68f

	/** Less filled than this shows as empty: a sliver only looks like a glitch. */
	const val MIN_SHOWN = 0.004f
}
