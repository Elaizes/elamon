package io.github.elaizes.elamon.ui

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import io.github.elaizes.elamon.Message
import io.github.elaizes.elamon.R

class ActivityTextTest {
	@Test
	fun agoInTheLargestWholeUnit() {
		assertEquals(Words.Str(R.string.ago_now), agoWords(0))
		assertEquals(Words.Str(R.string.ago_now), agoWords(1))
		assertEquals(Words.Count(R.plurals.ago_seconds, 2), agoWords(2))
		assertEquals(Words.Count(R.plurals.ago_seconds, 59), agoWords(59))
		assertEquals(Words.Count(R.plurals.ago_minutes, 1), agoWords(60))
		assertEquals(Words.Count(R.plurals.ago_minutes, 59), agoWords(3_599))
		assertEquals(Words.Count(R.plurals.ago_hours, 1), agoWords(3_600))
		assertEquals(Words.Count(R.plurals.ago_hours, 23), agoWords(86_399))
		assertEquals(Words.Count(R.plurals.ago_days, 1), agoWords(86_400))
		assertEquals(Words.Count(R.plurals.ago_days, 40), agoWords(40L * 86_400 + 5))
	}

	@Test
	fun okAndRefusalsAndFailures() {
		val ok = logResult("ok")
		assertEquals(Message.Res(R.string.log_ok), ok.words)
		assertTrue(ok.ok)
		assertFalse(ok.refused)
		for (code in listOf("not_allowed", "control_disabled", "too_many_requests", "not_permitted")) {
			val r = logResult(code)
			assertFalse(r.ok, code)
			assertTrue(r.refused, code)
		}
		val failed = logResult("failed")
		assertEquals(Message.Res(R.string.log_failed), failed.words)
		assertFalse(failed.ok || failed.refused)
		assertEquals(Message.Res(R.string.log_not_allowed), logResult("not_allowed").words)
		assertEquals(Message.Res(R.string.log_process_changed), logResult("process_changed").words)
	}

	@Test
	fun aCodeFromANewerServiceShowsAsItIs() {
		val r = logResult("rate_limited")
		assertEquals(Message.Res(R.string.log_other, listOf("rate_limited")), r.words)
		assertFalse(r.ok)
	}

	@Test
	fun everyCodeAnActingRequestCanEndWithHasWords() {
		// elamond/fixtures: every error code, and the log's own results.
		val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
		val errors = Json.parseToJsonElement(File(dir, "errors.json").readText()).jsonArray.map { it.jsonObject.getValue("error").jsonPrimitive.content }
		val logged = Json.parseToJsonElement(File(dir, "log.json").readText()).jsonArray.map { it.jsonObject.getValue("result").jsonPrimitive.content }
		// Pairing and the token aren't acting on the computer, so never in the log.
		val notActing = setOf("unauthorized", "wrong_code", "no_pairing")
		for (code in (errors + logged).toSet() - notActing) {
			assertNotEquals(R.string.log_other, logResult(code).words.id, "$code has no words of its own")
		}
	}
}
