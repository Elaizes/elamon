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
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import dev.chrisbanes.haze.HazeProgressive
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeStyle
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Grid

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
			Modifier.statusBarsPadding().fillMaxWidth().height(topBarHeight()).padding(horizontal = BarInset + sideInset()),
			verticalAlignment = Alignment.CenterVertically,
		) {
			if (onBack != null) {
				IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
			} else {
				// Without a button the small title starts where the big one does.
				Spacer(Modifier.width(Space.screen + Space.card - BarInset - Space.xs))
			}
			Column(Modifier.weight(1f).padding(horizontal = Space.xs)) {
				AnimatedVisibility(
					collapsed,
					enter = fadeIn(tween(if (animate) Motion.TITLE_IN else 0)),
					exit = fadeOut(tween(if (animate) Motion.TITLE_OUT else 0)),
				) {
					Column {
						Text(title, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
						if (subtitle != null) {
							Text(subtitle, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
						}
					}
				}
			}
			actions()
		}
	}
}

/** From the screen's sides to the top bar's buttons: their icons' edges are the cards'. */
private val BarInset: Dp = Space.screen - Sizes.buttonEmpty

/** How far in from the screen's sides something in the top bar keeps clear of its buttons. */
val BarButtonsClearance: Dp = BarInset + Sizes.touchTarget + Space.xs

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
			.padding(start = Space.card, end = Space.card, top = Space.xs, bottom = Space.m),
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

private val NavItemShape = RoundedCornerShape(Radius.tab)

/** The tab bar as a floating pill of glass; to a screen reader, a row of tabs with one selected. */
@Composable
fun FloatingNavBar(items: List<NavItem>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth().overSystemBar().padding(horizontal = Space.screen + sideInset(), vertical = Space.m), contentAlignment = Alignment.Center) {
		val shadow = Color.Black.copy(alpha = Alpha.SHADOW)
		val moveMs = if (LocalPrefs.current.animations) Motion.NAV else 0
		Row(
			Modifier
				.fillMaxWidth()
				.shadow(Sizes.shadow, CardShape, ambientColor = shadow, spotColor = shadow)
				.clip(CardShape)
				.glass()
				.padding(Space.xs)
				.selectableGroup(),
			horizontalArrangement = Arrangement.spacedBy(Space.xs),
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
						.padding(horizontal = Space.xs, vertical = Space.s),
					horizontalAlignment = Alignment.CenterHorizontally,
				) {
					// The label below says it; the icon would only say it twice.
					Icon(item.icon, null, tint = fg, modifier = Modifier.size(Sizes.icon))
					Spacer(Modifier.height(Space.xxs))
					// As heavy whichever tab is on: the labels stay where they are. A long one gets smaller, then ends in "…".
					Text(
						item.label,
						style = MaterialTheme.typography.labelSmall,
						color = fg,
						fontWeight = FontWeight.SemiBold,
						maxLines = 1,
						overflow = TextOverflow.Ellipsis,
						modifier = Modifier.shrinkToFit(),
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

/** The button at the top bar's end for the rest of what a screen offers, and the menu it opens under itself; [item]s close it. */
@Composable
fun OverflowMenu(content: @Composable (item: @Composable (String, () -> Unit) -> Unit) -> Unit) {
	var open by remember { mutableStateOf(false) }
	// Anchored to the button: the bar's row would have it open at the other side of the screen.
	Box {
		IconButton(onClick = { open = true }) { Icon(Icons.Default.MoreVert, stringResource(R.string.machine_more)) }
		DropdownMenu(expanded = open, onDismissRequest = { open = false }, shape = InnerShape) {
			content { label, onClick ->
				DropdownMenuItem(text = { Text(label) }, onClick = { open = false; onClick() })
			}
		}
	}
}

/** Clear of the system's navigation bar at the bottom of the screen; its sides are [sideInset]'s. */
@Composable
fun Modifier.overSystemBar(): Modifier = windowInsetsPadding(WindowInsets.navigationBars.only(WindowInsetsSides.Bottom))

/** The button that adds something, floating at the end of the screen's bottom, on the cards' edge. */
@Composable
fun AddButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
	ExtendedFloatingActionButton(
		onClick = onClick,
		icon = { Icon(Icons.Default.Add, null) },
		text = { Text(label) },
		shape = CardShape,
		containerColor = MaterialTheme.colorScheme.primaryContainer,
		contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
		// Material's extended button hides its text from screen readers: the button says it instead.
		modifier = modifier.overSystemBar().padding(horizontal = Space.screen + sideInset(), vertical = Space.l).semantics { contentDescription = label },
	)
}

/** What a screen says for a moment, over its bottom and clear of what floats there. */
@Composable
fun Messages(host: SnackbarHostState, modifier: Modifier = Modifier) {
	SnackbarHost(host, modifier.padding(horizontal = sideInset()).padding(bottom = bottomReserve() + Space.xs)) { Snackbar(it, shape = InnerShape) }
}

/** A sheet over the bottom of the screen, as far in from both sides; with [whole], open to the top at once. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GlassSheet(onDismiss: () -> Unit, whole: Boolean = false, content: @Composable ColumnScope.() -> Unit) {
	ModalBottomSheet(
		onDismissRequest = onDismiss,
		sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = whole),
		shape = RoundedCornerShape(topStart = Radius.card, topEnd = Radius.card),
		containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
	) {
		Column(
			Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).imePadding().padding(start = Space.xl, end = Space.xl, bottom = Space.xl),
			verticalArrangement = Arrangement.spacedBy(Space.m),
			content = content,
		)
	}
}

/** A dialog's button: what it says and does; [busy], a spinner in the words' place. */
class DialogButton(val label: String, val enabled: Boolean = true, val busy: Boolean = false, val onClick: () -> Unit)

/** An AlertDialog over a blurred screen: its title in the middle, its buttons sharing its width. */
@Composable
fun GlassDialog(
	onDismiss: () -> Unit,
	title: String,
	confirm: DialogButton,
	dismiss: DialogButton? = null,
	icon: ImageVector? = null,
	text: (@Composable () -> Unit)? = null,
) {
	AlertDialog(
		onDismissRequest = onDismiss,
		icon = icon?.let { { Icon(it, null) } },
		title = {
			BlurBehind()
			Text(title, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().wholeWords())
		},
		// Long text at a large font scrolls between the title and the buttons, which stay in sight.
		text = text?.let { t -> { Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState())) { t() } } },
		confirmButton = {
			// One under the other where a half is too narrow for what one says, the one that does it on top.
			EvenButtons {
				if (dismiss != null) DialogAction(dismiss)
				DialogAction(confirm)
			}
		},
		shape = CardShape,
		// Laid out within the window's insets, so the keyboard shrinks the dialog instead of covering its buttons.
		modifier = Modifier.imePadding(),
		properties = DialogProperties(decorFitsSystemWindows = false),
	)
}

