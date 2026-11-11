package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class LinkSchemeTest {
	@Test
	fun webPagesAreOpened() {
		for (ok in listOf(
			"http://192.168.200.5:8080",
			"https://pc.example.net/admin",
			"HTTP://PC:1",
			"http://user:pw@192.168.200.5:81/",
			"http://[fd00::1]:9090/x",
			"https://pc.example.net/?next=intent://x",
		)) {
			assertTrue(isWebLink(ok), ok)
		}
	}

	@Test
	fun anythingElseAServiceSendsIsNot() {
		for (bad in listOf(
			"intent://scan/#Intent;scheme=zxing;package=com.example;end",
			"javascript:alert(1)",
			"file:///sdcard/x",
			"content://media/external/x",
			"elamon://pair?v=1",
			"market://details?id=x",
			"ftp://pc/x",
			"data:text/html,x",
			"http:x",
			"http://",
			"http:///path",
			"http://:80/",
			"https://pc .example.net",
			"http://pc‮.example.net",
			"http://pc\n.example.net",
			"",
		)) {
			assertFalse(isWebLink(bad), bad)
		}
	}

	@Test
	fun whatALinkBecomesIsChecked() {
		// A link's address as the phone opens it: a web page, or nothing opened.
		assertTrue(isWebLink(linkUrl("{host}:3000", "192.168.200.5")))
		assertTrue(isWebLink(linkUrl("http://localhost:8096/web/", "fd7a:115c:a1e0::1")))
		assertFalse(isWebLink(linkUrl("intent://x#Intent;package=com.example;end", "192.168.200.5")))
		assertFalse(isWebLink(linkUrl("file:///etc/passwd", "192.168.200.5")))
	}
}
