package ua.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals

class PlaybackTest {
	private val minute = 60_000_000L

	@Test
	fun movesOnWhilePlaying() {
		assertEquals(10_000_000L, playbackPosition(10_000_000, minute, sampledAtMs = 1_000, nowMs = 1_000, playing = true))
		assertEquals(10_500_000L, playbackPosition(10_000_000, minute, sampledAtMs = 1_000, nowMs = 1_500, playing = true))
		// Samples far apart: the position keeps going the whole time, not just a moment.
		assertEquals(40_000_000L, playbackPosition(10_000_000, minute, sampledAtMs = 0, nowMs = 30_000, playing = true))
		assertEquals(20_000_000L, playbackPosition(10_000_000, minute, sampledAtMs = 0, nowMs = 5_000, playing = true, rate = 2.0))
	}

	@Test
	fun staysPutOtherwise() {
		assertEquals(10_000_000L, playbackPosition(10_000_000, minute, sampledAtMs = 0, nowMs = 30_000, playing = false))
		// A clock behind the sample doesn't move it back.
		assertEquals(10_000_000L, playbackPosition(10_000_000, minute, sampledAtMs = 5_000, nowMs = 1_000, playing = true))
	}

	@Test
	fun withinTheTrack() {
		assertEquals(minute, playbackPosition(59_000_000, minute, sampledAtMs = 0, nowMs = 5_000, playing = true))
		assertEquals(0L, playbackPosition(null, minute, sampledAtMs = 0, nowMs = 0, playing = true))
		assertEquals(0L, playbackPosition(-5, minute, sampledAtMs = 0, nowMs = 0, playing = false))
		assertEquals(minute, playbackPosition(2 * minute, minute, sampledAtMs = 0, nowMs = 0, playing = false))
		assertEquals(0L, playbackPosition(5, 0, sampledAtMs = 0, nowMs = 1_000, playing = true))
	}
}
