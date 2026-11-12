package io.github.elaizes.elamon.ui

/**
 * Where a track is at [nowMs], in seconds: [position] as the computer read it at [sampledAtMs],
 * moved on by the time since at [rate] while [playing], and kept within its [length].
 */
fun playbackPosition(position: Double?, length: Double, sampledAtMs: Long, nowMs: Long, playing: Boolean, rate: Double = 1.0): Double {
	val end = length.coerceAtLeast(0.0)
	val start = (position ?: 0.0).coerceIn(0.0, end)
	if (!playing) return start
	val elapsed = (nowMs - sampledAtMs).coerceAtLeast(0) / MS_PER_SECOND * rate
	return (start + elapsed).coerceIn(0.0, end)
}

private const val MS_PER_SECOND = 1000.0
