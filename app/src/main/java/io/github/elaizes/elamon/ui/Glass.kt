package io.github.elaizes.elamon.ui

import android.os.Build
import android.view.WindowManager
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import dev.chrisbanes.haze.HazeProgressive
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeStyle
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect
import io.github.elaizes.elamon.R

/** The content a screen's bars blur, provided by the screen. */
val LocalHaze = staticCompositionLocalOf<HazeState?> { null }

/** How much of the bottom of the screen something floats over, as measured; null where nothing does. */
val LocalBottomReserve = compositionLocalOf<Dp?> { null }

/**
 * How much of the bottom a list, a snackbar or the glass has to keep clear: what floats there
 * (the tab bar, a button) as measured, the system's navigation bar included, or that bar alone.
 */
@Composable
fun bottomReserve(): Dp {
	LocalBottomReserve.current?.let { return it }
	val d = LocalDensity.current
	return with(d) { WindowInsets.navigationBars.getBottom(d).toDp() }
}

private enum class BottomSlot { Bar, Content }

/**
 * [content] with [bar] floating over its bottom edge. The bar is measured first, so everything in
 * [content] knows from [bottomReserve] how much of the bottom it covers, from the first frame and
 * at any font size. An empty [bar] covers only the system's navigation bar.
 */
@Composable
fun BottomBarLayout(
	modifier: Modifier = Modifier,
	barAlignment: Alignment.Horizontal = Alignment.CenterHorizontally,
	bar: @Composable () -> Unit,
	content: @Composable () -> Unit,
) {
	val nav = WindowInsets.navigationBars
	SubcomposeLayout(modifier) { constraints ->
		val w = constraints.maxWidth
		val h = constraints.maxHeight
		val bars = subcompose(BottomSlot.Bar, bar).map { it.measure(constraints.copy(minWidth = 0, minHeight = 0)) }
		val covered = maxOf(bars.maxOfOrNull { it.height } ?: 0, nav.getBottom(this))
		val reserve = covered.toDp()
		val main = subcompose(BottomSlot.Content) {
			CompositionLocalProvider(LocalBottomReserve provides reserve, content = content)
		}.map { it.measure(constraints) }
		layout(w, h) {
			main.forEach { it.place(0, 0) }
			bars.forEach { it.place(barAlignment.align(it.width, w, layoutDirection), h - it.height) }
		}
	}
}

/** Frosted glass over the screen's content, or an almost opaque surface where blur is off. */
@Composable
fun Modifier.glass(alpha: Float = 1f, fadeDown: Boolean = false, fadeUp: Boolean = false): Modifier {
	val prefs = LocalPrefs.current
	val haze = LocalHaze.current
	val surface = MaterialTheme.colorScheme.surface
	val dark = LocalDark.current
	if (alpha <= MIN_ALPHA) return this
	if (!prefs.useBlur || haze == null) return this.background(surface.copy(alpha = Alpha.GLASS_SOLID * alpha))
	val style = HazeStyle(
		backgroundColor = surface,
		tints = listOf(HazeTint(surface.copy(alpha = if (dark) Alpha.GLASS_TINT_DARK else Alpha.GLASS_TINT_LIGHT))),
		blurRadius = Blur.glass,
		noiseFactor = Blur.NOISE,
	)
	return this.hazeEffect(haze, style) {
		this.alpha = alpha
		// Bars at the screen's edges fade their blur out toward the content.
		progressive = when {
			fadeDown -> HazeProgressive.verticalGradient(startIntensity = 1f, endIntensity = Blur.EDGE)
			fadeUp -> HazeProgressive.verticalGradient(startIntensity = Blur.EDGE, endIntensity = 1f)
			else -> null
		}
	}
}

/** Glass this faint isn't drawn at all. */
private const val MIN_ALPHA = 0.001f

/** How far a list scrolls before its big title has gone under the bar: about the title's line. */
@Composable
private fun titleTravelPx(): Float = with(LocalDensity.current) { MaterialTheme.typography.headlineMedium.lineHeight.toPx() }

/** How much of that the title may scroll before the bar takes over with the small one. */
private const val COLLAPSE_AT = 0.6f

