package io.github.elaizes.elamon.data

import android.security.keystore.KeyPermanentlyInvalidatedException
import java.security.GeneralSecurityException
import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** A token sealed with this phone's key: the ciphertext, GCM's tag at its end, and the IV it was sealed with. */
class Sealed(val data: ByteArray, val iv: ByteArray)

/**
 * Seals pairing tokens so that only this app on this phone opens them again. An interface, so the
 * store's tests can stand in for the key store. [aad], GCM's associated data, binds a sealed token
 * to what it is for ([tokenAad]): opened with any other, it doesn't open.
 */
interface TokenCipher {
	/** [plain] sealed under a fresh IV, bound to [aad]; throws when it can't be. */
	fun seal(plain: ByteArray, aad: ByteArray?): Sealed

	/** What [sealed] holds, sealed with [aad]; throws when it can't be opened: the key is gone, the data damaged, or another [aad]. */
	fun open(sealed: Sealed, aad: ByteArray?): ByteArray
}

/**
 * What a computer's token is bound to: its id, under a name of the format. A sealed token copied
 * to another computer's entry (by whoever could write the app's files) opens there no more.
 */
fun tokenAad(id: String): ByteArray = "elamon-token-v1\n$id".toByteArray(Charsets.UTF_8)

/** Where the key comes from: the Android key store in the app, memory in tests. */
interface KeySource {
	/** The key, or null when there is none (yet, or any more). */
	fun existing(): SecretKey?

	/** A new key, in place of any other. */
	fun create(): SecretKey

	fun drop()
}

/** A token couldn't be sealed: the key store doesn't work. */
class KeystoreException(cause: Throwable) : Exception(cause.message, cause)

/** Whether [e], or what caused it, says the key store's key can never be used again: then, and only then, a new one is made. */
fun permanentlyInvalidated(e: Throwable): Boolean =
	generateSequence(e) { it.cause.takeIf { c -> c !== it } }.take(MAX_CAUSES).any { it is KeyPermanentlyInvalidatedException }

private const val MAX_CAUSES = 8

/**
 * AES-GCM under [keys]' key. The cipher picks the IV, a new one for every token. A new key is made
 * only when there is none, or when the key store says the one it has is gone for good
 * ([invalidated]): any other failure is the key store's, perhaps for a moment, and a key dropped
 * for it would lose every pairing; it is thrown as [KeystoreException], the key left as it is.
 */
class GcmTokenCipher(
	private val keys: KeySource,
	private val invalidated: (Throwable) -> Boolean = ::permanentlyInvalidated,
) : TokenCipher {
	@Synchronized
	override fun seal(plain: ByteArray, aad: ByteArray?): Sealed {
		val existing = try {
			keys.existing()
		} catch (e: Exception) {
			if (!invalidated(e)) throw KeystoreException(e)
			null
		}
		val key = existing ?: return sealOrThrow(fresh(), plain, aad)
		return try {
			sealWith(key, plain, aad)
		} catch (e: Exception) {
			// A key that can never seal again can't open either, so what it sealed is lost already;
			// a new one lets pairing again work.
			if (!invalidated(e)) throw KeystoreException(e)
			sealOrThrow(fresh(), plain, aad)
		}
	}

	@Synchronized
	override fun open(sealed: Sealed, aad: ByteArray?): ByteArray {
		val key = keys.existing() ?: throw GeneralSecurityException("no key")
		val c = Cipher.getInstance(TRANSFORMATION)
		c.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(Limits.TOKEN_TAG_BITS, sealed.iv))
		aad?.let(c::updateAAD)
		return c.doFinal(sealed.data)
	}

	private fun fresh(): SecretKey = try {
		keys.drop()
		keys.create()
	} catch (e: Exception) {
		throw KeystoreException(e)
	}

	private fun sealOrThrow(key: SecretKey, plain: ByteArray, aad: ByteArray?): Sealed = try {
		sealWith(key, plain, aad)
	} catch (e: Exception) {
		throw KeystoreException(e)
	}

	private fun sealWith(key: SecretKey, plain: ByteArray, aad: ByteArray?): Sealed {
		val c = Cipher.getInstance(TRANSFORMATION)
		c.init(Cipher.ENCRYPT_MODE, key)
		aad?.let(c::updateAAD)
		val data = c.doFinal(plain)
		return Sealed(data, c.iv ?: throw GeneralSecurityException("no IV"))
	}

	private companion object {
		const val TRANSFORMATION = "AES/GCM/NoPadding"
	}
}
