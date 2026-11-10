package ua.elamon.ui

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.graphics.drawable.ColorDrawable
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.core.graphics.drawable.toDrawable
import androidx.core.view.WindowCompat
import ua.elamon.Prefs
import ua.elamon.R
import ua.elamon.ThemeMode

// Neutral greys with the primary's blue in them, so nothing of Material's default purple shows through.
private val DarkFallback = darkColorScheme(
	primary = Color(0xFF8AB4FF),
	onPrimary = Color(0xFF00285C),
	primaryContainer = Color(0xFF1F3A68),
	onPrimaryContainer = Color(0xFFD8E2FF),
	secondary = Color(0xFFBEC6DC),
	secondaryContainer = Color(0xFF3E4759),
	tertiary = Color(0xFFDDBCE0),
	background = Color(0xFF111318),
	surface = Color(0xFF111318),
	onSurfaceVariant = Color(0xFFC4C6D0),
	outline = Color(0xFF8E9099),
	outlineVariant = Color(0xFF44474F),
	surfaceContainerLowest = Color(0xFF0C0E13),
	surfaceContainerLow = Color(0xFF191C20),
	surfaceContainer = Color(0xFF1D2024),
	surfaceContainerHigh = Color(0xFF282A2F),
	surfaceContainerHighest = Color(0xFF33353A),
)

private val LightFallback = lightColorScheme(
	primary = Color(0xFF3A5BA9),
	onPrimary = Color.White,
	primaryContainer = Color(0xFFDAE2FF),
	onPrimaryContainer = Color(0xFF001848),
	secondary = Color(0xFF575E71),
	tertiary = Color(0xFF715573),
	background = Color(0xFFF7F8FC),
	surface = Color(0xFFF7F8FC),
	onSurfaceVariant = Color(0xFF44474F),
	outline = Color(0xFF74777F),
	outlineVariant = Color(0xFFC4C6D0),
	surfaceContainerLowest = Color.White,
	surfaceContainerLow = Color(0xFFFFFFFF),
	surfaceContainer = Color(0xFFEFF1F7),
	surfaceContainerHigh = Color(0xFFE8EAF1),
	surfaceContainerHighest = Color(0xFFE1E3EA),
)

/** Dark mode on black: the surfaces step up from pure black so cards still read as cards. */
private fun ColorScheme.pureBlack() = copy(
	background = Color.Black,
	surface = Color.Black,
	surfaceContainerLowest = Color.Black,
	surfaceContainerLow = Color(0xFF0E0E10),
	surfaceContainer = Color(0xFF141416),
	surfaceContainerHigh = Color(0xFF1C1C1F),
	surfaceContainerHighest = Color(0xFF26262A),
)

/** Light mode, One UI style: white cards on a faintly tinted page. */
private fun ColorScheme.cardsOnTint() = copy(
	surfaceContainerLow = Color.White,
	background = lerp(background, surfaceContainerHigh, 0.35f),
	surface = lerp(surface, surfaceContainerHigh, 0.35f),
)

/** Colours for the kinds of data, the same on every screen so a reader learns them once. */
@Immutable
data class MetricColors(
	val cpu: Color,
	val memory: Color,
	val swap: Color,
	val gpu: Color,
	val rx: Color,
	val tx: Color,
	val read: Color,
	val write: Color,
	val battery: Color,
	val good: Color,
	val warn: Color,
	val hot: Color,
)

private fun metricColors(dark: Boolean) = if (dark) {
	MetricColors(
		cpu = Color(0xFF7AB8FF),
		memory = Color(0xFFC3A6FF),
		swap = Color(0xFF8E7CD6),
		gpu = Color(0xFF6EE7A8),
		rx = Color(0xFF5EE0F2),
		tx = Color(0xFFFFB45C),
		read = Color(0xFF8FB3FF),
		write = Color(0xFFFF8FC6),
		battery = Color(0xFFFFD84D),
		good = Color(0xFF4ADE80),
		warn = Color(0xFFFACC15),
		hot = Color(0xFFFF6B6B),
	)
} else {
	MetricColors(
		cpu = Color(0xFF1E6FE0),
		memory = Color(0xFF7446D8),
		swap = Color(0xFF9A88E0),
		gpu = Color(0xFF0F9F5A),
		rx = Color(0xFF0B8DA8),
		tx = Color(0xFFD9690B),
		read = Color(0xFF2B5FD9),
		write = Color(0xFFD0337E),
		battery = Color(0xFFB98900),
		good = Color(0xFF16A34A),
		warn = Color(0xFFCA8A04),
		hot = Color(0xFFDC2626),
	)
}

val LocalMetricColors = staticCompositionLocalOf { metricColors(true) }
val LocalPrefs = staticCompositionLocalOf { Prefs() }
val LocalDark = staticCompositionLocalOf { true }

object Metric {
	val colors: MetricColors @Composable get() = LocalMetricColors.current
}

