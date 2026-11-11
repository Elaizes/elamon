package io.github.elaizes.elamon

import java.io.File
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ElamondException.Kind

class ErrorsTest {
	private fun answer(status: Int, code: String, message: String? = null): ElamondException =
		ElamondException.fromResponse(status, if (message == null) """{"error":"$code"}""" else """{"error":"$code","message":"$message"}""")

	@Test
	fun kindsFromAnswers() {
		assertEquals(Kind.Unauthorized, answer(401, "unauthorized").kind)
		assertEquals(Kind.ControlDisabled, answer(403, "control_disabled").kind)
		assertEquals(Kind.NotPermitted, answer(403, "not_permitted").kind)
		assertEquals(Kind.NotPermitted, answer(403, "protected_process").kind)
		assertEquals(Kind.WrongCode, answer(403, "wrong_code").kind)
		assertEquals(Kind.NoPairing, answer(403, "no_pairing").kind)
		// elamond 0.5 asks at the computer, which said no or nothing.
		assertEquals(Kind.PairDeclined, answer(403, "pair_declined").kind)
		assertEquals(Kind.Forbidden, answer(403, "something_new").kind)
		assertEquals(Kind.NotAllowed, answer(403, "not_allowed").kind)
		assertTrue(answer(403, "not_allowed").notAllowed)
		assertTrue(!answer(403, "control_disabled").notAllowed)
		assertEquals(Kind.TooManyRequests, answer(429, "too_many_requests").kind)
		assertEquals(Kind.TooManyRequests, ElamondException.fromResponse(429, "").kind)
		assertEquals(Kind.Conflict, answer(409, "process_changed").kind)
		for (code in listOf("not_found", "no_such_process", "no_such_action", "no_such_link", "no_such_service", "no_art")) {
			assertEquals(Kind.NotFound, answer(404, code).kind, code)
		}
		for (code in listOf("unknown_signal", "empty_command", "no_unit", "bad_name", "bad_url", "bad_unit", "bad_scope")) {
			assertEquals(Kind.BadRequest, answer(400, code).kind, code)
		}
		assertEquals(Kind.Failed, answer(500, "storage").kind)
		assertEquals(Kind.Failed, answer(502, "failed", "boom").kind)
		assertEquals(Kind.Protocol, ElamondException.fromResponse(418, "not json").kind)
		assertEquals(Kind.Failed, ElamondException.fromResponse(503, "").kind)
	}

	@Test
	fun answerKeepsCodeAndStatus() {
		val e = answer(404, "no_such_process")
		assertEquals("no_such_process", e.code)
		assertEquals(404, e.status)
		assertEquals("boom", answer(502, "failed", "boom").message)
		assertEquals("HTTP 418", ElamondException.fromResponse(418, "").message)
	}

	@Test
	fun everyCodeHasItsOwnWords() {
		val expected = mapOf(
			(401 to "unauthorized") to R.string.err_unauthorized,
			(403 to "control_disabled") to R.string.err_control_disabled,
			(403 to "not_permitted") to R.string.err_not_permitted,
			(403 to "protected_process") to R.string.err_protected_process,
			(403 to "wrong_code") to R.string.err_wrong_code,
			(403 to "no_pairing") to R.string.err_no_pairing,
			(403 to "pair_declined") to R.string.err_pair_declined,
			(404 to "not_found") to R.string.err_not_found,
			(404 to "no_such_process") to R.string.err_no_such_process,
			(404 to "no_such_action") to R.string.err_no_such_action,
			(404 to "no_such_link") to R.string.err_no_such_link,
			(404 to "no_art") to R.string.err_no_art,
			(400 to "unknown_signal") to R.string.err_unknown_signal,
			(400 to "empty_command") to R.string.err_empty_command,
			(400 to "no_unit") to R.string.err_no_unit,
			(400 to "bad_name") to R.string.err_bad_name,
			(400 to "bad_url") to R.string.err_bad_url,
			(400 to "bad_unit") to R.string.err_bad_unit,
			(400 to "bad_scope") to R.string.err_bad_scope,
			(500 to "storage") to R.string.err_storage,
			(403 to "not_allowed") to R.string.err_not_allowed,
			(429 to "too_many_requests") to R.string.err_too_many_requests,
			(409 to "process_changed") to R.string.err_process_changed,
			(403 to "too_many_phones") to R.string.err_too_many_phones,
			(403 to "rekey_refused") to R.string.err_rekey_refused,
		)
		for ((sent, id) in expected) {
			assertEquals(Message.Res(id), messageFor(answer(sent.first, sent.second)), sent.second)
		}
		assertEquals(expected.values.size, expected.values.toSet().size, "two codes share their words")
		// The old name of no_such_link, which elamond before 0.5 answers with.
		assertEquals(Message.Res(R.string.err_no_such_link), messageFor(answer(404, "no_such_service")))
	}

