package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertSame
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.elaizes.elamon.Scale

/** The app's own scale: its bounds and steps, and how it multiplies the phone's density. */
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
	fun dpAndSpGrowAlike() {
		val phone = Density(2f, fontScale = 1.5f)
		val big = scaled(phone, Scale.factor(120))
		with(big) {
			assertEquals(24f, 10.dp.toPx(), 0.001f)
			// 10 sp is 15 dp on this phone, then 1.2 times the pixels.
			assertEquals(36f, 10.sp.toPx(), 0.001f)
			assertEquals(1.5f, fontScale)
		}
		// At 100% it is the phone's own.
		assertSame(phone, scaled(phone, Scale.factor(100)))
	}

	@Test
	fun thePhonesOwnWayWithLargeTextIsKept() {
		// A phone that grows large text less than small (Android 14+): 20 sp is 30 dp, not 40, at 2.0.
		val phone = object : Density {
			override val density = 1f
			override val fontScale = 2f

			override fun TextUnit.toDp(): Dp = Dp(if (value >= 20f) value * 1.5f else value * 2f)

			override fun Dp.toSp(): TextUnit = (value / 2f).sp
		}
		with(scaled(phone, Scale.factor(80))) {
			assertEquals(24f, 20.sp.toPx(), 0.001f)
			assertEquals(16f, 10.sp.toPx(), 0.001f)
		}
	}
}