/** Green to amber to red as a temperature nears its limit. */
@Composable
fun tempColor(value: Float, high: Float? = null, crit: Float? = null): Color {
	val c = Metric.colors
	val hot = crit ?: high ?: 95f
	val warm = (high?.takeIf { it < hot } ?: (hot - 20f)).coerceAtLeast(40f)
	return when {
		value >= hot -> c.hot
		value >= warm -> lerp(c.warn, c.hot, ((value - warm) / (hot - warm)).coerceIn(0f, 1f))
		value >= warm - 20f -> lerp(c.good, c.warn, ((value - (warm - 20f)) / 20f).coerceIn(0f, 1f))
		else -> c.good
	}
}

@Composable
fun loadColor(fraction: Float, base: Color = MaterialTheme.colorScheme.primary): Color {
	val c = Metric.colors
	return when {
		fraction >= 0.9f -> c.hot
		fraction >= 0.75f -> lerp(c.warn, c.hot, (fraction - 0.75f) / 0.15f)
		else -> base
	}
}

@OptIn(androidx.compose.ui.text.ExperimentalTextApi::class)
private fun inter(weight: Int) = Font(
	R.font.inter,
	FontWeight(weight),
	variationSettings = FontVariation.Settings(FontVariation.weight(weight)),
)

val Inter = FontFamily(inter(400), inter(500), inter(600), inter(700), inter(800))

/** Tabular figures keep changing numbers from shifting sideways every second. */
private fun TextStyle.ui() = copy(fontFamily = Inter, fontFeatureSettings = "tnum")

private fun typography(): Typography {
	val b = Typography()
	return Typography(
		displayLarge = b.displayLarge.ui().copy(fontWeight = FontWeight.Bold, letterSpacing = (-1).sp),
		displayMedium = b.displayMedium.ui().copy(fontWeight = FontWeight.Bold, letterSpacing = (-0.8).sp),
		displaySmall = b.displaySmall.ui().copy(fontWeight = FontWeight.Bold, letterSpacing = (-0.6).sp),
		headlineLarge = b.headlineLarge.ui().copy(fontWeight = FontWeight.Bold, letterSpacing = (-0.5).sp),
		headlineMedium = b.headlineMedium.ui().copy(fontWeight = FontWeight.SemiBold, letterSpacing = (-0.4).sp),
		headlineSmall = b.headlineSmall.ui().copy(fontWeight = FontWeight.SemiBold, letterSpacing = (-0.3).sp),
		titleLarge = b.titleLarge.ui().copy(fontWeight = FontWeight.SemiBold, letterSpacing = (-0.2).sp),
		titleMedium = b.titleMedium.ui().copy(fontWeight = FontWeight.SemiBold),
		titleSmall = b.titleSmall.ui().copy(fontWeight = FontWeight.SemiBold),
		bodyLarge = b.bodyLarge.ui(),
		bodyMedium = b.bodyMedium.ui(),
		bodySmall = b.bodySmall.ui(),
		labelLarge = b.labelLarge.ui().copy(fontWeight = FontWeight.SemiBold),
		labelMedium = b.labelMedium.ui().copy(fontWeight = FontWeight.Medium),
		labelSmall = b.labelSmall.ui().copy(fontWeight = FontWeight.Medium),
	)
}

private val AppTypography = typography()

/** Whether the app is dark with [prefs], outside composition too (the window before Compose draws). */
fun isDark(context: Context, prefs: Prefs): Boolean = when (prefs.theme) {
	ThemeMode.System -> (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
	ThemeMode.Light -> false
	ThemeMode.Dark -> true
}

/** The app's colours with [prefs]; MainActivity takes the window's background from them before Compose starts. */
fun appColorScheme(context: Context, prefs: Prefs, dark: Boolean): ColorScheme {
	val base = when {
		prefs.dynamicColor && Build.VERSION.SDK_INT >= 31 ->
			if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
		dark -> DarkFallback
		else -> LightFallback
	}
	return when {
		dark && prefs.pureBlack -> base.pureBlack()
		!dark -> base.cardsOnTint()
		else -> base
	}
}

@Composable
fun ElamonTheme(prefs: Prefs, content: @Composable () -> Unit) {
	val dark = when (prefs.theme) {
		ThemeMode.System -> isSystemInDarkTheme()
		ThemeMode.Light -> false
		ThemeMode.Dark -> true
	}
	val context = LocalContext.current
	val scheme = appColorScheme(context, prefs, dark)
	// The system bars and the window behind everything follow the app's theme, which may differ from the phone's.
	val view = LocalView.current
	if (!view.isInEditMode) {
		val background = scheme.surface.toArgb()
		SideEffect {
			val window = (view.context as? Activity)?.window ?: return@SideEffect
			val bars = WindowCompat.getInsetsController(window, view)
			bars.isAppearanceLightStatusBars = !dark
			bars.isAppearanceLightNavigationBars = !dark
			if ((window.decorView.background as? ColorDrawable)?.color != background) window.setBackgroundDrawable(background.toDrawable())
			// The app draws its own glass under the navigation bar.
			if (Build.VERSION.SDK_INT >= 29) window.isNavigationBarContrastEnforced = false
		}
	}
	CompositionLocalProvider(
		LocalMetricColors provides metricColors(dark),
		LocalPrefs provides prefs,
		LocalDark provides dark,
	) {
		MaterialTheme(colorScheme = scheme, typography = AppTypography, content = content)
	}
}