	@Test
	fun everyCodeTheServiceHasIsKnown() {
		// elamond/fixtures/errors.json: the service's own list of codes with their statuses.
		val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
		val list = Json.parseToJsonElement(File(dir, "errors.json").readText()).jsonArray
		assertTrue(list.isNotEmpty())
		val generic = setOf(R.string.err_code, R.string.err_refused, R.string.err_protocol, R.string.err_unknown)
		for (item in list) {
			val code = item.jsonObject.getValue("error").jsonPrimitive.content
			val status = item.jsonObject.getValue("status").jsonPrimitive.int
			val e = answer(status, code)
			assertNotEquals(Kind.Protocol, e.kind, code)
			val m = messageFor(e)
			assertTrue(m is Message.Res && m.id !in generic, "$code ($status) has no words of its own: $m")
		}
	}

	@Test
	fun failedShowsTheComputersWords() {
		assertEquals(Message.Text("unit not found"), messageFor(answer(502, "failed", "unit not found")))
		assertEquals(Message.Res(R.string.err_failed), messageFor(answer(502, "failed")))
		assertEquals(Message.Res(R.string.err_failed), messageFor(answer(502, "failed", " ")))
	}

	@Test
	fun connectionProblems() {
		assertEquals(Message.Res(R.string.err_unauthorized), messageFor(ElamondException(Kind.Unauthorized)))
		assertEquals(Message.Res(R.string.err_certificate_changed), messageFor(ElamondException(Kind.CertificateChanged)))
		assertEquals(Message.Res(R.string.err_unreachable), messageFor(ElamondException(Kind.Unreachable, "timeout")))
		assertEquals(Message.Res(R.string.err_protocol), messageFor(ElamondException(Kind.Protocol, "Unexpected JSON token")))
		assertNotEquals(messageFor(ElamondException(Kind.Unauthorized)), messageFor(ElamondException(Kind.CertificateChanged)))
	}

	@Test
	fun codesFromNewerServicesShowAsTheyAre() {
		assertEquals(Message.Res(R.string.err_code, listOf("rate_limited")), messageFor(answer(403, "rate_limited")))
		assertEquals(Message.Res(R.string.err_code, listOf("too_long")), messageFor(answer(400, "too_long")))
		assertEquals(Message.Res(R.string.err_refused), messageFor(ElamondException.fromResponse(403, "")))
		// Statuses 0.5 brought in, from a service that gives no code or a newer one.
		assertEquals(Message.Res(R.string.err_too_many_requests), messageFor(ElamondException.fromResponse(429, "")))
		assertEquals(Message.Res(R.string.err_code, listOf("pid_reused")), messageFor(answer(409, "pid_reused")))
	}

	@Test
	fun otherFailures() {
		assertEquals(Message.Res(R.string.err_unknown), messageFor(IOException("x")))
		assertEquals(Message.Res(R.string.err_unknown), messageFor(IllegalStateException()))
	}
}
