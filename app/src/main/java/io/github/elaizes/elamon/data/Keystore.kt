package io.github.elaizes.elamon.data

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/**
 * The key that seals pairing tokens, in the Android key store: AES-256 for GCM, which never leaves
 * it. Not bound to unlocking the phone: the home screen reads tokens without asking.
 *
 * Nor is it made with setUnlockedDeviceRequired(true), which would keep it from opening anything
 * while the phone is locked: work in the background to come (a notification about a computer, a
 * widget) has to reach the computers then, with their tokens. Each token is bound to its computer
 * instead ([tokenAad]), and the app's files stay out of backups and transfers.
 */
object KeystoreKeys : KeySource {
	private const val PROVIDER = "AndroidKeyStore"
	private const val ALIAS = "elamon-tokens"

	private val store: KeyStore by lazy { KeyStore.getInstance(PROVIDER).apply { load(null) } }

	override fun existing(): SecretKey? = store.getKey(ALIAS, null) as? SecretKey

	override fun create(): SecretKey = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER).run {
		init(
			KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
				.setBlockModes(KeyProperties.BLOCK_MODE_GCM)
				.setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
				.setKeySize(Limits.TOKEN_KEY_BITS)
				.setUserAuthenticationRequired(false)
				.build()
		)
		generateKey()
	}

	override fun drop() {
		if (store.containsAlias(ALIAS)) store.deleteEntry(ALIAS)
	}
}
