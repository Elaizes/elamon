package io.github.elaizes.elamon.data

/** What pairing with a computer does to the list of computers here, told before the code is sent. */
sealed interface PairCheck {
	/** A computer not here yet, the same one under the same certificate, or the one being paired again: nothing to ask. */
	data object Fine : PairCheck

	/** [existing] is here under the same id with another certificate: pairing forgets it, so it is asked first. */
	data class Replaces(val existing: Machine) : PairCheck
}

/**
 * What pairing with the computer that said hello as [id], presenting the certificate [fingerprint],
 * does to [list]. [replaces] is the computer being paired again from its own screen, after its
 * certificate changed: that one is replaced knowingly.
 */
fun pairCheck(id: String, fingerprint: String, list: List<Machine>, replaces: String?): PairCheck {
	val existing = list.find { it.id == id } ?: return PairCheck.Fine
	if (existing.fingerprint.equals(fingerprint, ignoreCase = true) || existing.id == replaces) return PairCheck.Fine
	return PairCheck.Replaces(existing)
}

/**
 * What pairing again with the computer that said hello as [id] sends as proof that this is the phone
 * paired with it before: [existing], its entry here, and its [token] there ([PreviousPairing]). Only
 * to a computer the phone trusts already: [fingerprint], the certificate the pairing request is
 * pinned to, has to be the one pinned for it, as a new or changed certificate may be another
 * computer's, never to be shown the token. Never with a token that came with Import pairings and is
 * still the other phone's too ([Machine.tokenShared]): the entry is that phone's, and the computer
 * would unpair it. Null when there is nothing to send.
 */
fun previousPairing(id: String, fingerprint: String, existing: Machine?, token: String?): PreviousPairing? {
	val m = existing?.takeIf { it.id == id && it.fingerprint.equals(fingerprint, ignoreCase = true) && !it.tokenShared } ?: return null
	val clientId = m.clientId?.takeIf { it.isNotEmpty() } ?: return null
	return token?.takeIf { it.isNotEmpty() }?.let { PreviousPairing(clientId, it) }
}

/**
 * The id a pairing is kept under: the one the computer said hello with, which its answer may repeat
 * or leave out but not change. Null when it does: the answer isn't the computer's that was checked.
 */
fun pairedId(helloId: String, answeredId: String): String? = if (answeredId.isEmpty() || answeredId == helloId) helloId else null

/** Whether [id] is a service's id as elamond makes them: [Limits.ID_HEX] lowercase hex digits. */
fun isServiceId(id: String): Boolean = SERVICE_ID.matches(id)

private val SERVICE_ID = Regex("[0-9a-f]{${Limits.ID_HEX}}")

/**
 * The start of a certificate's fingerprint as an mDNS announcement gives it ("fp"), lowercase; null
 * when it gives none, an empty one or one that isn't hex: then nothing is checked against it, and
 * the person compares the digits instead.
 */
fun fingerprintPrefix(announced: String?): String? =
	announced?.lowercase()?.takeIf { p -> p.length in 1..Limits.FINGERPRINT_HEX && p.all { it in '0'..'9' || it in 'a'..'f' } }

/** Whether a tap at [now] counts on what pairs, the screen having been ready, or the app brought to the front, since [since]. */
fun tapCounts(now: Long, since: Long): Boolean = now - since >= Limits.TAP_QUIET_MS
