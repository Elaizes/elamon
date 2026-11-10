package io.github.elaizes.elamon.ui

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.graphics.drawable.ColorDrawable
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
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
import io.github.elaizes.elamon.Prefs
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.ThemeMode

val LocalMetricColors = staticCompositionLocalOf { metricColors(true) }
val LocalPrefs = staticCompositionLocalOf { Prefs() }
val LocalDark = staticCompositionLocalOf { true }

object Metric {
	val colors: MetricColors @Composable get() = LocalMetricColors.current
}

/** A [Level] in the colours of [good], [warn] and [hot], blended where the level lies between two. */
fun Level.color(good: Color, warn: Color, hot: Color): Color = when (band) {
	Band.Good -> good
	Band.GoodToWarn -> lerp(good, warn, t)
	Band.Warn -> warn
	Band.WarnToHot -> lerp(warn, hot, t)
	Band.Hot -> hot
}

/** Green to amber to red as a temperature nears its limit: the sensor's own [high] and [crit] when it has them. */
@Composable
fun tempColor(value: Float, high: Float? = null, crit: Float? = null): Color {
	val c = Metric.colors
	return tempLevel(value, high, crit).color(c.good, c.warn, c.hot)
}

/** A load's colour: [base] until it nears full, then amber to red. */
@Composable
fun loadColor(fraction: Float, base: Color = MaterialTheme.colorScheme.primary): Color {
	val c = Metric.colors
	return loadLevel(fraction).color(base, c.warn, c.hot)
}

/** A battery's colour by its charge in %. */
@Composable
fun batteryColor(percent: Float): Color {
	val c = Metric.colors
	return batteryLevel(percent).color(c.good, c.warn, c.hot)
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
	if (prefs.dynamicColor && Build.VERSION.SDK_INT >= 31) {
		return finishScheme(if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context), dark, prefs.pureBlack)
	}
	return fallbackColorScheme(dark, prefs.pureBlack)
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
