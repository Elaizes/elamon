package ua.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class IntervalsTest {
	@Test
	fun parsesSeconds() {
		assertEquals(1000, Intervals.parse("1"))
		assertEquals(500, Intervals.parse("0.5"))
		assertEquals(500, Intervals.parse("0,5"))
		assertEquals(250, Intervals.parse(" 0.25 "))
		assertEquals(1500, Intervals.parse("1.5"))
		assertEquals(1000, Intervals.parse("1.0004"))
		assertEquals(60_000, Intervals.parse("60"))
	}

	@Test
	fun rejectsWhatIsOutOfRange() {
		assertNull(Intervals.parse("0.2"))
		assertNull(Intervals.parse("0"))
		assertNull(Intervals.parse("-1"))
		assertNull(Intervals.parse("60.5"))
		assertNull(Intervals.parse(""))
		assertNull(Intervals.parse("fast"))
		// Never faster than the service reads.
		assertNull(Intervals.parse("1", minMs = 2000))
		assertEquals(2000, Intervals.parse("2", minMs = 2000))
	}

	@Test
	fun effectiveInterval() {
		assertEquals(2000, Intervals.effective(1000, 2000))
		assertEquals(5000, Intervals.effective(5000, 1000))
		assertEquals(500, Intervals.effective(500, null))
	}
}
