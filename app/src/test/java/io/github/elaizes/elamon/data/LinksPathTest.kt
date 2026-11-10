package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.test.runTest

/** Links under /api/v1/links, and under /api/v1/services on elamond 0.4, which has no such path. */
class LinksPathTest {
	private val client = ElamondClient("127.0.0.1", DEFAULT_PORT, pin = null)

	private fun answer(status: Int, code: String) = ElamondException.fromResponse(status, """{"error":"$code"}""")

	/** What 0.4 answers for any path it doesn't have. */
	private val unknown = answer(404, "not_found")

	@Test
	fun onlyAnUnknownPathIsAnOlderService() {
		assertTrue(unknown.unknownPath)
		assertFalse(answer(404, "no_such_link").unknownPath)
		assertFalse(answer(404, "no_such_service").unknownPath)
		assertFalse(answer(403, "control_disabled").unknownPath)
		assertFalse(ElamondException(ElamondException.Kind.NotFound).unknownPath)
	}

	@Test
	fun theNewPathFirst() = runTest {
		val asked = mutableListOf<String>()
		assertEquals("ok", client.onLinks("/abc/unit") { asked += it; "ok" })
		assertEquals(listOf("/api/v1/links/abc/unit"), asked)
	}

	@Test
	fun theOldPathWhereTheNewOneIsUnknown() = runTest {
		val asked = mutableListOf<String>()
		val saved = client.onLinks { path ->
			asked += path
			if (path.startsWith(LINKS_PATH)) throw unknown
			"saved"
		}
		assertEquals("saved", saved)
		assertEquals(listOf("/api/v1/links", "/api/v1/services"), asked)
	}

	@Test
	fun anyOtherAnswerIsTheAnswer() = runTest {
		for (e in listOf(answer(404, "no_such_link"), answer(403, "control_disabled"), ElamondException(ElamondException.Kind.Unreachable))) {
			val asked = mutableListOf<String>()
			val thrown = assertFailsWith<ElamondException> { client.onLinks("/x") { asked += it; throw e } }
			assertEquals(e, thrown)
			assertEquals(listOf("/api/v1/links/x"), asked)
		}
	}

	@Test
	fun theOldPathsAnswerStands() = runTest {
		val asked = mutableListOf<String>()
		val gone = answer(404, "no_such_service")
		val thrown = assertFailsWith<ElamondException> {
			client.onLinks("/x") { path -> asked += path; throw if (path.startsWith(LINKS_PATH)) unknown else gone }
		}
		assertEquals(gone, thrown)
		assertEquals(listOf("/api/v1/links/x", "/api/v1/services/x"), asked)
	}
}