/** True once a list has scrolled its big title out from under the bar. */
@Composable
fun rememberCollapsed(list: LazyListState): Boolean {
	val px = titleTravelPx() * COLLAPSE_AT
	val collapsed by remember(list, px) {
		derivedStateOf { list.firstVisibleItemIndex > 0 || list.firstVisibleItemScrollOffset > px }
	}
	return collapsed
}

/** The top bar's height under the status bar: [Sizes.topBarMin], more when large text needs it. */
@Composable
fun topBarHeight(): Dp {
	val t = MaterialTheme.typography
	val text = with(LocalDensity.current) { t.titleLarge.lineHeight.toDp() + t.labelMedium.lineHeight.toDp() }
	return maxOf(Sizes.topBarMin, text + Space.l)
}

/**
 * The bar above a screen: see-through while the big title shows, glass with a small title
 * once it has scrolled away.
 */
@Composable
fun GlassTopBar(
	title: String,
	collapsed: Boolean,
	onBack: (() -> Unit)? = null,
	subtitle: String? = null,
	actions: @Composable RowScope.() -> Unit = {},
) {
	val animate = LocalPrefs.current.animations
	val a by animateFloatAsState(if (collapsed) 1f else 0f, tween(if (animate) Motion.BAR_FADE else 0), label = "bar")
	Box(Modifier.fillMaxWidth().glass(alpha = a, fadeDown = true)) {
		Row(
			Modifier.statusBarsPadding().fillMaxWidth().height(topBarHeight()).padding(horizontal = Space.xs),
			verticalAlignment = Alignment.CenterVertically,
		) {
			if (onBack != null) {
				IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
			} else {
				Spacer(Modifier.width(Space.m))
			}
			Column(Modifier.weight(1f).padding(start = Space.xs)) {
				AnimatedVisibility(
					collapsed,
					enter = fadeIn(tween(if (animate) Motion.TITLE_IN else 0)),
					exit = fadeOut(tween(if (animate) Motion.TITLE_OUT else 0)),
				) {
					Column {
						Text(title, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
						if (subtitle != null) {
							Text(subtitle, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
						}
					}
				}
			}
			actions()
		}
	}
}

/** How far in from the screen's sides something in the top bar keeps clear of its buttons. */
val BarButtonsClearance: Dp = Space.xs + Sizes.touchTarget + Space.xs

/**
 * A screen's big title, right under the bar and as tall as what it holds; it fades as it scrolls
 * under the bar, where the small title takes over.
 */
@Composable
fun BigTitle(
	title: String,
	subtitle: (@Composable () -> Unit)? = null,
	extra: (@Composable () -> Unit)? = null,
	list: LazyListState? = null,
) {
	val fadePx = titleTravelPx()
	val alpha by remember(list, fadePx) {
		derivedStateOf {
			when {
				list == null -> 1f
				list.firstVisibleItemIndex > 0 -> 0f
				else -> (1f - list.firstVisibleItemScrollOffset / fadePx).coerceIn(0f, 1f)
			}
		}
	}
	Column(
		Modifier
			.fillMaxWidth()
			.padding(top = topBarHeight())
			.graphicsLayer { this.alpha = alpha }
			.padding(start = Space.xxl, end = Space.xxl, top = Space.xs, bottom = Space.m),
	) {
		Text(title, style = MaterialTheme.typography.headlineMedium, maxLines = TITLE_LINES, overflow = TextOverflow.Ellipsis)
		if (subtitle != null) {
			Spacer(Modifier.height(Space.xs))
			subtitle()
		}
		if (extra != null) {
			Spacer(Modifier.height(Space.m))
			extra()
		}
	}
}

/** A long name wraps once before it's cut. */
private const val TITLE_LINES = 2

class NavItem(val label: String, val icon: ImageVector)

private val NavBarShape = RoundedCornerShape(Radius.floating)
private val NavItemShape = RoundedCornerShape(Radius.card)

/** The least a tab's label shrinks to before it is cut short. */
private val NavLabelMin = 9.sp

/** The tab bar as a floating pill of glass; to a screen reader, a row of tabs with one selected. */
@Composable
fun FloatingNavBar(items: List<NavItem>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = Space.xl, vertical = Space.m), contentAlignment = Alignment.Center) {
		val shadow = Color.Black.copy(alpha = Alpha.SHADOW)
		val moveMs = if (LocalPrefs.current.animations) Motion.NAV else 0
		Row(
			Modifier
				.fillMaxWidth()
				.shadow(Space.lPlus, NavBarShape, ambientColor = shadow, spotColor = shadow)
				.clip(NavBarShape)
				.glass()
				.padding(Space.xsPlus)
				.selectableGroup(),
			horizontalArrangement = Arrangement.spacedBy(Space.xxs),
		) {
			items.forEachIndexed { i, item ->
				val on = i == selected
				val bg by animateColorAsState(
					if (on) MaterialTheme.colorScheme.primaryContainer else Color.Transparent,
					tween(moveMs),
					label = "navbg",
				)
				val fg by animateColorAsState(
					if (on) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant,
					tween(moveMs),
					label = "navfg",
				)
				Column(
					Modifier
						.weight(1f)
						.clip(NavItemShape)
						.background(bg)
						.selectable(
							selected = on,
							interactionSource = remember { MutableInteractionSource() },
							indication = null,
							role = Role.Tab,
							onClick = { onSelect(i) },
						)
						.padding(horizontal = Space.xxs, vertical = Space.s),
					horizontalAlignment = Alignment.CenterHorizontally,
				) {
					// The label below says it; the icon would only say it twice.
					Icon(item.icon, null, tint = fg, modifier = Modifier.size(Sizes.iconL))
					// At a large font a long label shrinks to fit its tab, down to a floor, then ends in "…".
					val label = MaterialTheme.typography.labelSmall
					Text(
						item.label,
						style = label,
						color = fg,
						fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium,
						maxLines = 1,
						overflow = TextOverflow.Ellipsis,
						autoSize = TextAutoSize.StepBased(minFontSize = NavLabelMin, maxFontSize = label.fontSize),
					)
				}
			}
		}
	}
}

