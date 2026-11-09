package ua.elamon.ui

import android.os.Build
import android.view.WindowManager
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogWindowProvider
import dev.chrisbanes.haze.HazeProgressive
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeStyle
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect

/** The content a screen's bars blur, provided by the screen. */
val LocalHaze = staticCompositionLocalOf<HazeState?> { null }

/** Frosted glass over the screen's content, or an almost opaque surface where blur is off. */
@Composable
fun Modifier.glass(alpha: Float = 1f, fadeDown: Boolean = false, fadeUp: Boolean = false): Modifier {
	val prefs = LocalPrefs.current
	val haze = LocalHaze.current
	val surface = MaterialTheme.colorScheme.surface
	val dark = LocalDark.current
	if (alpha <= 0.001f) return this
	if (!prefs.useBlur || haze == null) return this.background(surface.copy(alpha = 0.94f * alpha))
	val style = HazeStyle(
		backgroundColor = surface,
		tints = listOf(HazeTint(surface.copy(alpha = if (dark) 0.62f else 0.7f))),
		blurRadius = 26.dp,
		noiseFactor = 0.04f,
	)
	return this.hazeEffect(haze, style) {
		this.alpha = alpha
		// Bars at the screen's edges fade their blur out toward the content.
		progressive = when {
			fadeDown -> HazeProgressive.verticalGradient(startIntensity = 1f, endIntensity = 0.35f)
			fadeUp -> HazeProgressive.verticalGradient(startIntensity = 0.35f, endIntensity = 1f)
			else -> null
		}
	}
}

