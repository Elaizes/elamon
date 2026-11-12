package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class PairingTest {
	private val id = "0123456789abcdef0123456789abcdef"
	private val fp = "ab".repeat(32)
	private val otherFp = "cd".repeat(32)

	private fun machine(id: String, fingerprint: String = fp) =
		Machine(id = id, name = "My PC", hosts = listOf("192.168.200.10"), port = DEFAULT_PORT, fingerprint = fingerprint)

	@Test
	fun aNewComputerOrTheSameCertificateReplacesNothingUnasked() {
		assertEquals(PairCheck.Fine, pairCheck(id, fp, emptyList(), replaces = null))
		assertEquals(PairCheck.Fine, pairCheck(id, fp, listOf(machine("f".repeat(32))), replaces = null))
		// The same computer under the same certificate: its token is renewed, nothing lost.
		assertEquals(PairCheck.Fine, pairCheck(id, fp, listOf(machine(id)), replaces = null))
		assertEquals(PairCheck.Fine, pairCheck(id, fp.uppercase(), listOf(machine(id)), replaces = null))
	}

	@Test
	fun anotherCertificateUnderAnIdHereIsAskedAboutFirst() {
		val here = machine(id, otherFp)
		assertEquals(PairCheck.Replaces(here), pairCheck(id, fp, listOf(machine("f".repeat(32)), here), replaces = null))
		// Paired again from another computer's screen: this one is still asked about.
		assertEquals(PairCheck.Replaces(here), pairCheck(id, fp, listOf(here), replaces = "f".repeat(32)))
		// Paired again from its own screen, after its certificate changed: replaced knowingly.
		assertEquals(PairCheck.Fine, pairCheck(id, fp, listOf(here), replaces = id))
	}

	@Test
	fun pairingAgainProvesItIsThePhoneOnlyToTheComputerItTrusts() {
		val here = machine(id).copy(clientId = "0123abcd")
		val proof = PreviousPairing("0123abcd", "token")
		// The computer it has, under the certificate pinned for it, by QR code or by code alike.
		assertEquals(proof, previousPairing(id, fp, here, "token"))
		assertEquals(proof, previousPairing(id, fp.uppercase(), here, "token"))
		// A computer it doesn't have, or not that one: nothing.
		assertNull(previousPairing(id, fp, null, "token"))
		assertNull(previousPairing("f".repeat(32), fp, here, "token"))
		// Another certificate, new or changed, may be another computer: never shown the token.
		assertNull(previousPairing(id, otherFp, here, "token"))
		// Nothing to prove it with: the entry or the token unknown, the key store lost the key.
		assertNull(previousPairing(id, fp, here.copy(clientId = null), "token"))
		assertNull(previousPairing(id, fp, here.copy(clientId = ""), "token"))
		assertNull(previousPairing(id, fp, here, null))
		assertNull(previousPairing(id, fp, here, ""))
		// A token from Import pairings is still the other phone's, whose entry would be unpaired.
		assertNull(previousPairing(id, fp, here.copy(tokenShared = true), "token"))
	}

	@Test
	fun thePairingAnswerMayNotChangeTheId() {
		assertEquals(id, pairedId(id, ""))
		assertEquals(id, pairedId(id, id))
		assertNull(pairedId(id, "f".repeat(32)))
		assertNull(pairedId(id, id.uppercase()))
	}

	@Test
	fun serviceIdsAre32LowercaseHexDigits() {
		assertTrue(isServiceId(id))
		for (bad in listOf("", id.uppercase(), id + "0", id.drop(1), "g" + id.drop(1), "../" + id.drop(3), " $id".take(32))) {
			assertFalse(isServiceId(bad), bad)
		}
	}

	@Test
	fun anEmptyOrBrokenAnnouncedFingerprintIsNone() {
		assertNull(fingerprintPrefix(null))
		assertNull(fingerprintPrefix(""))
		assertNull(fingerprintPrefix("not hex"))
		assertNull(fingerprintPrefix("ab".repeat(33)))
		assertEquals("abcdef0123456789", fingerprintPrefix("ABCDEF0123456789"))
		assertEquals(fp, fingerprintPrefix(fp))
	}

	@Test
	fun tapsTooSoonAfterTheScreenOrTheAppCameAreIgnored() {
		val shown = 50_000L
		assertFalse(tapCounts(shown, shown))
		assertFalse(tapCounts(shown + Limits.TAP_QUIET_MS - 1, shown))
		assertTrue(tapCounts(shown + Limits.TAP_QUIET_MS, shown))
		// A clock that went back doesn't let one through.
		assertFalse(tapCounts(shown - 5_000, shown))
	}

	@Test
	fun namesFromAfarCantPassForOthers() {
		// Right-to-left override: "My PC‮gnp.exe" would read "My PCexe.png".
		assertEquals("My PCgnp.exe", cleanName("My PC‮gnp.exe"))
		// Isolates, embeddings, marks, joiners, byte order marks, soft hyphens: all format characters.
		assertEquals("abcdefgh", cleanName("a⁦b⁩c‪d‏e‍f﻿g­h"))
		// Control characters, line breaks and separators: one line.
		assertEquals("ab cd", cleanName("a\u0000b\r\n c d \u0085"))
		// A lone half of a surrogate pair goes; whole characters stay, whatever the script.
		assertEquals("ab", cleanName("a\uD800b"))
		assertEquals("Робочий 😀 电脑", cleanName("  Робочий 😀 电脑  "))
		// Cut to the service's length, by character.
		assertEquals(Limits.NAME_MAX, cleanName("😀".repeat(100)).codePointCount(0, cleanName("😀".repeat(100)).length))
		assertEquals("", cleanName("‮​\n"))
	}

	@Test
	fun longerTextKeepsItsLinesWhenAsked() {
		assertEquals("one\ntwo", "one‮\ntwo\u0007".withoutControls(keepLineBreaks = true))
		assertEquals("onetwo", "one\ntwo".withoutControls())
	}
}
