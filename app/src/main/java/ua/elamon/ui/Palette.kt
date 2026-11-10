package ua.elamon.ui

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp

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

/** How far a light page moves from its background toward the cards' grey, so white cards stand out. */
private const val PAGE_TINT = 0.35f

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
	background = lerp(background, surfaceContainerHigh, PAGE_TINT),
	surface = lerp(surface, surfaceContainerHigh, PAGE_TINT),
)

/** [base] (the wallpaper's colours or the fallback) as the app shows it: black or tinted as set. */
internal fun finishScheme(base: ColorScheme, dark: Boolean, pureBlack: Boolean): ColorScheme = when {
	dark && pureBlack -> base.pureBlack()
	!dark -> base.cardsOnTint()
	else -> base
}

/** The app's own colours, where the wallpaper's aren't used. */
internal fun fallbackColorScheme(dark: Boolean, pureBlack: Boolean): ColorScheme =
	finishScheme(if (dark) DarkFallback else LightFallback, dark, pureBlack)

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

internal fun metricColors(dark: Boolean) = if (dark) {
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
