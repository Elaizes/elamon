package io.github.elaizes.elamon.data

import java.security.GeneralSecurityException
import java.security.ProviderException
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** The list in memory, with every write kept; [dies] makes the next write never happen, as when the app is stopped. */
private class MemoryFile(var json: String? = null) : MachineFile {
	val writes = mutableListOf<String>()
	var dies = false

	override fun read() = json

	override fun write(json: String) {
		if (dies) {
			dies = false
			throw IllegalStateException("stopped")
		}
		writes += json
		this.json = json
	}
}

/** A key in memory, as the key store keeps one; setting [key] to null is a key store that lost it. */
private class MemoryKeys : KeySource {
	var key: SecretKey? = null
	var created = 0

	override fun existing() = key

	override fun create(): SecretKey = KeyGenerator.getInstance("AES").apply { init(Limits.TOKEN_KEY_BITS) }.generateKey().also {
		key = it
		created++
	}

	override fun drop() {
		key = null
	}
}

/** The real cipher with a key in memory, which can be told to fail as a broken key store would. */
private class FakeCipher(val keys: MemoryKeys = MemoryKeys()) : TokenCipher {
	private val inner = GcmTokenCipher(keys)
	var sealing = true
	var opening = true

	override fun seal(plain: ByteArray): Sealed = if (sealing) inner.seal(plain) else throw ProviderException("key store down")

	override fun open(sealed: Sealed): ByteArray = if (opening) inner.open(sealed) else throw GeneralSecurityException("can't open")
}

/** How 0.4 kept a computer: the token required, other fields left alone. */
@Serializable
private data class OldServer(val id: String, val name: String, val hosts: List<String>, val port: Int, val fingerprint: String, val token: String)

/** How 0.4 read the list: fields it doesn't know are left alone. */
private val OldJson = Json { ignoreUnknownKeys = true }

class MachineStoreTest {
	/** Two computers as an app before 0.5 left them: plain tokens, no client id or role. */
	private val before = """
		[{"id":"a","name":"Test computer","hosts":["192.168.200.101","laptop.lan"],"port":47823,"fingerprint":"aa","token":"secret-a","lastHost":"192.168.200.101","os":"NixOS","addedAt":5},
		 {"id":"b","name":"Server","hosts":["10.0.0.2"],"port":47823,"fingerprint":"bb","token":"secret-b"}]
	""".trimIndent()

	private val secrets = listOf("secret-a", "secret-b", "secret-c", "secret-new")

	private fun machine(id: String) = Machine(id = id, name = id, hosts = listOf("h-$id"), port = DEFAULT_PORT, fingerprint = "f-$id")

	/** Each machine as written: its token field, and whether a sealed one and its IV are there. */
	private fun written(json: String) = Json.parseToJsonElement(json).jsonArray.map { it.jsonObject }

	private fun assertNothingPlain(json: String) {
		for (s in secrets) assertFalse(s in json, "$s written in plain text: $json")
		for (m in written(json)) {
			assertEquals("", m.getValue("token").jsonPrimitive.content)
			assertNotNull(m["tokenSealed"])
			assertNotNull(m["tokenIv"])
		}
	}

	@Test
	fun plainTokensAreSealedOnLoadInOneWrite() {
		val file = MemoryFile(before)
		val store = MachineStore(file, FakeCipher())
		assertEquals(1, file.writes.size)
		assertNothingPlain(file.writes.single())
		assertEquals(listOf("a", "b"), written(file.writes.single()).map { it.getValue("id").jsonPrimitive.content })

		val a = store.get("a")!!
		assertEquals("Test computer", a.name)
		assertEquals(listOf("192.168.200.101", "laptop.lan"), a.hosts)
		assertEquals("192.168.200.101", a.lastHost)
		assertEquals("NixOS", a.os)
		assertEquals(5, a.addedAt)
		assertEquals("secret-a", store.token(a))
		assertEquals("secret-b", store.token(store.get("b")!!))
	}

