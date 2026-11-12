package io.github.elaizes.elamon.ui

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlinx.coroutines.test.runTest
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.StreamLine
import io.github.elaizes.elamon.data.UiDoc

/** A computer's card on the home screen: its figures, and what it says when they can't be had. */
class HomeCardTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "ui.json").readText())
	private val data = ProtocolJson.decodeFromString(StreamLine.serializer(), File(fixtures, "stream-data.json").readText()) as DataLine

	/** The ui kept from before the first tab was renamed on the computer. */
	private val kept = ui.copy(
		version = "old",
		tabs = listOf("start"),
		dashboards = ui.dashboards.map { if (it.id == "home") it.copy(id = "start") else it },
	)

	private fun refused(status: Int, code: String) = ElamondException.fromResponse(status, """{"error":"$code"}""")

	/** A computer whose dashboards are [has], and the uis asked for, true where fresh from it. */
	private class Computer(val has: Set<String>, val data: DataLine) {
		val asked = mutableListOf<Boolean>()
		val read = mutableListOf<String>()
		var refusal: (String) -> ElamondException = { ElamondException.fromResponse(404, """{"error":"no_such_dashboard"}""") }

		suspend fun snapshot(tab: String): DataLine {
			read += tab
			if (tab !in has) throw refusal(tab)
			return data.copy(dashboard = tab)
		}
	}

	@Test
	fun theKeptUiWhileItsTabIsThere() = runTest {
		val c = Computer(setOf("home"), data)
		val live = cardReading(ui = { fresh -> c.asked += fresh; ui }, snapshot = c::snapshot)
		assertEquals(listOf("CPU", "Memory", "Temperature"), assertIs<Live.Online>(live).stats.map { it.label })
		assertEquals(listOf(false), c.asked)
	}

	@Test
	fun aRefusedTabHasTheUiReadAgain() = runTest {
		// The kept ui's first tab was renamed on the computer: the snapshot is refused (404), which isn't
		// the computer being offline. The ui is read again, and its first tab gives the figures.
		val c = Computer(setOf("home"), data)
		val live = cardReading(ui = { fresh -> c.asked += fresh; if (fresh) ui else kept }, snapshot = c::snapshot)
		assertEquals(3, assertIs<Live.Online>(live).stats.size)
		assertEquals(listOf(false, true), c.asked)
		assertEquals(listOf("start", "home"), c.read)
	}

	@Test
	fun readAgainOnceOnly() = runTest {
		val c = Computer(emptySet(), data)
		val e = assertFailsWith<ElamondException> { cardReading(ui = { fresh -> c.asked += fresh; kept }, snapshot = c::snapshot) }
		assertEquals("no_such_dashboard", e.code)
		assertEquals(listOf(false, true), c.asked)
		// And it says the computer answers.
		assertEquals(Live.Online(emptyList()), cardError(e))
	}

	@Test
	fun notReadAgainWhenTheUiIsntWhy() = runTest {
		for (refusal in listOf(refused(401, "unauthorized"), refused(429, "too_many_requests"), refused(500, "failed"), ElamondException(ElamondException.Kind.Unreachable))) {
			val c = Computer(emptySet(), data)
			c.refusal = { refusal }
			assertFailsWith<ElamondException> { cardReading(ui = { fresh -> c.asked += fresh; ui }, snapshot = c::snapshot) }
			assertEquals(listOf(false), c.asked)
		}
	}

	@Test
	fun noTabNoFigures() = runTest {
		val c = Computer(setOf("home"), data)
		assertEquals(Live.Online(emptyList()), cardReading(ui = { ui.copy(tabs = emptyList()) }, snapshot = c::snapshot))
		assertEquals(Live.Online(emptyList()), cardReading(ui = { ui.copy(tabs = listOf("gone")) }, snapshot = c::snapshot))
		assertEquals(emptyList(), c.read)
	}

	@Test
	fun whatAFailureSays() {
		assertEquals(Live.Unpaired, cardError(refused(401, "unauthorized")))
		assertEquals(Live.Offline, cardError(ElamondException(ElamondException.Kind.Unreachable, "timeout")))
		assertEquals(Live.Offline, cardError(ElamondException(ElamondException.Kind.CertificateChanged)))
		assertEquals(Live.Offline, cardError(IllegalStateException()))
		// It answered: online, with nothing to show, not "offline".
		assertEquals(Live.Online(emptyList()), cardError(refused(404, "no_such_dashboard")))
		assertEquals(Live.Online(emptyList()), cardError(refused(403, "not_allowed")))
		assertEquals(Live.Online(emptyList()), cardError(refused(500, "failed")))
		assertEquals(Live.Online(emptyList()), cardError(ElamondException(ElamondException.Kind.Protocol, "nested too deep")))
	}
}
