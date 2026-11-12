package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertSame

/** Where a paired computer says it is (`endpoints`), taken into the addresses the app keeps for it. */
class EndpointsTest {
	@Test
	fun theAddressThatWorkedStaysFirstThenWhereTheComputerIsNowThenTheRest() {
		// Paired by code on the local network: the one address it was reached at.
		assertEquals(
			listOf("192.0.2.10", "100.64.0.7", "desk.example.net", "desk.tail0000.ts.net"),
			withEndpoints(listOf("192.0.2.10"), "192.0.2.10", listOf("192.0.2.10", "100.64.0.7", "desk.example.net", "desk.tail0000.ts.net")),
		)
		// Reached last over the VPN, with an address of its own the computer no longer names: kept, after.
		assertEquals(
			listOf("100.64.0.7", "192.0.2.10", "desk.example.net", "deskpc.lan"),
			withEndpoints(listOf("deskpc.lan", "100.64.0.7"), "100.64.0.7", listOf("192.0.2.10", "100.64.0.7", "desk.example.net")),
		)
		// Never reached yet: the computer's order.
		assertEquals(listOf("192.0.2.10", "desk.example.net", "deskpc.lan"), withEndpoints(listOf("deskpc.lan"), null, listOf("192.0.2.10", "desk.example.net")))
	}

	@Test
	fun eachIsTakenOnlyAsAnAddressOrANameAsAPairingLinksAre() {
		val told = listOf(
			"[2001:db8::7]", " 192.0.2.11 ", "fd7a:115c:a1e0::7",
			// Not an address or a name: a port, a path, a scheme, a space, a zone, nothing.
			"192.0.2.12:47823", "desk.example.net/x", "https://desk.example.net", "my desk", "fe80::1%wlan0", "", "[]", "999.0.0.1", "desk..example.net",
		)
		assertEquals(listOf("192.0.2.10", "2001:db8::7", "192.0.2.11", "fd7a:115c:a1e0::7"), withEndpoints(listOf("192.0.2.10"), "192.0.2.10", told))
	}

	@Test
	fun eachIsKeptOnceANameWhateverItsCase() {
		assertEquals(
			listOf("Desk.example.net", "192.0.2.10", "2001:DB8::7"),
			withEndpoints(listOf("Desk.example.net", "192.0.2.10"), "Desk.example.net", listOf("desk.example.net", "192.0.2.10", "2001:DB8::7", "2001:db8::7", "DESK.EXAMPLE.NET")),
		)
	}

	@Test
	fun sixteenAtMostWhereTheComputerIsNowBeforeWhatPairingGave() {
		val paired = (1..8).map { "192.0.2.$it" }
		val now = (1..16).map { "198.51.100.$it" }
		val kept = withEndpoints(paired, "192.0.2.3", now)
		assertEquals(Limits.MACHINE_HOSTS_MAX, kept.size)
		// The one that worked last, then the computer's list as far as it fits; the rest of the old go.
		assertEquals(listOf("192.0.2.3") + now.take(15), kept)
		// A computer that names fewer leaves room for the old.
		val some = withEndpoints(paired, "192.0.2.3", now.take(10))
		assertEquals(listOf("192.0.2.3") + now.take(10) + listOf("192.0.2.1", "192.0.2.2", "192.0.2.4", "192.0.2.5", "192.0.2.6"), some)
		// However many it sends.
		assertEquals(Limits.MACHINE_HOSTS_MAX, withEndpoints(paired, null, (1..10_000).map { "10.0.${it / 250}.${it % 250 + 1}" }).size)
	}

	@Test
	fun nothingToTakeLeavesTheListAsItIs() {
		val hosts = listOf("deskpc.lan", "192.0.2.10")
		// An older service sends none; nothing it sends is an address or a name.
		assertSame(hosts, withEndpoints(hosts, "192.0.2.10", emptyList()))
		assertSame(hosts, withEndpoints(hosts, "192.0.2.10", listOf("not an address", "")))
		// The same as kept: equal, so the store writes nothing.
		assertEquals(listOf("192.0.2.10", "deskpc.lan"), withEndpoints(listOf("192.0.2.10", "deskpc.lan"), "192.0.2.10", listOf("deskpc.lan")))
	}

	private fun store(file: MemoryFile) = MachineStore(file, FakeCipher()).apply {
		put(Machine(id = "a", name = "Desk", hosts = listOf("192.0.2.10"), port = DEFAULT_PORT, fingerprint = "aa", lastHost = "192.0.2.10"), "secret-a")
	}

	@Test
	fun theStoreTakesThemOverThePinnedCertificateOnlyAndWritesOnlyAChange() {
		val file = MemoryFile()
		val store = store(file)
		val writes = file.writes.size
		val now = listOf("192.0.2.10", "100.64.0.7", "desk.tail0000.ts.net")
		store.takeEndpoints("a", "AA", now)
		assertEquals(now, store.get("a")!!.hosts)
		assertEquals(writes + 1, file.writes.size)
		// Told the same again: nothing written.
		store.takeEndpoints("a", "aa", now)
		store.takeEndpoints("a", "aa", emptyList())
		assertEquals(writes + 1, file.writes.size)
		// An answer over another certificate (the computer paired again meanwhile), or for a computer
		// not here, changes nothing.
		store.takeEndpoints("a", "bb", listOf("198.51.100.1"))
		store.takeEndpoints("gone", "aa", listOf("198.51.100.1"))
		assertEquals(now, store.get("a")!!.hosts)
		assertEquals(writes + 1, file.writes.size)
		// The token stays as it was.
		assertEquals("secret-a", store.token(store.get("a")!!))
	}
}
