package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull

class PairLinkTest {
	private val id = "0123456789abcdef0123456789abcdef"
	private val fp = "ab".repeat(32)

	private val good = mapOf(
		"v" to "1",
		"id" to id,
		"name" to "deskpc",
		"port" to "47823",
		"fp" to fp,
		"code" to "042917",
		"h" to "192.168.200.101,100.64.0.1",
	)

	private fun parse(params: Map<String, String?>, scheme: String? = "elamon", host: String? = "pair") =
		PairTarget.fromQuery(scheme, host) { params[it] }

	private fun changed(vararg change: Pair<String, String?>) = parse(good + change)

	@Test
	fun theLinkElamondPrints() {
		val t = assertIs<PairLink.Target>(parse(good)).target
		assertEquals(listOf("192.168.200.101", "100.64.0.1"), t.hosts)
		assertEquals(47823, t.port)
		assertEquals("deskpc", t.name)
		assertEquals(id, t.id)
		assertEquals(fp, t.fingerprint)
		assertEquals("042917", t.code)
		assertNull(t.replaces)
	}

	@Test
	fun otherLinksAreNotPairingLinks() {
		assertNull(parse(good, scheme = "https"))
		assertNull(parse(good, host = "settings"))
		assertNull(parse(good, scheme = null, host = null))
	}

	@Test
	fun versions() {
		assertIs<PairLink.Newer>(changed("v" to "2"))
		assertIs<PairLink.Newer>(changed("v" to "17"))
		assertIs<PairLink.Invalid>(changed("v" to null))
		assertIs<PairLink.Invalid>(changed("v" to "0"))
		assertIs<PairLink.Invalid>(changed("v" to "one"))
		assertIs<PairLink.Invalid>(changed("v" to "1.0"))
	}

	@Test
	fun fieldsChecked() {
		assertIs<PairLink.Invalid>(changed("id" to null))
		assertIs<PairLink.Invalid>(changed("id" to id.uppercase()))
		assertIs<PairLink.Invalid>(changed("id" to id.drop(1)))
		assertIs<PairLink.Invalid>(changed("id" to "$id&x"))
		assertIs<PairLink.Invalid>(changed("fp" to null))
		assertIs<PairLink.Invalid>(changed("fp" to fp.drop(2)))
		assertIs<PairLink.Invalid>(changed("fp" to fp.dropLast(1) + "g"))
		assertIs<PairLink.Invalid>(changed("code" to null))
		assertIs<PairLink.Invalid>(changed("code" to "12345"))
		assertIs<PairLink.Invalid>(changed("code" to "1234567"))
		assertIs<PairLink.Invalid>(changed("code" to "12a456"))
		assertIs<PairLink.Invalid>(changed("code" to "12 3456"))
		// Digits of other scripts aren't the code's digits.
		assertIs<PairLink.Invalid>(changed("code" to "١٢٣٤٥٦"))
		assertIs<PairLink.Invalid>(changed("port" to "0"))
		assertIs<PairLink.Invalid>(changed("port" to "70000"))
		assertIs<PairLink.Invalid>(changed("port" to "80a"))
		// A fingerprint in capitals is the same fingerprint.
		assertEquals(fp, assertIs<PairLink.Target>(changed("fp" to fp.uppercase())).target.fingerprint)
		assertEquals(47823, assertIs<PairLink.Target>(changed("port" to null)).target.port)
		assertEquals(8443, assertIs<PairLink.Target>(changed("port" to "8443")).target.port)
	}

	@Test
	fun hostsChecked() {
		assertIs<PairLink.Invalid>(changed("h" to null))
		assertIs<PairLink.Invalid>(changed("h" to ""))
		assertIs<PairLink.Invalid>(changed("h" to " , "))
		for (bad in listOf("evil@192.168.200.1", "192.168.200.1/x", "pc#a", "pc?b", "my pc", "192.168.200.1:80", "300.1.1.1")) {
			assertIs<PairLink.Invalid>(changed("h" to "192.168.200.101,$bad"), bad)
		}
		assertIs<PairLink.Invalid>(changed("h" to (1..9).joinToString(",") { "10.0.0.$it" }))
		assertEquals(listOf("fd00::1", "pc.local"), assertIs<PairLink.Target>(changed("h" to "fd00::1, [fd00::1], pc.local")).target.hosts)
	}

