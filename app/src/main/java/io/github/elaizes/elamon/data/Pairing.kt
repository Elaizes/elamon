package io.github.elaizes.elamon.data

/**
 * What pairing again with the computer that said hello as [id] sends as proof that this is the phone
 * paired with it before: [existing], its entry here, and its [token] there ([PreviousPairing]). Only
 * to a computer the phone trusts already: [fingerprint], the certificate the pairing request is
 * pinned to, has to be the one pinned for it, as a new or changed certificate may be another
 * computer's, never to be shown the token. Null when there is nothing to send.
 */
fun previousPairing(id: String, fingerprint: String, existing: Machine?, token: String?): PreviousPairing? {
	val m = existing?.takeIf { it.id == id && it.fingerprint.equals(fingerprint, ignoreCase = true) } ?: return null
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