	@Test
	fun sealedListLoadsWithoutWritingAgain() {
		val file = MemoryFile(before)
		val cipher = FakeCipher()
		MachineStore(file, cipher)
		// Another run: the key store opens what it sealed, and there is nothing to write.
		val again = MachineStore(file, FakeCipher(cipher.keys))
		assertEquals(1, file.writes.size)
		assertEquals("secret-a", again.token(again.get("a")!!))
		assertEquals("secret-b", again.token(again.get("b")!!))
	}

	@Test
	fun stoppedBeforeTheWriteLeavesTheOldListToSealNextTime() {
		val file = MemoryFile(before)
		file.dies = true
		assertFailsWith<IllegalStateException> { MachineStore(file, FakeCipher()) }
		assertEquals(before, file.json)

		val store = MachineStore(file, FakeCipher())
		assertNothingPlain(file.json!!)
		assertEquals("secret-a", store.token(store.get("a")!!))
		assertEquals("secret-b", store.token(store.get("b")!!))
	}

	@Test
	fun aKeyStoreThatCantSealKeepsTheOldTokensWorking() {
		val file = MemoryFile(before)
		val cipher = FakeCipher().apply { sealing = false }
		val store = MachineStore(file, cipher)
		// Nothing to write: the tokens stay as they were, and work.
		assertTrue(file.writes.isEmpty())
		assertEquals("secret-a", store.token(store.get("a")!!))

		// Once it works, the next save seals them.
		cipher.sealing = true
		store.update("a") { it.copy(lastHost = "laptop.lan") }
		assertNothingPlain(file.json!!)
		assertEquals("secret-a", store.token(store.get("a")!!))
		assertEquals("secret-b", store.token(store.get("b")!!))
	}

	@Test
	fun roundTripWithAFreshIvEachTime() {
		val file = MemoryFile()
		val cipher = FakeCipher()
		val store = MachineStore(file, cipher)
		store.put(machine("a"), "secret-c")
		store.put(machine("b"), "secret-c")
		val a = store.get("a")!!
		val b = store.get("b")!!
		assertNotEquals(a.tokenIv, b.tokenIv)
		assertNotEquals(a.tokenSealed, b.tokenSealed)
		assertEquals("", a.token)
		assertEquals("secret-c", store.token(a))

		val again = MachineStore(file, FakeCipher(cipher.keys))
		assertEquals("secret-c", again.token(again.get("a")!!))
		assertEquals("secret-c", again.token(again.get("b")!!))
	}

	@Test
	fun aLostKeyLeavesThePhoneUnpairedUntilPairedAgain() {
		val file = MemoryFile()
		val cipher = FakeCipher()
		MachineStore(file, cipher).put(machine("a"), "secret-c")

		// The key store was reset: the computer stays listed, without a token to send.
		cipher.keys.key = null
		val store = MachineStore(file, FakeCipher(cipher.keys))
		val a = store.get("a")!!
		assertNull(store.token(a))

		// Pairing again seals the new token under a new key.
		store.put(machine("a"), "secret-new", replacing = "a")
		assertEquals(listOf("a"), store.machines.value.map { it.id })
		assertEquals("secret-new", store.token(store.get("a")!!))
		assertEquals("secret-new", MachineStore(file, FakeCipher(cipher.keys)).let { it.token(it.get("a")!!) })
	}

	@Test
	fun aKeyStoreThatCantOpenOrDamagedDataIsUnpairedNotACrash() {
		val file = MemoryFile()
		val cipher = FakeCipher()
		MachineStore(file, cipher).put(machine("a"), "secret-c")
		val broken = FakeCipher(cipher.keys).apply { opening = false }
		val store = MachineStore(file, broken)
		assertNull(store.token(store.get("a")!!))

		val ok = MachineStore(file, FakeCipher(cipher.keys))
		val a = ok.get("a")!!
		assertNull(ok.token(a.copy(tokenSealed = a.tokenSealed!!.reversed())))
		assertNull(ok.token(a.copy(tokenIv = "not base64!")))
		assertNull(ok.token(a.copy(tokenSealed = null)))
		assertNull(ok.token(a.copy(tokenIv = null)))
	}

