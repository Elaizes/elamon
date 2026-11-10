package ua.elamon.ui

import androidx.compose.ui.graphics.Color
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ContrastTest {
	@Test
	fun wcagRatios() {
		assertEquals(21.0, contrastRatio(Color.Black, Color.White), 0.01)
		assertEquals(1.0, contrastRatio(Color.White, Color.White), 0.001)
		// WCAG's own example: #767676 is the lightest grey that reads on white.
		assertEquals(4.54, contrastRatio(Color(0xFF767676), Color.White), 0.01)
	}

	/** Every colour a tag is shown in, in a theme's fallback colours. */
	private fun pillAccents(dark: Boolean, pureBlack: Boolean): Map<String, Color> {
		val scheme = fallbackColorScheme(dark, pureBlack)
		val m = metricColors(dark)
		return mapOf(
			"good" to m.good,
			"warn" to m.warn,
			"hot" to m.hot,
			"primary" to scheme.primary,
			"secondary" to scheme.secondary,
			"tertiary" to scheme.tertiary,
			"outline" to scheme.outline,
		)
	}

	@Test
	fun everyTagReadsOnItsTint() {
		for ((dark, pureBlack) in listOf(false to false, true to false, true to true)) {
			val surfaces = pillSurfaces(fallbackColorScheme(dark, pureBlack))
			for ((name, accent) in pillAccents(dark, pureBlack)) {
				val ink = pillTextColor(accent, surfaces, dark)
				for (bg in pillBackgrounds(accent, surfaces)) {
					val ratio = contrastRatio(ink, bg)
					assertTrue(ratio >= MIN_TEXT_CONTRAST, "$name, ${if (dark) "dark" else "light"}${if (pureBlack) " black" else ""}: $ratio")
				}
			}
		}
	}

	@Test
	fun lightAmberAndGreenNeededDarkening() {
		val surfaces = pillSurfaces(fallbackColorScheme(dark = false, pureBlack = false))
		val m = metricColors(false)
		for (accent in listOf(m.warn, m.good)) {
			// As it was: the accent itself on its tint, about 2.5:1 and 2.8:1.
			assertTrue(pillBackgrounds(accent, surfaces).any { contrastRatio(accent, it) < MIN_TEXT_CONTRAST })
			val ink = pillTextColor(accent, surfaces, dark = false)
			assertTrue(relativeLuminance(ink) < relativeLuminance(accent), "darker, not lighter")
		}
	}

	@Test
	fun colourThatReadsIsLeftAlone() {
		val surfaces = pillSurfaces(fallbackColorScheme(dark = true, pureBlack = false))
		val warn = metricColors(true).warn
		assertEquals(warn, pillTextColor(warn, surfaces, dark = true))
	}
}
