package io.github.elaizes.elamon.data

import java.security.Signature
import java.security.cert.CertificateFactory
import java.security.interfaces.ECPublicKey
import java.util.Base64
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * One step from an older certificate of the computer's to the one after it, signed with the older
 * one's key by `elamond rotate-cert`: what `GET /api/v1/identity/rotations` lists, oldest first.
 */
@Serializable
data class Rotation(
	/** The version of the signed text; this app follows [Rotations.FORMAT] only. */
	val format: String,
	@SerialName("old_fp") val oldFp: String,
	@SerialName("new_fp") val newFp: String,
	/** Unix seconds. */
	val t: Long,
	/** The old certificate, DER in base64: its public key checks [signature]. */
	@SerialName("old_cert") val oldCert: String,
	/** ECDSA P-256 with SHA-256 over [Rotations.signedText], DER in base64. */
	val signature: String,
)

/**
 * How a paired computer's new certificate is trusted without pairing again: every step from the
 * certificate the phone pinned to the one the computer presents now has to be signed with the key
 * of the certificate before it. Those keys are deleted once they've signed, so nobody else can make
 * such a chain, and a phone that missed a rotation or two still follows.
 */
object Rotations {
	const val PATH = "/api/v1/identity/rotations"
	const val FORMAT = "elamon-rotate-v1"

	private val HEX_FINGERPRINT = Regex("[0-9a-f]{${Limits.FINGERPRINT_HEX}}")

	/** P-256's field, in bits. */
	private const val P256_BITS = 256

	/** The exact bytes a step's signature covers. */
	fun signedText(r: Rotation): ByteArray = "${r.format}\n${r.oldFp}\n${r.newFp}\n${r.t}".toByteArray(Charsets.UTF_8)

	/**
	 * The fingerprint to pin in place of [pinned], when [chain] leads from it to [presented], the
	 * certificate the computer presented while it was read: from the step away from [pinned] on,
	 * each signed with its old certificate's key and starting where the one before ended. Null when
	 * it doesn't, whatever the reason.
	 */
	fun follow(chain: List<Rotation>, pinned: String, presented: String): String? {
		var at = pinned.lowercase()
		val target = presented.lowercase()
		val start = chain.indexOfFirst { it.oldFp == at }
		if (start < 0 || at == target) return null
		for (step in chain.drop(start)) {
			if (!signedFrom(step, at)) return null
			at = step.newFp
			// The service may be about to present a newer one than it does yet.
			if (at == target) return at
		}
		return null
	}

	/** [step] leads away from the certificate [from], in a format this app knows, signed with that certificate's key. */
	internal fun signedFrom(step: Rotation, from: String): Boolean {
		if (step.format != FORMAT || step.oldFp != from || !HEX_FINGERPRINT.matches(step.newFp)) return false
		return try {
			val der = Base64.getDecoder().decode(step.oldCert)
			val key = CertificateFactory.getInstance("X.509").generateCertificate(der.inputStream()).publicKey
			ElamondClient.sha256Hex(der) == from &&
				(key as? ECPublicKey)?.params?.curve?.field?.fieldSize == P256_BITS &&
				Signature.getInstance("SHA256withECDSA").run {
					initVerify(key)
					update(signedText(step))
					verify(Base64.getDecoder().decode(step.signature))
				}
		} catch (e: Exception) {
			false
		}
	}
}

/**
 * Reaches a paired computer with [reach] under the pinned fingerprint [pinned]. When it presents
 * another certificate ([reach] throws CertificateChanged), [renewed] asks it how it came to that one:
 * a fingerprint it leads to is handed to [repin] and the computer reached under it. Otherwise the
 * certificate changed as far as the phone can tell, and CertificateChanged is thrown on.
 */
internal suspend fun <T : Any> reachFollowing(
	pinned: String,
	reach: suspend (String) -> T?,
	renewed: suspend (String) -> String?,
	repin: suspend (String) -> Unit,
): T? {
	try {
		return reach(pinned)
	} catch (e: ElamondException) {
		if (e.kind != ElamondException.Kind.CertificateChanged) throw e
	}
	val next = renewed(pinned)?.takeIf { it != pinned } ?: throw ElamondException(ElamondException.Kind.CertificateChanged)
	repin(next)
	return reach(next)
}

/**
 * [ElamondClient.firstReachable] for the paired [machine], following its certificate when the
 * computer renewed it ([Rotations]): then [repin] gets the fingerprint followed and the new one to keep.
 */
suspend fun reachPaired(machine: Machine, repin: suspend (from: String, to: String) -> Unit): Pair<ElamondClient, Hello>? {
	val hosts = machine.hostsToTry()
	return reachFollowing(
		pinned = machine.fingerprint,
		reach = { pin -> ElamondClient.firstReachable(hosts, machine.port, pin, machine.id) },
		renewed = { pin -> ElamondClient.renewedPin(hosts, machine.port, pin, machine.id) },
		repin = { to -> repin(machine.fingerprint, to) },
	)
}
