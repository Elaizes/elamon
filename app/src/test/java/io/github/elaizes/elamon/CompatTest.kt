package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class CompatTest {
	@Test
	fun compareVersions() {
		assertEquals(0, Versions.compare("0.4.0", "0.4.0"))
		assertEquals(0, Versions.compare("0.4", "0.4.0"))
		assertEquals(0, Versions.compare("v0.4.0", "0.4.0"))
		assertEquals(0, Versions.compare("0.4.0-rc1", "0.4.0"))
		assertEquals(0, Versions.compare("0.4.0+git.abc", "0.4"))
		assertTrue(Versions.compare("0.4.1", "0.4.0") > 0)
		assertTrue(Versions.compare("0.10.0", "0.9.9") > 0)
		assertTrue(Versions.compare("1.0", "0.99.99") > 0)
		assertTrue(Versions.compare("0.3.0", "0.4") < 0)
		assertTrue(Versions.compare("0.3.9-beta", "0.4.0") < 0)
	}

	@Test
	fun compareOddVersions() {
		assertEquals(0, Versions.compare("", "0.0.0"))
		assertTrue(Versions.compare("unknown", "0.1") < 0)
		assertTrue(Versions.compare(" 0.4.0 ", "0.3") > 0)
	}

	@Test
	fun appTooOld() {
		assertEquals(Compat.UpdateApp, compatibility("0.4.0", elamondApi = 2, elamondVersion = "1.0.0", minApp = null))
		assertEquals(Compat.UpdateApp, compatibility("0.4.0", elamondApi = 1, elamondVersion = "0.5.0", minApp = "0.5.0"))
		// Both out of date: the app comes first, it's the one that can't read everything.
		assertEquals(Compat.UpdateApp, compatibility("0.2.0", elamondApi = 1, elamondVersion = "0.3.0", minApp = "0.3.0"))
	}

	@Test
	fun elamondTooOld() {
		assertEquals(Compat.UpdateElamond, compatibility("0.4.0", elamondApi = 1, elamondVersion = "0.3.0", minApp = null))
		assertEquals(Compat.UpdateElamond, compatibility("0.4.0", elamondApi = 1, elamondVersion = "0.2.1", minApp = null))
	}

	@Test
	fun inStep() {
		assertEquals(Compat.Ok, compatibility("0.4.0", elamondApi = 1, elamondVersion = "0.4.0", minApp = "0.3.0"))
		assertEquals(Compat.Ok, compatibility("0.4.0", elamondApi = 1, elamondVersion = "0.4.2", minApp = "0.4"))
	}
}