/** A dialog that says [text] and no more, in the middle as its title is. */
@Composable
fun GlassDialog(onDismiss: () -> Unit, title: String, text: String, confirm: DialogButton, dismiss: DialogButton? = null) =
	GlassDialog(onDismiss, title, confirm, dismiss) { Text(text, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth()) }

@Composable
private fun DialogAction(b: DialogButton) {
	TextButton(onClick = b.onClick, enabled = b.enabled && !b.busy) {
		if (b.busy) Spinner() else Text(b.label, textAlign = TextAlign.Center)
	}
}

/**
 * Buttons side by side, sharing the width evenly; where one's words don't fit its share, one under
 * the other, the last on top, each the whole width.
 */
@Composable
fun EvenButtons(content: @Composable () -> Unit) {
	Layout(content, Modifier.fillMaxWidth()) { buttons, constraints ->
		val gap = Space.s.roundToPx()
		val n = buttons.size
		val width = constraints.maxWidth
		val line = Grid.Line(List(n) { it to 1 }, n)
		val share = line.cells.map { (i, span) -> Grid.extent(i, span, n, width, gap) }
		if (buttons.indices.all { buttons[it].maxIntrinsicWidth(Constraints.Infinity) <= share[it].width }) {
			val placed = buttons.mapIndexed { i, b -> b.measure(Constraints.fixedWidth(share[i].width)) }
			layout(width, placed.maxOfOrNull { it.height } ?: 0) { placed.forEachIndexed { i, p -> p.placeRelative(share[i].start, 0) } }
		} else {
			val placed = buttons.reversed().map { it.measure(Constraints.fixedWidth(width)) }
			layout(width, placed.sumOf { it.height }) {
				var y = 0
				for (p in placed) {
					p.placeRelative(0, y)
					y += p.height
				}
			}
		}
	}
}
