package io.github.elaizes.elamon.ui

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/*
 * The app's measures in one place: spacing, corners, sizes, motion, see-through levels, blur and
 * the parts of charts. Colours live in Palette.kt; where readings turn amber and red, in
 * Thresholds.kt.
 */

/**
 * Spacing on a 4 dp grid. The "Plus" steps in between are kept where a full step would show:
 * an icon tight against its label, the inside of chips, the padding that sets a card's density.
 */
object Space {
	val xxs = 2.dp
	val xs = 4.dp
	val xsPlus = 6.dp
	val s = 8.dp
	val sPlus = 10.dp
	val m = 12.dp
	val mPlus = 14.dp
	val l = 16.dp
	val lPlus = 18.dp
	val xl = 20.dp
	val xxl = 24.dp
	val xxlPlus = 28.dp
	val xxxl = 32.dp

	/** Between a big number and the unit set smaller beside it. */
	val unitGap = 3.dp

	/** How far a unit sits up from the bottom to meet a big number's baseline, and a smaller one's. */
	val unitLift = 5.dp
	val unitLiftSmall = 3.dp
}

/** Corner radii, smallest to largest. */
object Radius {
	/** A core's bar. */
	val xs = 4.dp

	/** A chart's tooltip. */
	val s = 8.dp

	/** A command in the setup steps. */
	val sPlus = 10.dp

	/** Code blocks, the fingerprint, monitors in the arrangement. */
	val m = 12.dp

	/** Core cells, an action's output. */
	val mPlus = 14.dp

	/** Home's small figures, a cover. */
	val l = 16.dp

	/** Tiles inside a card (profiles, power, actions) and the note about versions. */
	val lPlus = 18.dp

	/** A process's row. */
	val xl = 20.dp

	/** Cards, and the selected tab in the tab bar. */
	val card = 26.dp

	/** The search field and the add button. */
	val field = 28.dp

	/** The floating tab bar and dialogs. */
	val floating = 32.dp
}

val CardShape = RoundedCornerShape(Radius.card)

/** Fully round ends, for tags and chips. */
val PillShape = CircleShape

/** Sizes of things that aren't spacing: icons, discs, controls, bars. */
object Sizes {
	/** The least a control may be to the finger. */
	val touchTarget = 48.dp

	// ---- icons ------------------------------------------------------------------------------
	val iconS = 16.dp
	val iconM = 18.dp

	/** Tab bar icons, and the moon of a sleeping GPU. */
	val iconL = 22.dp

	/** The play button's icon, and the note standing in for a missing cover. */
	val iconXl = 30.dp
	val iconArt = 32.dp

	/** An icon's share of the disc it sits on. */
	const val BADGE_ICON = 0.56f

	// ---- discs and avatars --------------------------------------------------------------------
	val avatarS = 32.dp
	val badge = 34.dp
	val badgeRow = 36.dp
	val avatar = 40.dp
	val badgeFound = 44.dp
	val avatarSheet = 48.dp
	val badgeComputer = 52.dp
	val pairIcon = 56.dp
	val badgeNotice = 64.dp

	/** A setup step's number. */
	val stepNumber = 28.dp

	/** A cover, as shown and as decoded. */
	val art = 72.dp
	val playButton = 64.dp

	/** The menu button on a service's tile, as drawn; its touch target is still [touchTarget]. */
	val tileMenu = 32.dp

	/** A tag's dot, and a legend's. */
	val pillDot = 6.dp
	val legendDot = 8.dp

	// ---- bars ---------------------------------------------------------------------------------
	val barHair = 4.dp
	val barMini = 5.dp
	val barThin = 6.dp
	val bar = 8.dp
	val barTile = 10.dp
	val barThick = 12.dp

	// ---- the rest -----------------------------------------------------------------------------
	/** The top bar under the status bar, before large text makes it taller. */
	val topBarMin = 64.dp

	/** A spinner's line, inside a button or a row. */
	val spinnerStroke = 2.dp

	/** A power button's least height, so a row with "needs permission" doesn't stand out. */
	val powerTileMin = 112.dp

	/** The figures at the end of a process's row. */
	val processFigures = 96.dp

	/** The volume's percentage beside its slider. */
	val volumeLabel = 44.dp

	/** The tallest an action's output and the licence text get before they scroll. */
	val outputMax = 360.dp
	val licenceMax = 420.dp
}

/** How long things move, in milliseconds; with animations off, everything but the bars' fades is instant. */
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

/** How see-through things are. */
object Alpha {
	/** Bars where blur is off: almost opaque. */
	const val GLASS_SOLID = 0.94f

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

	/** A tile's tint, fading from its top-left corner to its bottom-right one. */
	const val TILE_FROM = 0.13f
	const val TILE_TO = 0.02f

	/** A computer card's tint, online and not. */
	const val CARD_ONLINE = 0.12f
	const val CARD_OFFLINE = 0.04f

	/** Hairlines between rows, and between the questions of the guide. */
	const val DIVIDER = 0.4f
	const val DIVIDER_STRONG = 0.5f

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

/** Frosted glass. */
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

	/** On a detail page; the battery's and the temperatures' have less to show. */
	val detail = 200.dp
	val battery = 180.dp
	val temperatures = 170.dp

	/** In a tile, and as a sparkline. */
	val tile = 56.dp
	val sparkline = 36.dp

	/** The monitors' arrangement. */
	val displays = 150.dp

	/** Core bars in the CPU tile's place and on the CPU page, and the gap between them. */
	val cores = 44.dp
	val coresDetail = 56.dp
	val coreGap = 3.dp

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

	/** A monitor's outline, and the gap between monitors. */
	val outline = 2.dp
	val monitorGap = 4.dp

	/** Less than this much of a pixel isn't drawn. */
	const val MIN_PX = 0.5f

	/** The scale's label. */
	val axisText: TextStyle
		@Composable @ReadOnlyComposable get() = MaterialTheme.typography.labelSmall

	/** What a finger reads off a chart. */
	val tooltipText: TextStyle
		@Composable @ReadOnlyComposable get() = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.SemiBold)

	/** A monitor's name in the arrangement. */
	val monitorText: TextStyle
		@Composable @ReadOnlyComposable get() = MaterialTheme.typography.labelLarge
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

	/** Less filled than this shows as empty: a sliver only looks like a glitch. */
	const val MIN_SHOWN = 0.004f
}
