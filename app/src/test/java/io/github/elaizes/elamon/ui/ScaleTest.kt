package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import io.github.elaizes.elamon.Scale

/** The app's own scale: its bounds and steps, and the display it makes of the phone's. */
class ScaleTest {
	@Test
	fun theScalesThereAre() {
		assertEquals(listOf(80, 140, 100), listOf(Scale.MIN, Scale.MAX, Scale.DEFAULT))
		// Within the bounds, on a step: the nearest, a half up.
		assertEquals(listOf(80, 80, 140, 140, 100, 100, 105, 115, 120, 125), listOf(0, 79, 141, 1000, 100, 102, 103, 117, 118, 123).map(Scale::of))
		// A slider of 80 to 140 in fives: 13 places, the 11 stops between the ends.
		assertEquals(11, Scale.STOPS)
		assertEquals((Scale.MIN..Scale.MAX step Scale.STEP).count(), Scale.STOPS + 2)
		assertEquals(listOf(0.8f, 1f, 1.4f, 1.4f), listOf(80, 100, 140, 200).map(Scale::factor))
	}

	@Test
	fun theDisplayAtAScale() {
		// A phone of 420 dpi and 411 dp across: at 140%, more pixels to a dp and fewer dp across, as
		// Android's own display size makes it; at 80%, the other way. Text grows with the dp, as the
		// font scale is left alone.
		assertEquals(listOf(336, 420, 588), listOf(80, 100, 140).map { Scale.dpi(420, it) })
		assertEquals(listOf(513, 411, 293), listOf(80, 100, 140).map { Scale.dp(411, it) })
		// The pixels across stay the phone's, within a dp's rounding: 1079 px at 420 dpi, 1077 at 588.
		assertEquals(411 * 420 / 160f, Scale.dp(411, 140) * Scale.dpi(420, 140) / 160f, 588 / 160f)
	}
}
