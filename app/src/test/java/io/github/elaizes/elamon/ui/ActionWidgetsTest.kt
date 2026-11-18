package io.github.elaizes.elamon.ui

import io.github.elaizes.elamon.data.ActionResult
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class ActionWidgetsTest {
	@Test
	fun aSliderOfManyStepsHasNoTicks() {
		assertEquals(19, sliderSteps(0.0, 100.0, 5.0))
		assertEquals(100, sliderSteps(0.0, 101.0, 1.0))
		assertEquals(0, sliderSteps(0.0, 100.0, null))
		// 0–1000 by 1: a hundred ticks reached only every tenth value; without them it reaches each.
		assertEquals(0, sliderSteps(0.0, 1000.0, 1.0))
	}

	@Test
	fun aButtonWhoseActionRunsTakesNoTap() {
		assertTrue(takesTap(enabled = true, running = false))
		assertFalse(takesTap(enabled = false, running = false))
		// Tapped again while it runs, it would ask again, and the run would be dropped.
		assertFalse(takesTap(enabled = true, running = true))
	}

	@Test
	fun aButtonShowsHowItEndedOnlyOnceItHas() {
		val done = ActionResult(exitCode = 0, output = "hello")
		assertEquals(done, shownResult(running = false, result = done))
		// Run again: the spinner in the icon's place says so, and the last outcome isn't shown meanwhile.
		assertNull(shownResult(running = true, result = done))
		assertNull(shownResult(running = false, result = null))
	}
}
