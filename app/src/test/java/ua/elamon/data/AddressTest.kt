package ua.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class AddressTest {
	@Test
	fun hosts() {
		assertTrue(Hosts.isIpv4("192.168.200.101"))
		assertTrue(Hosts.isIpv4("0.0.0.0"))
		assertFalse(Hosts.isIpv4("256.1.1.1"))
		assertFalse(Hosts.isIpv4("1.2.3"))
		assertFalse(Hosts.isIpv4("1.2.3.4.5"))
		assertFalse(Hosts.isIpv4("1.2.3.-4"))

		assertTrue(Hosts.isIpv6("::1"))
		assertTrue(Hosts.isIpv6("::"))
		assertTrue(Hosts.isIpv6("fd7a:115c:a1e0::1"))
		assertTrue(Hosts.isIpv6("2001:db8:0:0:0:0:2:1"))
		assertTrue(Hosts.isIpv6("::ffff:192.168.200.1"))
		assertFalse(Hosts.isIpv6("1::2::3"))
		assertFalse(Hosts.isIpv6("2001:db8:0:0:0:0:2:1:5"))
		assertFalse(Hosts.isIpv6("12345::1"))
		assertFalse(Hosts.isIpv6("fe80::1%wlan0"))
		assertFalse(Hosts.isIpv6("g::1"))
		assertFalse(Hosts.isIpv6(":1:2:3:4:5:6:7"))

		assertTrue(Hosts.isDnsName("deskpc"))
		assertTrue(Hosts.isDnsName("deskpc.tail1234.ts.net"))
		assertTrue(Hosts.isDnsName("my-pc.local."))
		assertFalse(Hosts.isDnsName("-pc.local"))
		assertFalse(Hosts.isDnsName("pc..local"))
		assertFalse(Hosts.isDnsName("300.1.1.1"))
		assertFalse(Hosts.isDnsName(""))

		for (bad in listOf("user@host", "host/path", "host#x", "host?x", "my host", "host:1", "[::1]", "a".repeat(64) + ".com")) {
			assertFalse(Hosts.isValid(bad), bad)
		}
	}

	@Test
	fun addressesAsTyped() {
		assertEquals("192.168.200.5" to 47823, parseAddress("192.168.200.5", 47823))
		assertEquals("192.168.200.5" to 9000, parseAddress(" 192.168.200.5:9000 ", 47823))
		assertEquals("pc.local" to 47823, parseAddress("https://pc.local/", 47823))
		// http:// is just as likely to be typed; the host isn't IPv6 for its colons.
		assertEquals("192.168.200.5" to 9000, parseAddress("http://192.168.200.5:9000", 47823))
		assertEquals("pc" to 47823, parseAddress("HTTP://pc", 47823))
		assertEquals("fd00::1" to 47823, parseAddress("fd00::1", 47823))
		assertEquals("fd00::1" to 47823, parseAddress("[fd00::1]", 47823))
		assertEquals("fd00::1" to 8443, parseAddress("[fd00::1]:8443", 47823))
		assertEquals("::1" to 1, parseAddress("https://[::1]:1/", 47823))
	}

	@Test
	fun garbageAddresses() {
		for (bad in listOf("", "  ", "host:abc", "host:", "host:0", "host:65536", "[fd00::1]:99999", "[fd00::1]:x", "[fd00::1", "[fd00::1]8443",
			"[pc.local]:80", "fd00::g", "user@pc", "pc/admin", "pc:80/x", "my pc", "1:2:3", "http://", "pc?x=1")) {
			assertNull(parseAddress(bad, 47823), bad)
		}
	}

	@Test
	fun serviceLinks() {
		val h = "192.168.200.5"
		assertEquals("http://192.168.200.5:8080", serviceUrl("http://{host}:8080", h))
		assertEquals("http://192.168.200.5:3000", serviceUrl("{host}:3000", h))
		assertEquals("https://192.168.200.5/admin", serviceUrl("https://{host}/admin", h))
		assertEquals("http://192.168.200.5:8096/web/", serviceUrl("http://localhost:8096/web/", h))
		assertEquals("http://192.168.200.5:81", serviceUrl("http://127.0.0.1:81", h))
		assertEquals("HTTP://192.168.200.5:1", serviceUrl("HTTP://LOCALHOST:1", h))
		assertEquals("http://user:pw@192.168.200.5:81/", serviceUrl("http://user:pw@localhost:81/", h))
		assertEquals("http://192.168.200.5:9090/x", serviceUrl("http://[::1]:9090/x", h))
		assertEquals("http://192.168.200.5", serviceUrl("localhost", h))
	}

	@Test
	fun serviceLinksLeaveTheRestAlone() {
		val h = "192.168.200.5"
		// Only the host is the computer's own name; another loopback address or a path is left as it is.
		assertEquals("http://127.0.0.10:80/", serviceUrl("http://127.0.0.10:80/", h))
		assertEquals("http://192.168.200.5:8080/?next=http://localhost/x#127.0.0.1", serviceUrl("http://localhost:8080/?next=http://localhost/x#127.0.0.1", h))
		assertEquals("http://localhost.example.org/", serviceUrl("http://localhost.example.org/", h))
		assertEquals("https://example.org/localhost", serviceUrl("https://example.org/localhost", h))
		assertEquals("http://192.168.200.5:3000/?u=http://x", serviceUrl("{host}:3000/?u=http://x", h))
		// The placeholder means this address wherever it is.
		assertEquals("http://192.168.200.5:8384/?s=192.168.200.5", serviceUrl("http://{host}:8384/?s={host}", h))
	}

	@Test
	fun serviceLinksOverIpv6() {
		assertEquals("http://[fd00::1]:8384", serviceUrl("http://{host}:8384", "fd00::1"))
		assertEquals("http://[fd00::1]:9090/x", serviceUrl("http://[::1]:9090/x", "fd00::1"))
		assertEquals("http://[fd00::2]:80", serviceUrl("http://[fd00::2]:80", "fd00::1"))
	}
}
