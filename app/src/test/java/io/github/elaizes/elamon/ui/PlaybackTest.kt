package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals

class PlaybackTest {
	private val minute = 60.0

	@Test
	fun movesOnWhilePlaying() {
		assertEquals(10.0, playbackPosition(10.0, minute, sampledAtMs = 1_000, nowMs = 1_000, playing = true))
		assertEquals(10.5, playbackPosition(10.0, minute, sampledAtMs = 1_000, nowMs = 1_500, playing = true))
		// Lines far apart: the position keeps going the whole time, not just a moment.
		assertEquals(40.0, playbackPosition(10.0, minute, sampledAtMs = 0, nowMs = 30_000, playing = true))
		assertEquals(20.0, playbackPosition(10.0, minute, sampledAtMs = 0, nowMs = 5_000, playing = true, rate = 2.0))
	}

	@Test
	fun staysPutOtherwise() {
		assertEquals(10.0, playbackPosition(10.0, minute, sampledAtMs = 0, nowMs = 30_000, playing = false))
		// A clock behind the line doesn't move it back.
		assertEquals(10.0, playbackPosition(10.0, minute, sampledAtMs = 5_000, nowMs = 1_000, playing = true))
	}

	@Test
	fun withinTheTrack() {
		assertEquals(minute, playbackPosition(59.0, minute, sampledAtMs = 0, nowMs = 5_000, playing = true))
		assertEquals(0.0, playbackPosition(null, minute, sampledAtMs = 0, nowMs = 0, playing = true))
		assertEquals(0.0, playbackPosition(-5.0, minute, sampledAtMs = 0, nowMs = 0, playing = false))
		assertEquals(minute, playbackPosition(2 * minute, minute, sampledAtMs = 0, nowMs = 0, playing = false))
		assertEquals(0.0, playbackPosition(5.0, 0.0, sampledAtMs = 0, nowMs = 1_000, playing = true))
	}
}