/** The bottom of the screen, where content fades into glass under what floats there: [bottomReserve] tall. */
@Composable
fun BottomGlass(modifier: Modifier = Modifier) {
	val prefs = LocalPrefs.current
	val haze = LocalHaze.current
	val surface = MaterialTheme.colorScheme.surface
	val m = modifier.fillMaxWidth().height(bottomReserve())
	if (prefs.useBlur && haze != null) {
		Box(m.glass(fadeUp = true))
	} else {
		Box(m.background(Brush.verticalGradient(listOf(Color.Transparent, surface.copy(alpha = Alpha.SCRIM)))))
	}
}

/** Blurs whatever is behind the dialog this is called in (Android 12+, where the device allows it). */
@Composable
fun BlurBehind(radius: Int = Blur.BEHIND_PX) {
	val prefs = LocalPrefs.current
	val view = LocalView.current
	SideEffect {
		val window = (view.parent as? DialogWindowProvider)?.window ?: return@SideEffect
		if (prefs.useBlur && Build.VERSION.SDK_INT >= 31) {
			window.addFlags(WindowManager.LayoutParams.FLAG_BLUR_BEHIND)
			window.attributes = window.attributes.apply { blurBehindRadius = radius }
			window.setDimAmount(Blur.BEHIND_DIM)
		}
	}
}

private val DialogShape = RoundedCornerShape(Radius.floating)

/** An AlertDialog over a blurred screen. */
@Composable
fun GlassDialog(
	onDismiss: () -> Unit,
	title: String,
	text: (@Composable () -> Unit)? = null,
	icon: ImageVector? = null,
	confirm: @Composable () -> Unit,
	dismiss: (@Composable () -> Unit)? = null,
) {
	AlertDialog(
		onDismissRequest = onDismiss,
		icon = icon?.let { { Icon(it, null) } },
		title = {
			BlurBehind()
			Text(title)
		},
		// Long text at a large font scrolls between the title and the buttons, which stay in sight.
		text = text?.let { t -> { Column(Modifier.verticalScroll(rememberScrollState())) { t() } } },
		confirmButton = confirm,
		dismissButton = dismiss,
		shape = DialogShape,
		// Laid out within the window's insets, so the keyboard shrinks the dialog instead of covering its buttons.
		modifier = Modifier.imePadding(),
		properties = DialogProperties(decorFitsSystemWindows = false),
	)
}
