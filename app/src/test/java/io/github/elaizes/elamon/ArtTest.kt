package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals

class ArtTest {
	@Test
	fun coversReadAtAFraction() {
		// A 72 dp cover on a 3x screen is 216 px: a 3000 px image is read at an eighth, never smaller than that.
		assertEquals(8, sampleSize(3000, 3000, 216))
		assertEquals(4, sampleSize(1000, 1000, 216))
		assertEquals(2, sampleSize(500, 500, 216))
		assertEquals(1, sampleSize(400, 400, 216))
		assertEquals(1, sampleSize(100, 100, 216))
		// The shorter side decides, so a wide image still fills the square.
		assertEquals(1, sampleSize(3000, 300, 216))
		assertEquals(1, sampleSize(3000, 3000, 0))
	}
}
