package ua.elamon.ui

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
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle

private val DarkFallback = darkColorScheme(
	primary = Color(0xFF7DD3FC),
	onPrimary = Color(0xFF00344A),
	primaryContainer = Color(0xFF0E4A63),
	onPrimaryContainer = Color(0xFFC7EBFF),
	secondary = Color(0xFFB4C8D6),
	tertiary = Color(0xFFC4B5FD),
	background = Color(0xFF0E1417),
	surface = Color(0xFF0E1417),
	surfaceContainerLowest = Color(0xFF090F12),
	surfaceContainerLow = Color(0xFF161D20),
	surfaceContainer = Color(0xFF1A2124),
	surfaceContainerHigh = Color(0xFF242B2F),
	surfaceContainerHighest = Color(0xFF2F363A),
)

private val LightFallback = lightColorScheme(
	primary = Color(0xFF00658A),
	onPrimary = Color.White,
	primaryContainer = Color(0xFFC4E7FF),
	onPrimaryContainer = Color(0xFF001E2C),
	secondary = Color(0xFF4E616D),
	tertiary = Color(0xFF6750A4),
	background = Color(0xFFF6FAFD),
	surface = Color(0xFFF6FAFD),
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

private fun metricColors(scheme: ColorScheme, dark: Boolean) = if (dark) {
	MetricColors(
		cpu = scheme.primary,
		memory = Color(0xFFC4B5FD),
		swap = Color(0xFF8B7FD1),
		gpu = Color(0xFF86EFAC),
		rx = Color(0xFF67E8F9),
		tx = Color(0xFFFDBA74),
		read = Color(0xFF93C5FD),
		write = Color(0xFFF9A8D4),
		battery = Color(0xFFFDE047),
		good = Color(0xFF4ADE80),
		warn = Color(0xFFFACC15),
		hot = Color(0xFFF87171),
	)
} else {
	MetricColors(
		cpu = scheme.primary,
		memory = Color(0xFF6D4FD8),
		swap = Color(0xFF9A88E0),
		gpu = Color(0xFF15803D),
		rx = Color(0xFF0E7490),
		tx = Color(0xFFC2410C),
		read = Color(0xFF1D4ED8),
		write = Color(0xFFBE185D),
		battery = Color(0xFFA16207),
		good = Color(0xFF16A34A),
		warn = Color(0xFFCA8A04),
		hot = Color(0xFFDC2626),
	)
}

val LocalMetricColors = staticCompositionLocalOf { metricColors(DarkFallback, true) }

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
fun loadColor(fraction: Float): Color {
	val c = Metric.colors
	return when {
		fraction >= 0.9f -> c.hot
		fraction >= 0.75f -> lerp(c.warn, c.hot, (fraction - 0.75f) / 0.15f)
		else -> MaterialTheme.colorScheme.primary
	}
}

private val Numbers = TextStyle(fontFeatureSettings = "tnum")

@Composable
fun ElamonTheme(content: @Composable () -> Unit) {
	val dark = isSystemInDarkTheme()
	val context = LocalContext.current
	val scheme = when {
		Build.VERSION.SDK_INT >= 31 -> if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
		dark -> DarkFallback
		else -> LightFallback
	}
	val base = Typography()
	// Tabular figures keep changing numbers from shifting sideways every second.
	val typography = Typography(
		displayLarge = base.displayLarge.merge(Numbers),
		displayMedium = base.displayMedium.merge(Numbers),
		displaySmall = base.displaySmall.merge(Numbers),
		headlineLarge = base.headlineLarge.merge(Numbers),
		headlineMedium = base.headlineMedium.merge(Numbers),
		headlineSmall = base.headlineSmall.merge(Numbers),
		titleLarge = base.titleLarge.merge(Numbers),
		titleMedium = base.titleMedium.merge(Numbers),
		titleSmall = base.titleSmall.merge(Numbers),
		bodyLarge = base.bodyLarge.merge(Numbers),
		bodyMedium = base.bodyMedium.merge(Numbers),
		bodySmall = base.bodySmall.merge(Numbers),
		labelLarge = base.labelLarge.merge(Numbers),
		labelMedium = base.labelMedium.merge(Numbers),
		labelSmall = base.labelSmall.merge(Numbers),
	)
	androidx.compose.runtime.CompositionLocalProvider(LocalMetricColors provides metricColors(scheme, dark)) {
		MaterialTheme(colorScheme = scheme, typography = typography, content = content)
	}
}
