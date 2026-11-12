package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class NetworkChangesTest {
	private val wifi = NetworkChanges.Seen("wlan", setOf(1))
	private val none = NetworkChanges.Seen(null, emptySet())

	@Test
	fun whereWatchingStartsIsNoChange() {
		assertFalse(NetworkChanges.isChange(null, wifi))
		assertFalse(NetworkChanges.isChange(wifi, wifi.copy()))
	}

	@Test
	fun aNetworkFromNoneOrAnotherOrOverAnotherWayIsOne() {
		// A screen opened in a dead zone: the first network that comes is a reason to look at once.
		assertTrue(NetworkChanges.isChange(none, wifi))
		assertTrue(NetworkChanges.isChange(wifi, none))
		assertTrue(NetworkChanges.isChange(wifi, NetworkChanges.Seen("rmnet", setOf(0))))
		// A VPN over Wi-Fi, then over mobile data: the same network, another way under it.
		assertTrue(NetworkChanges.isChange(NetworkChanges.Seen("tun", setOf(4, 1)), NetworkChanges.Seen("tun", setOf(4, 0))))
	}
}
