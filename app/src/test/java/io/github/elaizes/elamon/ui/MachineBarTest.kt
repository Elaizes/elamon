package io.github.elaizes.elamon.ui

import io.github.elaizes.elamon.Conn
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/** What the top bar of a computer's screen names: a page open over the tabs, or the computer. */
class MachineBarTest {
	@Test
	fun aPageOpenIsNamedWhileItShows() {
		assertEquals("disks", pageShown("disks", listOf("disks"), turnedAway = false))
		// Two pages deep, the one on top.
		assertEquals("unit", pageShown("unit", listOf("links", "unit"), turnedAway = false))
		// On a tab, the computer.
		assertNull(pageShown("overview", emptyList(), turnedAway = false))
		assertNull(pageShown(null, emptyList(), turnedAway = false))
	}

	@Test
	fun turnedAwayTheBarIsTheComputers() {
		for (conn in Conn.entries) {
			val shown = pageShown("disks", listOf("disks"), turnsAway(conn))
			// Unpaired or refused while a page was open: the notice is the computer's, not the page's.
			if (conn in setOf(Conn.Unauthorized, Conn.CertificateChanged, Conn.Refused, Conn.Incompatible)) assertNull(shown, conn.name)
			// Offline or reconnecting, the page stays with its last readings, and so does its name.
			else assertEquals("disks", shown, conn.name)
		}
	}
}
