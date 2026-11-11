package io.github.elaizes.elamon.data

import java.security.GeneralSecurityException
import java.security.SecureRandom
import java.util.Base64
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.PBEKeySpec
import javax.crypto.spec.SecretKeySpec
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Pairings exported from one phone for another: the paired computers with their tokens, encrypted
 * with a passphrase, as Settings → Export pairings writes them and Import pairings reads them.
 *
 * A file is three parts, as the service's backups are: the line "elamon-pairings", a line of JSON
 * saying how the rest is kept ([Header]), and the rest: the list ([Contents]) sealed with AES-256-GCM
 * under a key PBKDF2-HMAC-SHA256 makes from the passphrase, with the two lines before it as GCM's
 * associated data, so neither can be changed unnoticed. The tokens are in the list in plain text,
 * and so only ever written sealed.
 */
object PairingsFile {
	const val MAGIC = "elamon-pairings\n"

	/** The format written, and the newest read: one an older app would read wrong raises it. */
	const val FORMAT = 1

	const val KDF = "PBKDF2WithHmacSHA256"
	const val CIPHER = "AES-256-GCM"

	/** PBKDF2's iterations: OWASP's count for HMAC-SHA256 (2023). The file says how many it was made with. */
	const val ITERATIONS = 600_000

	/** The most a file may ask for, so one can't keep the phone busy for minutes. */
	const val ITERATIONS_MAX = 10_000_000

	const val SALT_BYTES = 16
	const val NONCE_BYTES = 12
	const val KEY_BITS = 256
	const val TAG_BITS = 128

	/** The shortest passphrase a file is written with, in characters. */
	const val PASSPHRASE_MIN = 8

	/** The largest file read: thousands of computers would fit. */
	const val FILE_MAX = 4 * 1024 * 1024

	/** The second line. */
	@Serializable
	data class Header(
		val format: Int,
		val kdf: String,
		val iterations: Int,
		/** Base64. */
		val salt: String,
		val cipher: String,
		/** GCM's IV, base64. */
		val nonce: String,
	)

	/** What is sealed: every computer with its token in [Machine.token]. */
	@Serializable
	data class Contents(
		val machines: List<Machine>,
		@SerialName("exported_at") val exportedAt: Long = 0,
		@SerialName("app_version") val appVersion: String = "",
	)

	/** What reading a file came to. */
	sealed interface Opened {
		/** The computers, each with its token in plain text in [Machine.token]. */
		data class Pairings(val machines: List<Machine>) : Opened

		/** The passphrase doesn't open it, or the file was changed: GCM can't tell which. */
		data object WrongPassphrase : Opened

		/** It starts as an export but doesn't read as one. */
		data object Damaged : Opened

		/** Written by a newer app, in a format this one doesn't know. */
		data class Newer(val format: Int) : Opened

		/** Not an export of pairings at all. */
		data object NotPairings : Opened
	}

	private val json = Json {
		ignoreUnknownKeys = true
		explicitNulls = false
	}
	private val b64 = Base64.getEncoder()
	private val b64d = Base64.getDecoder()

	/**
	 * The file for [machines], each with its token in [Machine.token], sealed under [passphrase].
	 * [iterations] other than [ITERATIONS] only for tests.
	 */
	fun seal(
		machines: List<Machine>,
		passphrase: CharArray,
		exportedAt: Long,
		appVersion: String,
		random: SecureRandom = SecureRandom(),
		iterations: Int = ITERATIONS,
	): ByteArray {
		val salt = ByteArray(SALT_BYTES).also(random::nextBytes)
		val nonce = ByteArray(NONCE_BYTES).also(random::nextBytes)
		val header = Header(FORMAT, KDF, iterations, b64.encodeToString(salt), CIPHER, b64.encodeToString(nonce))
		val head = (MAGIC + json.encodeToString(Header.serializer(), header) + "\n").toByteArray(Charsets.UTF_8)
		// Only what a computer is reached and recognised by goes along, never this phone's sealing of its token.
		val plain = json.encodeToString(Contents.serializer(), Contents(machines.map { it.copy(tokenSealed = null, tokenIv = null) }, exportedAt, appVersion))
			.toByteArray(Charsets.UTF_8)
		try {
			val c = Cipher.getInstance(TRANSFORMATION)
			c.init(Cipher.ENCRYPT_MODE, key(passphrase, salt, iterations), GCMParameterSpec(TAG_BITS, nonce))
			c.updateAAD(head)
			return head + c.doFinal(plain)
		} finally {
			plain.fill(0)
		}
	}

