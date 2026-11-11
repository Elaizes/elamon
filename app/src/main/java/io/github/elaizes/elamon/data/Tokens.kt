package io.github.elaizes.elamon.data

import java.security.GeneralSecurityException
import java.security.InvalidKeyException
import java.security.UnrecoverableKeyException
import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** A token sealed with this phone's key: the ciphertext, GCM's tag at its end, and the IV it was sealed with. */
class Sealed(val data: ByteArray, val iv: ByteArray)

/**
 * Seals pairing tokens so that only this app on this phone opens them again. An interface, so the
 * store's tests can stand in for the key store.
 */
interface TokenCipher {
	/** [plain] sealed under a fresh IV; throws when it can't be. */
	fun seal(plain: ByteArray): Sealed

	/** What [sealed] holds; throws when it can't be opened: the key is gone, or the data damaged. */
	fun open(sealed: Sealed): ByteArray
}

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

/** AES-GCM under [keys]' key. The cipher picks the IV, a new one for every token. */
class GcmTokenCipher(private val keys: KeySource) : TokenCipher {
	/** Sealing with no key, or one that can't be read, makes a new key: what was sealed before is lost either way. */
	@Synchronized
	override fun seal(plain: ByteArray): Sealed {
		val existing = try {
			keys.existing()
		} catch (e: UnrecoverableKeyException) {
			null
		}
		val key = existing ?: return sealWith(fresh(), plain)
		return try {
			sealWith(key, plain)
		} catch (e: InvalidKeyException) {
			// A key that can't seal can't open either, so what it sealed is lost already; a new one
			// lets pairing again work.
			sealWith(fresh(), plain)
		}
	}

	@Synchronized
	override fun open(sealed: Sealed): ByteArray {
		val key = keys.existing() ?: throw GeneralSecurityException("no key")
		val c = Cipher.getInstance(TRANSFORMATION)
		c.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(Limits.TOKEN_TAG_BITS, sealed.iv))
		return c.doFinal(sealed.data)
	}

	private fun fresh(): SecretKey {
		keys.drop()
		return keys.create()
	}

	private fun sealWith(key: SecretKey, plain: ByteArray): Sealed {
		val c = Cipher.getInstance(TRANSFORMATION)
		c.init(Cipher.ENCRYPT_MODE, key)
		val data = c.doFinal(plain)
		return Sealed(data, c.iv ?: throw GeneralSecurityException("no IV"))
	}

	private companion object {
		const val TRANSFORMATION = "AES/GCM/NoPadding"
	}
}