	@Test
	fun plainTokensAreNeverWrittenAgain() {
		val file = MemoryFile(before)
		val store = MachineStore(file, FakeCipher())
		store.put(machine("c"), "secret-c")
		store.update("a") { it.copy(lastHost = "laptop.lan") }
		// Even a plain token put back in by mistake goes out sealed.
		store.update("b") { it.copy(token = "secret-new") }
		store.put(machine("d"), "secret-new", replacing = "c")
		store.remove("a")
		assertEquals(6, file.writes.size)
		file.writes.forEach(::assertNothingPlain)
		assertEquals("secret-new", store.token(store.get("b")!!))
		assertEquals("secret-new", store.token(store.get("d")!!))
		assertEquals(listOf("b", "d"), store.machines.value.map { it.id })
	}

	@Test
	fun aTokenThatCantBeSealedIsNotSaved() {
		val file = MemoryFile()
		val cipher = FakeCipher()
		val store = MachineStore(file, cipher)
		store.put(machine("a"), "secret-c")
		cipher.sealing = false
		assertFailsWith<KeystoreException> { store.put(machine("b"), "secret-new") }
		assertEquals(1, file.writes.size)
		assertEquals(listOf("a"), store.machines.value.map { it.id })
	}

	@Test
	fun anOlderAppStillReadsTheList() {
		val file = MemoryFile(before)
		MachineStore(file, FakeCipher())
		val old = OldJson.decodeFromString<List<OldServer>>(file.json!!)
		assertEquals(listOf("a", "b"), old.map { it.id })
		// It finds no token, so the computer turns it away and it can pair again.
		assertTrue(old.all { it.token.isEmpty() })
	}

	@Test
	fun gcmSealsUnderAFreshIvAndOpensWhatItSealed() {
		val keys = MemoryKeys()
		val c = GcmTokenCipher(keys)
		val one = c.seal("secret-c".toByteArray())
		val two = c.seal("secret-c".toByteArray())
		assertEquals(1, keys.created)
		assertEquals(12, one.iv.size)
		assertFalse(one.iv.contentEquals(two.iv))
		assertFalse(one.data.contentEquals(two.data))
		assertEquals("secret-c", String(c.open(one)))
		assertEquals("secret-c", String(c.open(two)))

		// Changed or under another IV, it doesn't open.
		assertFailsWith<GeneralSecurityException> { c.open(Sealed(one.data.copyOf().also { it[0] = (it[0] + 1).toByte() }, one.iv)) }
		assertFailsWith<GeneralSecurityException> { c.open(Sealed(one.data, two.iv)) }
	}

	@Test
	fun gcmWithoutItsKeyOpensNothingAndSealsUnderANewOne() {
		val keys = MemoryKeys()
		val c = GcmTokenCipher(keys)
		val old = c.seal("secret-a".toByteArray())
		keys.key = null
		assertFailsWith<GeneralSecurityException> { c.open(old) }
		val new = c.seal("secret-new".toByteArray())
		assertEquals(2, keys.created)
		assertEquals("secret-new", String(c.open(new)))
	}

	@Test
	fun gcmReplacesAKeyThatCantSeal() {
		val keys = MemoryKeys()
		// Not an AES key: the cipher refuses it, as the key store refuses a broken one.
		keys.key = KeyGenerator.getInstance("HmacSHA256").generateKey()
		val c = GcmTokenCipher(keys)
		val s = c.seal("secret-c".toByteArray())
		assertEquals(1, keys.created)
		assertEquals("secret-c", String(c.open(s)))
	}
}
