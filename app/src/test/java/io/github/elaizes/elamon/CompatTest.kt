package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class CompatTest {
	@Test
	fun compareVersions() {
		assertEquals(0, Versions.compare("1.2.0", "1.2.0"))
		assertEquals(0, Versions.compare("1.2", "1.2.0"))
		assertEquals(0, Versions.compare("v1.2.0", "1.2.0"))
		assertEquals(0, Versions.compare("1.2.0-rc1", "1.2.0"))
		assertEquals(0, Versions.compare("1.2.0+git.abc", "1.2"))
		assertTrue(Versions.compare("1.2.1", "1.2.0") > 0)
		assertTrue(Versions.compare("0.10.0", "0.9.9") > 0)
		assertTrue(Versions.compare("1.0", "0.99.99") > 0)
		assertTrue(Versions.compare("1.1.0", "1.2") < 0)
		assertTrue(Versions.compare("1.1.9-beta", "1.2.0") < 0)
	}

	@Test
	fun compareOddVersions() {
		assertEquals(0, Versions.compare("", "0.0.0"))
		assertTrue(Versions.compare("unknown", "0.1") < 0)
		assertTrue(Versions.compare(" 1.2.0 ", "1.1") > 0)
	}

	@Test
	fun appTooOld() {
		assertEquals(Compat.UpdateApp, compatibility("0.6.0", elamondApi = 3, elamondVersion = "1.0.0", minApp = null))
		assertEquals(Compat.UpdateApp, compatibility("0.6.0", elamondApi = 2, elamondVersion = "0.7.0", minApp = "0.7.0"))
	}

	@Test
	fun elamondTooOld() {
		// API 1, elamond 0.5 and before: nothing of it is read, whatever else it says.
		assertEquals(Compat.UpdateElamond, compatibility("0.6.0", elamondApi = 1, elamondVersion = "0.5.0", minApp = "0.5.0"))
		assertEquals(Compat.UpdateElamond, compatibility("0.6.0", elamondApi = 1, elamondVersion = "0.6.0", minApp = null))
		assertEquals(Compat.UpdateElamond, compatibility("0.6.0", elamondApi = 2, elamondVersion = "0.5.9", minApp = null))
		assertEquals("0.6.0", MIN_ELAMOND)
	}

	@Test
	fun inStep() {
		assertEquals(Compat.Ok, compatibility("0.6.0", elamondApi = 2, elamondVersion = MIN_ELAMOND, minApp = "0.6.0"))
		assertEquals(Compat.Ok, compatibility("0.6.1", elamondApi = 2, elamondVersion = "0.6.2", minApp = "0.6"))
	}
}