	@Test
	fun namesComeAfterTheAddresses() {
		// What elamond 0.5 writes with a name from its settings and its Tailscale name.
		val h = "192.168.200.101,100.64.0.1,pc.example.net,deskpc.tail1234.ts.net"
		val t = assertIs<PairLink.Target>(changed("h" to h)).target
		assertEquals(listOf("192.168.200.101", "100.64.0.1", "pc.example.net", "deskpc.tail1234.ts.net"), t.hosts)
		// Eight in all is what the app reads, names counted.
		val eight = (1..6).joinToString(",") { "10.0.0.$it" } + ",pc.example.net,pc.tail1234.ts.net"
		assertEquals(8, assertIs<PairLink.Target>(changed("h" to eight)).target.hosts.size)
		assertIs<PairLink.Invalid>(changed("h" to "$eight,pc.local"))
		// A trailing dot, as DNS writes names, reads as the name.
		assertEquals(listOf("pc.example.net."), assertIs<PairLink.Target>(changed("h" to "pc.example.net.")).target.hosts)
		for (bad in listOf("pc..example", "-pc.example", "pc_1.example", "pc.123")) {
			assertIs<PairLink.Invalid>(changed("h" to "192.168.200.101,$bad"), bad)
		}
	}

	@Test
	fun aPairedComputerKeepsItsNamesAndTriesWhatWorkedFirst() {
		val t = assertIs<PairLink.Target>(changed("h" to "192.168.200.101,100.64.0.1,deskpc.tail1234.ts.net")).target
		// Reached by its name: everything kept in the link's order.
		val hosts = t.hostsToKeep("deskpc.tail1234.ts.net")
		assertEquals(listOf("192.168.200.101", "100.64.0.1", "deskpc.tail1234.ts.net"), hosts)
		val m = Machine(id = id, name = "deskpc", hosts = hosts, port = 47823, fingerprint = fp, lastHost = "deskpc.tail1234.ts.net")
		assertEquals(listOf("deskpc.tail1234.ts.net", "192.168.200.101", "100.64.0.1"), m.hostsToTry())
		assertEquals(listOf("100.64.0.1", "192.168.200.101", "deskpc.tail1234.ts.net"), m.copy(lastHost = "100.64.0.1").hostsToTry())
		assertEquals(hosts, m.copy(lastHost = null).hostsToTry())
		// Added by a name typed in: that name.
		val typed = PairTarget(hosts = listOf("pc.example.net"), port = 47823, name = null, id = null, fingerprint = null)
		assertEquals(listOf("pc.example.net"), typed.hostsToKeep("pc.example.net"))
		assertEquals(listOf("pc.example.net"), parseAddress("pc.example.net", DEFAULT_PORT)?.let { listOf(it.first) })
	}

	@Test
	fun names() {
		assertNull(assertIs<PairLink.Target>(changed("name" to null)).target.name)
		assertNull(assertIs<PairLink.Target>(changed("name" to "  ")).target.name)
		assertEquals("ab", assertIs<PairLink.Target>(changed("name" to "a\nb\u0007")).target.name)
		assertEquals(64, assertIs<PairLink.Target>(changed("name" to "x".repeat(200))).target.name?.length)
		// Any web page can open a link: its name can't turn the words around it.
		assertEquals("My PCgnp.exe", assertIs<PairLink.Target>(changed("name" to "My PC\u202Egnp.exe")).target.name)
	}

	private fun machine(id: String) = Machine(id = id, name = id, hosts = listOf("h"), port = 1, fingerprint = "f", token = "t")

	@Test
	fun pairingAgainKeepsThePlace() {
		val list = listOf(machine("a"), machine("b"), machine("c"))
		// The same computer: in its own place, with the new token.
		val again = machine("b").copy(token = "new")
		assertEquals(listOf("a", "b", "c"), withMachine(list, again).map { it.id })
		assertEquals("new", withMachine(list, again)[1].token)
		// A reset computer with a new id takes the old entry's place, and the old entry is gone.
		assertEquals(listOf("a", "d", "c"), withMachine(list, machine("d"), replacing = "b").map { it.id })
		// Nothing to replace: added at the end.
		assertEquals(listOf("a", "b", "c", "d"), withMachine(list, machine("d")).map { it.id })
		assertEquals(listOf("a", "b", "c", "d"), withMachine(list, machine("d"), replacing = "gone").map { it.id })
		// The new id already paired too (both entries go, one stays, where the first was).
		assertEquals(listOf("a", "c"), withMachine(list, machine("a"), replacing = "b").map { it.id })
	}

	@Test
	fun aScreenOfAReplacedEntryFindsTheOneInItsPlace() {
		val exists = setOf("c", "x")::contains
		// Paired again as b, then again as c: the screen of a goes to c.
		assertEquals("c", successorOf("a", mapOf("a" to "b", "b" to "c"), exists))
		// Never replaced, or replaced by one since forgotten: nowhere to go.
		assertNull(successorOf("a", emptyMap(), exists))
		assertNull(successorOf("a", mapOf("a" to "b"), exists))
		// A trail going round in a circle ends.
		assertNull(successorOf("a", mapOf("a" to "b", "b" to "a"), exists))
	}
}