/** True once a list has scrolled its big title out from under the bar. */
@Composable
fun rememberCollapsed(list: LazyListState, threshold: Dp = 56.dp): Boolean {
	val px = with(LocalDensity.current) { threshold.toPx() }
	val collapsed by remember(list) {
		derivedStateOf { list.firstVisibleItemIndex > 0 || list.firstVisibleItemScrollOffset > px }
	}
	return collapsed
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
	val a by animateFloatAsState(if (collapsed) 1f else 0f, tween(220), label = "bar")
	Box(Modifier.fillMaxWidth().glass(alpha = a, fadeDown = true)) {
		Row(
			Modifier.statusBarsPadding().fillMaxWidth().height(64.dp).padding(horizontal = 4.dp),
			verticalAlignment = Alignment.CenterVertically,
		) {
			if (onBack != null) {
				IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") }
			} else {
				Spacer(Modifier.width(12.dp))
			}
			Column(Modifier.weight(1f).padding(start = 4.dp)) {
				AnimatedVisibility(collapsed, enter = fadeIn(tween(200)), exit = fadeOut(tween(150))) {
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

/** One UI's big title: a third of the screen, so what's on top is within thumb reach. */
@Composable
fun BigTitle(
	title: String,
	subtitle: (@Composable () -> Unit)? = null,
	extra: (@Composable () -> Unit)? = null,
	list: LazyListState? = null,
) {
	val h = (LocalConfiguration.current.screenHeightDp * 0.27f).dp.coerceIn(170.dp, 300.dp)
	val fadePx = with(LocalDensity.current) { (h * 0.45f).toPx() }
	// It fades as it scrolls under the bar, where the small title takes over.
	val alpha by remember(list) {
		derivedStateOf {
			when {
				list == null -> 1f
				list.firstVisibleItemIndex > 0 -> 0f
				else -> (1f - list.firstVisibleItemScrollOffset / fadePx).coerceIn(0f, 1f)
			}
		}
	}
	Column(
		Modifier.fillMaxWidth().height(h).graphicsLayer { this.alpha = alpha }.padding(horizontal = 24.dp),
		verticalArrangement = Arrangement.Center,
	) {
		Text(title, style = MaterialTheme.typography.displaySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
		if (subtitle != null) {
			Spacer(Modifier.height(6.dp))
			subtitle()
		}
		if (extra != null) {
			Spacer(Modifier.height(12.dp))
			extra()
		}
	}
}

/** Space a list keeps clear for the bars that float over it. */
@Composable
fun barPadding(top: Boolean = true, bottomNav: Boolean = false): PaddingValues {
	val d = LocalDensity.current
	val status = with(d) { WindowInsets.statusBars.getTop(d).toDp() }
	val nav = with(d) { WindowInsets.navigationBars.getBottom(d).toDp() }
	return PaddingValues(
		start = 16.dp,
		end = 16.dp,
		top = if (top) status + 64.dp else 0.dp,
		bottom = nav + if (bottomNav) 104.dp else 24.dp,
	)
}

class NavItem(val label: String, val icon: ImageVector)

/** The tab bar as a floating pill of glass. */
@Composable
fun FloatingNavBar(items: List<NavItem>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 20.dp, vertical = 12.dp), contentAlignment = Alignment.Center) {
		Row(
			Modifier
				.fillMaxWidth()
				.shadow(18.dp, RoundedCornerShape(32.dp), ambientColor = Color.Black.copy(alpha = 0.3f), spotColor = Color.Black.copy(alpha = 0.3f))
				.clip(RoundedCornerShape(32.dp))
				.glass()
				.padding(6.dp),
			horizontalArrangement = Arrangement.spacedBy(2.dp),
		) {
			items.forEachIndexed { i, item ->
				val on = i == selected
				val bg by animateColorAsState(
					if (on) MaterialTheme.colorScheme.primaryContainer else Color.Transparent,
					tween(200),
					label = "navbg",
				)
				val fg by animateColorAsState(
					if (on) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant,
					tween(200),
					label = "navfg",
				)
				Column(
					Modifier
						.weight(1f)
						.clip(RoundedCornerShape(26.dp))
						.background(bg)
						.clickable(remember { MutableInteractionSource() }, null) { onSelect(i) }
						.padding(horizontal = 2.dp, vertical = 8.dp),
					horizontalAlignment = Alignment.CenterHorizontally,
				) {
					Icon(item.icon, item.label, tint = fg, modifier = Modifier.size(22.dp))
					Text(
						item.label,
						style = MaterialTheme.typography.labelSmall,
						color = fg,
						fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium,
						maxLines = 1,
						overflow = TextOverflow.Clip,
					)
				}
			}
		}
	}
}

/** The bottom of the screen, where content fades into glass under the nav bar and the floating tabs. */
@Composable
fun BottomGlass(modifier: Modifier = Modifier, extra: Dp = 92.dp) {
	val prefs = LocalPrefs.current
	val haze = LocalHaze.current
	val d = LocalDensity.current
	val nav = with(d) { WindowInsets.navigationBars.getBottom(d).toDp() }
	val surface = MaterialTheme.colorScheme.surface
	val m = modifier.fillMaxWidth().height(nav + extra)
	if (prefs.useBlur && haze != null) {
		Box(m.glass(fadeUp = true))
	} else {
		Box(m.background(Brush.verticalGradient(listOf(Color.Transparent, surface.copy(alpha = 0.92f)))))
	}
}

/** Strip at the very top that keeps the status bar readable over scrolled content. */
@Composable
fun StatusBarGlass() {
	Box(Modifier.fillMaxWidth().windowInsetsTopHeight(WindowInsets.statusBars).glass(fadeDown = true))
}

/** Blurs whatever is behind the dialog this is called in (Android 12+, where the device allows it). */
@Composable
fun BlurBehind(radius: Int = 48) {
	val prefs = LocalPrefs.current
	val view = LocalView.current
	SideEffect {
		val window = (view.parent as? DialogWindowProvider)?.window ?: return@SideEffect
		if (prefs.useBlur && Build.VERSION.SDK_INT >= 31) {
			window.addFlags(WindowManager.LayoutParams.FLAG_BLUR_BEHIND)
			window.attributes = window.attributes.apply { blurBehindRadius = radius }
			window.setDimAmount(0.35f)
		}
	}
}

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
		text = text,
		confirmButton = confirm,
		dismissButton = dismiss,
		shape = RoundedCornerShape(32.dp),
	)
}
