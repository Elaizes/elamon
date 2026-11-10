package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals

class LimitsTest {
	@Test
	fun backoffDoublesUpToItsCap() {
		assertEquals(listOf(1000L, 2000L, 4000L, 8000L, 8000L, 8000L), (0..5).map { Limits.backoffMs(it) })
		assertEquals(1000L, Limits.backoffMs(-1))
		assertEquals(8000L, Limits.backoffMs(Int.MAX_VALUE))
	}

	@Test
	fun aStreamWaitsAFewOfItsSamples() {
		assertEquals(15_000, Limits.streamReadTimeoutMs(1000))
		assertEquals(40_000, Limits.streamReadTimeoutMs(10_000))
	}

	@Test
	fun namesAreCutByCharacterAsTheServiceCounts() {
		assertEquals("abc", "abcdef".takeCodePoints(3))
		assertEquals("abc", "abc".takeCodePoints(64))
		// An emoji is one character to the service, two to Kotlin: never split in half.
		assertEquals("a😀", "a😀b".takeCodePoints(2))
		assertEquals("я".repeat(64), "я".repeat(100).takeCodePoints(Limits.LINK_NAME_MAX))
		assertEquals("", "abc".takeCodePoints(0))
	}

	@Test
	fun addressesAreCutByUtf8Bytes() {
		assertEquals("abc", "abcdef".takeUtf8Bytes(3))
		// "я" is two bytes: three of them don't fit in five.
		assertEquals("яя", "яяя".takeUtf8Bytes(5))
		assertEquals("a", "a😀".takeUtf8Bytes(4))
		assertEquals("a😀", "a😀".takeUtf8Bytes(5))
		val long = "x".repeat(600)
		assertEquals(Limits.LINK_URL_MAX, long.takeUtf8Bytes(Limits.LINK_URL_MAX).length)
	}
}
