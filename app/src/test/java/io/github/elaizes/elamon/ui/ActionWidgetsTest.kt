package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals

class ActionWidgetsTest {
	@Test
	fun aSliderOfManyStepsHasNoTicks() {
		assertEquals(19, sliderSteps(0.0, 100.0, 5.0))
		assertEquals(100, sliderSteps(0.0, 101.0, 1.0))
		assertEquals(0, sliderSteps(0.0, 100.0, null))
		// 0–1000 by 1: a hundred ticks reached only every tenth value; without them it reaches each.
		assertEquals(0, sliderSteps(0.0, 1000.0, 1.0))
	}
}