	/** What [bytes] hold, opened with [passphrase]. A newer format is told before the passphrase is tried. */
	fun open(bytes: ByteArray, passphrase: CharArray): Opened {
		val header = when (val h = header(bytes)) {
			is Read.Header -> h
			is Read.Not -> return h.why
		}
		val plain = try {
			val c = Cipher.getInstance(TRANSFORMATION)
			c.init(Cipher.DECRYPT_MODE, key(passphrase, header.salt, header.iterations), GCMParameterSpec(TAG_BITS, header.nonce))
			c.updateAAD(bytes, 0, header.length)
			c.doFinal(bytes, header.length, bytes.size - header.length)
		} catch (e: AEADBadTagException) {
			return Opened.WrongPassphrase
		} catch (e: GeneralSecurityException) {
			return Opened.Damaged
		}
		try {
			val contents = runCatching { json.decodeFromString(Contents.serializer(), String(plain, Charsets.UTF_8)) }.getOrNull() ?: return Opened.Damaged
			if (!contents.machines.all(::usable)) return Opened.Damaged
			return Opened.Pairings(contents.machines.map { it.copy(tokenSealed = null, tokenIv = null) })
		} finally {
			plain.fill(0)
		}
	}

	/**
	 * Whether [bytes] look like an export this app reads, from their first two lines alone: what to
	 * tell before asking for the passphrase. Null when they do.
	 */
	fun check(bytes: ByteArray): Opened? = (header(bytes) as? Read.Not)?.why

	private sealed interface Read {
		class Header(val salt: ByteArray, val nonce: ByteArray, val iterations: Int, val length: Int) : Read
		class Not(val why: Opened) : Read
	}

	private fun header(bytes: ByteArray): Read {
		val magic = MAGIC.toByteArray(Charsets.UTF_8)
		if (bytes.size < magic.size || !bytes.copyOfRange(0, magic.size).contentEquals(magic)) return Read.Not(Opened.NotPairings)
		val end = (magic.size until bytes.size).firstOrNull { bytes[it] == '\n'.code.toByte() } ?: return Read.Not(Opened.Damaged)
		val line = String(bytes, magic.size, end - magic.size, Charsets.UTF_8)
		val obj = runCatching { json.parseToJsonElement(line).jsonObject }.getOrNull() ?: return Read.Not(Opened.Damaged)
		val format = runCatching { obj["format"]?.jsonPrimitive?.intOrNull }.getOrNull() ?: return Read.Not(Opened.Damaged)
		if (format > FORMAT) return Read.Not(Opened.Newer(format))
		val h = runCatching { json.decodeFromJsonElement(Header.serializer(), obj) }.getOrNull() ?: return Read.Not(Opened.Damaged)
		if (h.kdf != KDF || h.cipher != CIPHER || h.iterations !in 1..ITERATIONS_MAX) return Read.Not(Opened.Damaged)
		val salt = runCatching { b64d.decode(h.salt) }.getOrNull()?.takeIf { it.isNotEmpty() } ?: return Read.Not(Opened.Damaged)
		val nonce = runCatching { b64d.decode(h.nonce) }.getOrNull()?.takeIf { it.size == NONCE_BYTES } ?: return Read.Not(Opened.Damaged)
		return Read.Header(salt, nonce, h.iterations, end + 1)
	}

	private fun key(passphrase: CharArray, salt: ByteArray, iterations: Int): SecretKeySpec {
		val spec = PBEKeySpec(passphrase, salt, iterations, KEY_BITS)
		try {
			val raw = SecretKeyFactory.getInstance(KDF).generateSecret(spec).encoded
			try {
				return SecretKeySpec(raw, "AES")
			} finally {
				raw.fill(0)
			}
		} finally {
			spec.clearPassword()
		}
	}

	/** A computer an import may add: what the app needs to reach it, recognise it and be let in. */
	private fun usable(m: Machine): Boolean =
		m.id.isNotBlank() && m.id.length <= ID_MAX && m.id.none { it.isISOControl() } &&
			m.fingerprint.length == Limits.FINGERPRINT_HEX && m.fingerprint.all { it in '0'..'9' || it in 'a'..'f' } &&
			m.hosts.isNotEmpty() && m.hosts.size <= HOSTS_MAX && m.hosts.all(Hosts::isValid) &&
			m.port in 1..Limits.PORT_MAX &&
			m.token.isNotEmpty() && m.token.length <= TOKEN_MAX && m.token.none { it.isISOControl() }

	private const val TRANSFORMATION = "AES/GCM/NoPadding"
	private const val ID_MAX = 128
	private const val HOSTS_MAX = 32
	private const val TOKEN_MAX = 512
}

/** What an import did: the list as it is now, and how many computers were new and how many replaced. */
data class Merged(val list: List<Machine>, val added: Int, val replaced: Int)

/**
 * [current] with [imported] in it: a computer with an id already in the list takes that entry's
 * place, others go at the end, in their order. A computer listed twice counts once, as the last.
 */
fun mergeImported(current: List<Machine>, imported: List<Machine>): Merged {
	var list = current
	var added = 0
	var replaced = 0
	for (m in imported.associateBy { it.id }.values) {
		if (list.any { it.id == m.id }) replaced++ else added++
		list = withMachine(list, m)
	}
	return Merged(list, added, replaced)
}
