package ua.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import ua.elamon.data.Temp

class ThresholdsTest {
	@Test
	fun temperatureWithoutLimits() {
		// Red at 95, amber from 75, turning from green at 55.
		assertEquals(Level(Band.Good), tempLevel(50f))
		assertEquals(Level(Band.GoodToWarn, 0f), tempLevel(55f))
		assertEquals(Level(Band.GoodToWarn, 0.5f), tempLevel(65f))
		assertEquals(Level(Band.WarnToHot, 0f), tempLevel(75f))
		assertEquals(Level(Band.WarnToHot, 0.5f), tempLevel(85f))
		assertEquals(Level(Band.Hot), tempLevel(95f))
		assertEquals(Level(Band.Hot), tempLevel(120f))
	}

	@Test
	fun temperatureWithTheSensorsLimits() {
		// A drive that is hot at 70 and critical at 84: red there, not at 95.
		assertEquals(Level(Band.Hot), tempLevel(84f, high = 70f, crit = 84f))
		assertEquals(Band.WarnToHot, tempLevel(84f).band)
		assertEquals(Level(Band.WarnToHot, 0.5f), tempLevel(77f, high = 70f, crit = 84f))
		assertEquals(Level(Band.GoodToWarn, 0.5f), tempLevel(60f, high = 70f, crit = 84f))
		assertEquals(Level(Band.Good), tempLevel(45f, high = 70f, crit = 84f))
		// Only a critical limit: amber starts the usual way below it.
		assertEquals(Level(Band.WarnToHot, 0.5f), tempLevel(90f, crit = 100f))
		// Only a high limit: that is where it turns red.
		assertEquals(Level(Band.Hot), tempLevel(80f, high = 80f))
		// A high limit at the critical one says nothing more: amber the usual way below red.
		assertEquals(Level(Band.WarnToHot, 0.25f), tempLevel(85f, high = 100f, crit = 100f))
		// Amber never starts below 40, whatever the sensor claims.
		assertEquals(Level(Band.WarnToHot, 0.5f), tempLevel(45f, high = 30f, crit = 50f))
	}

	@Test
	fun loadAndBattery() {
		assertEquals(Level(Band.Good), loadLevel(0.5f))
		assertEquals(Level(Band.WarnToHot, 0f), loadLevel(0.75f))
		assertEquals(Band.WarnToHot, loadLevel(0.8f).band)
		assertEquals(Level(Band.Hot), loadLevel(0.9f))
		assertEquals(Level(Band.Hot), batteryLevel(10f))
		assertEquals(Level(Band.Warn), batteryLevel(15f))
		assertEquals(Level(Band.Warn), batteryLevel(29.9f))
		assertEquals(Level(Band.Good), batteryLevel(30f))
	}

	@Test
	fun bars() {
		assertEquals(0.5f, tempBarFraction(50f))
		assertEquals(0.5f, tempBarFraction(42f, crit = 84f))
		assertEquals(0.5f, tempBarFraction(40f, high = 80f))
		assertEquals(1f, tempBarFraction(130f))
		assertTrue(limitNear(70f, 84f))
		assertFalse(limitNear(68f, 84f))
		assertFalse(limitNear(99f, null))
		assertEquals(3000f, fanBarTop(1000))
		assertEquals(5000f, fanBarTop(4000))
		assertEquals(0.5f, processCpuFraction(12.5f))
	}

	@Test
	fun theCpusOwnSensor() {
		val board = Temp("acpitz", "board", null, "temp1", 50f)
		val core = Temp("coretemp", "cpu", null, "Core 0", 60f, high = 100f, crit = 100f)
		val pkg = Temp("coretemp", "cpu", null, "Package id 0", 62f, high = 80f, crit = 100f)
		assertEquals(pkg, cpuSensor(listOf(board, core, pkg), 62f))
		// The service's figure came from somewhere else: no limits are borrowed.
		assertNull(cpuSensor(listOf(board, core, pkg), 70f))
		assertEquals(core, cpuSensor(listOf(board, core), 60f))
		assertEquals(board, cpuSensor(listOf(board), 50f))
		assertNull(cpuSensor(listOf(board, core, pkg), null))
	}
}
