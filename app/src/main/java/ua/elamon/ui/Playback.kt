package ua.elamon.ui

/**
 * Where a track is at [nowMs]: [positionUs] as the computer read it at [sampledAtMs], moved on by
 * the time since at [rate] while [playing], and kept within its [lengthUs].
 */
fun playbackPosition(positionUs: Long?, lengthUs: Long, sampledAtMs: Long, nowMs: Long, playing: Boolean, rate: Double = 1.0): Long {
	val end = lengthUs.coerceAtLeast(0)
	val start = (positionUs ?: 0).coerceIn(0, end)
	if (!playing) return start
	val elapsedUs = ((nowMs - sampledAtMs).coerceAtLeast(0) * 1000.0 * rate).toLong()
	return (start + elapsedUs).coerceIn(0, end)
}
