package ua.elamon.ui

import androidx.compose.material3.ColorScheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.colorspace.ColorSpaces
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.lerp
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/** WCAG's least contrast for text of ordinary size, like a tag's 12 sp. */
const val MIN_TEXT_CONTRAST = 4.5

/** WCAG's relative luminance of a colour, its alpha ignored: 0 for black, 1 for white. */
fun relativeLuminance(c: Color): Double {
	val s = c.convert(ColorSpaces.Srgb)
	fun linear(v: Float): Double = if (v <= 0.04045f) v / 12.92 else ((v + 0.055) / 1.055).pow(2.4)
	return 0.2126 * linear(s.red) + 0.7152 * linear(s.green) + 0.0722 * linear(s.blue)
}

/** WCAG's contrast ratio of two opaque colours, from 1 (the same) to 21 (black and white). */
fun contrastRatio(a: Color, b: Color): Double {
	val la = relativeLuminance(a)
	val lb = relativeLuminance(b)
	return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
}

/** [accent] moved toward black (white on [dark] themes) the least that reads on every one of [backgrounds]. */
fun readableOn(accent: Color, backgrounds: List<Color>, dark: Boolean, min: Double = MIN_TEXT_CONTRAST): Color {
	fun reads(c: Color) = backgrounds.all { contrastRatio(c, it) >= min }
	if (reads(accent)) return accent
	val toward = if (dark) Color.White else Color.Black
	// Contrast only grows on the way to black (or white) here, so halving finds the least step.
	var lo = 0f
	var hi = 1f
	repeat(SEARCH_STEPS) {
		val mid = (lo + hi) / 2
		if (reads(lerp(accent, toward, mid))) hi = mid else lo = mid
	}
	return lerp(accent, toward, hi)
}

private const val SEARCH_STEPS = 12

/** The surfaces a tag is put on: the page, cards, and the tiles inside cards. */
fun pillSurfaces(scheme: ColorScheme): List<Color> = listOf(scheme.surface, scheme.surfaceContainerLow, scheme.surfaceContainerHigh)

/** What a tag of [accent] looks like under its text: its tint over each of [surfaces]. */
fun pillBackgrounds(accent: Color, surfaces: List<Color>): List<Color> =
	surfaces.map { accent.copy(alpha = Alpha.PILL).compositeOver(it) }

/** A tag's text: its [accent], darker or lighter where that alone wouldn't read on its tint. */
fun pillTextColor(accent: Color, surfaces: List<Color>, dark: Boolean): Color =
	readableOn(accent, pillBackgrounds(accent, surfaces), dark)
