package ua.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import ua.elamon.data.Cpu
import ua.elamon.data.Gpu
import ua.elamon.data.GpuHistory
import ua.elamon.data.History
import ua.elamon.data.Memory
import ua.elamon.data.Net
import ua.elamon.data.Sample

class HistTest {
	private fun sample(t: Long, cpu: Float = t.toFloat(), gpus: List<Gpu> = emptyList(), net: List<Net> = emptyList()) =
		Sample(t = t, uptime = 0, cpu = Cpu(usage = cpu, temp = 50f), memory = Memory(used = 1000), gpus = gpus, net = net)

	private fun Points.values() = (0 until size).map { v(it) }
	private fun Points.times() = (0 until size).map { t(it) }

	@Test
	fun appendsInOrder() {
		val h = Hist()
		assertTrue(h.append(sample(1000, 1f)))
		assertTrue(h.append(sample(2000, 2f)))
		assertEquals(listOf(1000L, 2000L), h.cpu.times())
		assertEquals(listOf(1f, 2f), h.cpu.values())
		assertEquals(listOf(50f, 50f), h.cpuTemp.values())
		assertEquals(2000L, h.lastTime)
	}

	@Test
	fun skipsWhatIsNotNewer() {
		val h = Hist()
		h.append(sample(5000))
		val v = h.version
		assertFalse(h.append(sample(5000)))
		assertFalse(h.append(sample(4000)))
		assertEquals(1, h.size)
		assertEquals(v, h.version)
	}

	@Test
	fun clockTurnedFarBackStartsOver() {
		val h = Hist()
		h.windowMs = 600_000
		h.append(sample(10_000_000))
		h.append(sample(10_001_000))
		// Two hours back, as when the computer's clock went from local time to UTC.
		assertTrue(h.append(sample(10_001_000 - 7_200_000)))
		assertEquals(1, h.size)
	}

	@Test
	fun prunesByTime() {
		val h = Hist()
		h.windowMs = 60_000
		for (s in 0..300) h.append(sample(s * 1000L))
		val from = h.lastTime - h.windowMs
		// One point at or before the window stays, so a line still enters from the left edge.
		assertTrue(h.t(0) <= from)
		assertTrue(h.t(1) > from)
		assertEquals((h.t(0)..h.lastTime step 1000).toList(), h.cpu.times())
		assertEquals(h.cpu.times().map { it.toFloat() }, h.cpu.values())
	}

	@Test
	fun countIsOnlyAMemoryGuard() {
		val h = Hist()
		h.windowMs = 100_000_000
		val n = Hist.MAX_POINTS + 10
		for (i in 0 until n) h.append(sample(i * 10L))
		assertEquals(Hist.MAX_POINTS, h.size)
		assertEquals(100L, h.t(0))
		assertEquals((n - 1) * 10L, h.lastTime)
		assertEquals(10f * 10, h.cpu.v(0))
	}

	@Test
	fun gpusThatComeLaterHaveGapsBefore() {
		val h = Hist()
		h.append(sample(1000))
		h.append(sample(2000, gpus = listOf(Gpu(id = "a", usage = 30f, temp = 60f))))
		h.append(sample(3000, gpus = listOf(Gpu(id = "a", state = "sleeping"))))
		assertEquals(setOf("a"), h.gpuIds)
		val usage = h.gpuUsage("a").values()
		assertTrue(usage[0].isNaN())
		assertEquals(30f, usage[1])
		assertTrue(usage[2].isNaN())
		assertTrue(h.gpuTemp("missing").values().all { it.isNaN() })
	}

	@Test
	fun networkCountsRealLinksOnly() {
		val h = Hist()
		val net = listOf(
			Net(name = "eno1", kind = "wired", rx = 100.0, tx = 10.0),
			Net(name = "wlan0", kind = "wireless", rx = 50.0, tx = 5.0),
			Net(name = "tailscale0", kind = "vpn", rx = 1000.0, tx = 1000.0),
		)
		h.append(sample(1000, net = net))
		assertEquals(150f, h.rx.v(0))
		assertEquals(15f, h.tx.v(0))
	}

	@Test
	fun historyReplacesEverything() {
		val h = Hist()
		h.append(sample(999_000))
		h.replace(
			History(
				t = listOf(1000, 2000, 2000, 3000),
				cpu = listOf(1f, 2f, 99f, 3f),
				cpuTemp = listOf(40f, null, null, 42f),
				mem = listOf(10, 20, 20),
				gpus = listOf(GpuHistory(id = "g", usage = listOf(null, 5f, 5f, 6f))),
			)
		)
		// The repeated moment is dropped; a list shorter than t reads as missing.
		assertEquals(listOf(1000L, 2000L, 3000L), h.cpu.times())
		assertEquals(listOf(1f, 2f, 3f), h.cpu.values())
		assertTrue(h.cpuTemp.v(1).isNaN())
		assertEquals(10f, h.mem.v(0))
		assertTrue(h.mem.v(2).isNaN())
		assertTrue(h.gpuUsage("g").v(0).isNaN())
		assertEquals(6f, h.gpuUsage("g").v(2))
		// Samples carry on from the history's last moment.
		assertFalse(h.append(sample(3000)))
		assertTrue(h.append(sample(4000)))
	}

	@Test
	fun wrapsAroundTheRing() {
		val h = Hist()
		h.windowMs = 60_000
		for (s in 0 until 1000) h.append(sample(s * 1000L))
		assertEquals((h.t(0)..999_000L step 1000).toList(), h.cpu.times())
		assertEquals(999_000f, h.cpu.v(h.size - 1))
	}

	@Test
	fun gapsAreMeasuredAgainstTheUsualStep() {
		val h = Hist()
		h.stepHintMs = 1000
		for (s in 0 until 10) h.append(sample(s * 1000L))
		assertEquals(1000L, h.medianStepMs())
		assertEquals(3000L, h.gapMs)
		// Asking every 5 s from now on: those steps aren't gaps.
		h.stepHintMs = 5000
		assertEquals(15_000L, h.gapMs)
		assertEquals(15_000L, h.cpu.gapMs)
	}

	@Test
	fun versionCountsChanges() {
		val h = Hist()
		val v0 = h.version
		h.append(sample(1000))
		h.replace(History(t = listOf(1L)))
		h.windowMs = 120_000
		assertEquals(v0 + 3, h.version)
	}
}
