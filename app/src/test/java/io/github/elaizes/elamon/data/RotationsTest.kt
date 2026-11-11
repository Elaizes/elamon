package io.github.elaizes.elamon.data

import java.io.File
import java.util.Base64
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.builtins.ListSerializer

/**
 * Following a computer's renewed certificate, against three rotations elamond made and signed
 * (elamond/fixtures/rotations.json): from the certificate pinned to the one presented, every step
 * signed with the key of the one before, or nothing.
 */
class RotationsTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val chain: List<Rotation> = ProtocolJson.decodeFromString(ListSerializer(Rotation.serializer()), File(dir, "rotations.json").readText())
	private val first = chain.first().oldFp
	private val last = chain.last().newFp

	@Test
	fun theServicesChainLeadsFromTheFirstCertificateToTheLast() {
		assertEquals(3, chain.size)
		assertEquals(last, Rotations.follow(chain, first, last))
		// Pinned in any case: fingerprints are compared as the service writes them.
		assertEquals(last, Rotations.follow(chain, first.uppercase(), last.uppercase()))
	}

	@Test
	fun aPhoneThatSawALaterCertificateFollowsFromThere() {
		assertEquals(last, Rotations.follow(chain, chain[1].oldFp, last))
		assertEquals(last, Rotations.follow(chain, chain[2].oldFp, last))
		// The service presents the second while it serves the rotations to the third: that's enough.
		assertEquals(chain[1].newFp, Rotations.follow(chain, first, chain[1].newFp))
	}

	@Test
	fun aSignatureOverSomethingElseIsRefused() {
		val swapped = chain.toMutableList().apply { this[1] = this[1].copy(signature = chain[0].signature) }
		assertNull(Rotations.follow(swapped, first, last))
		val sig = Base64.getDecoder().decode(chain[2].signature).apply { this[size - 1] = (this[size - 1] + 1).toByte() }
		val flipped = chain.toMutableList().apply { this[2] = this[2].copy(signature = Base64.getEncoder().encodeToString(sig)) }
		assertNull(Rotations.follow(flipped, first, last))
		// Signed, but the text changed since: another time, or another certificate after it.
		assertNull(Rotations.follow(chain.toMutableList().apply { this[0] = this[0].copy(t = this[0].t + 1) }, first, last))
		val elsewhere = chain.toMutableList().apply { this[2] = this[2].copy(newFp = "0".repeat(Limits.FINGERPRINT_HEX)) }
		assertNull(Rotations.follow(elsewhere, first, "0".repeat(Limits.FINGERPRINT_HEX)))
	}

	@Test
	fun aBrokenLinkLeadsNowhere() {
		assertNull(Rotations.follow(chain.filterIndexed { i, _ -> i != 1 }, first, last))
		// A step whose old certificate isn't the one it says it follows.
		val other = chain.toMutableList().apply { this[1] = this[1].copy(oldCert = chain[0].oldCert) }
		assertNull(Rotations.follow(other, first, last))
		// Out of order.
		assertNull(Rotations.follow(chain.reversed(), first, last))
	}

	@Test
	fun theChainHasToEndAtTheCertificatePresented() {
		assertNull(Rotations.follow(chain, first, "f".repeat(Limits.FINGERPRINT_HEX)))
		// Nothing to follow from a certificate the chain doesn't know, or to the one already pinned.
		assertNull(Rotations.follow(chain, "f".repeat(Limits.FINGERPRINT_HEX), last))
		assertNull(Rotations.follow(chain, last, last))
		assertNull(Rotations.follow(emptyList(), first, last))
	}

	@Test
	fun aFormatThisAppDoesNotKnowIsNotFollowed() {
		val newer = chain.map { it.copy(format = "elamon-rotate-v2") }
		assertNull(Rotations.follow(newer, first, last))
		assertNull(Rotations.follow(chain.toMutableList().apply { this[1] = this[1].copy(format = "") }, first, last))
	}

	@Test
	fun brokenFieldsAreRefusedNotThrown() {
		assertNull(Rotations.follow(chain.toMutableList().apply { this[0] = this[0].copy(oldCert = "not base64!") }, first, last))
		assertNull(Rotations.follow(chain.toMutableList().apply { this[0] = this[0].copy(signature = "") }, first, last))
		assertNull(Rotations.follow(chain.toMutableList().apply { this[0] = this[0].copy(oldCert = "AAAA") }, first, last))
	}

	// ---- what the app does on meeting another certificate ----------------------------------

	private val changed = ElamondException(ElamondException.Kind.CertificateChanged)

	@Test
	fun aRenewedCertificateIsPinnedAndUsed() = runBlocking {
		val asked = mutableListOf<String>()
		var pinned: String? = null
		val reached = reachFollowing(
			pinned = "old",
			reach = { pin -> asked += pin; if (pin == "old") throw changed else "reached with $pin" },
			renewed = { "new" },
			repin = { pinned = it },
		)
		assertEquals("reached with new", reached)
		assertEquals(listOf("old", "new"), asked)
		assertEquals("new", pinned)
	}

	@Test
	fun withoutAChainTheCertificateChanged() = runBlocking {
		var repinned = false
		val e = assertFailsWith<ElamondException> {
			reachFollowing<String>(pinned = "old", reach = { throw changed }, renewed = { null }, repin = { repinned = true })
		}
		assertEquals(ElamondException.Kind.CertificateChanged, e.kind)
		// A chain that only leads back to the pin is none.
		assertFailsWith<ElamondException> { reachFollowing<String>(pinned = "old", reach = { throw changed }, renewed = { "old" }, repin = { repinned = true }) }
		assertEquals(false, repinned)
	}

	@Test
	fun otherwiseNothingIsAsked() = runBlocking {
		var renewals = 0
		assertEquals("ok", reachFollowing(pinned = "p", reach = { "ok" }, renewed = { renewals++; null }, repin = {}))
		assertNull(reachFollowing<String>(pinned = "p", reach = { null }, renewed = { renewals++; null }, repin = {}))
		val e = assertFailsWith<ElamondException> {
			reachFollowing<String>(pinned = "p", reach = { throw ElamondException(ElamondException.Kind.Unauthorized) }, renewed = { renewals++; null }, repin = {})
		}
		assertEquals(ElamondException.Kind.Unauthorized, e.kind)
		assertEquals(0, renewals)
	}
}
