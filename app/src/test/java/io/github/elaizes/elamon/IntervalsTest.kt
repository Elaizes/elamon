package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals

class IntervalsTest {
	@Test
	fun effectiveInterval() {
		// Never faster than the service reads, never slower than a minute.
		assertEquals(2000, Intervals.effective(1000, 2000))
		assertEquals(5000, Intervals.effective(5000, 1000))
		assertEquals(500, Intervals.effective(500, null))
		assertEquals(60_000, Intervals.effective(90_000, null))
	}
}
