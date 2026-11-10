package ua.elamon.data

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
	fun names() {
		assertNull(assertIs<PairLink.Target>(changed("name" to null)).target.name)
		assertNull(assertIs<PairLink.Target>(changed("name" to "  ")).target.name)
		assertEquals("ab", assertIs<PairLink.Target>(changed("name" to "a\nb\u0007")).target.name)
		assertEquals(64, assertIs<PairLink.Target>(changed("name" to "x".repeat(200))).target.name?.length)
	}

	private fun server(id: String) = Server(id = id, name = id, hosts = listOf("h"), port = 1, fingerprint = "f", token = "t")

	@Test
	fun pairingAgainKeepsThePlace() {
		val list = listOf(server("a"), server("b"), server("c"))
		// The same computer: in its own place, with the new token.
		val again = server("b").copy(token = "new")
		assertEquals(listOf("a", "b", "c"), withServer(list, again).map { it.id })
		assertEquals("new", withServer(list, again)[1].token)
		// A reset computer with a new id takes the old entry's place, and the old entry is gone.
		assertEquals(listOf("a", "d", "c"), withServer(list, server("d"), replacing = "b").map { it.id })
		// Nothing to replace: added at the end.
		assertEquals(listOf("a", "b", "c", "d"), withServer(list, server("d")).map { it.id })
		assertEquals(listOf("a", "b", "c", "d"), withServer(list, server("d"), replacing = "gone").map { it.id })
		// The new id already paired too (both entries go, one stays, where the first was).
		assertEquals(listOf("a", "c"), withServer(list, server("a"), replacing = "b").map { it.id })
	}
}
