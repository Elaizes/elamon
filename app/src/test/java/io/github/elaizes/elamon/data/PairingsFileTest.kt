package io.github.elaizes.elamon.data

import java.security.SecureRandom
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

class PairingsFileTest {
	/** Cheap enough for tests; the file says how many it was made with, so it opens as any other. */
	private val quick = 1_000

	private fun machine(id: String, token: String = "token-$id", name: String = "Computer $id") = Machine(
		id = id,
		name = name,
		hosts = listOf("192.168.200.101", "laptop.example.net"),
		port = DEFAULT_PORT,
		fingerprint = "ab".repeat(32),
		token = token,
		lastHost = "192.168.200.101",
		os = "NixOS",
		addedAt = 5,
		clientId = "0123abcd",
		role = "admin",
	)

	private val two = listOf(machine("a"), machine("b"))
	private fun pass(s: String = "two words or more") = s.toCharArray()
	private fun seal(machines: List<Machine> = two, passphrase: String = "two words or more", iterations: Int = quick) =
		PairingsFile.seal(machines, pass(passphrase), exportedAt = 7, appVersion = "0.5.0", iterations = iterations)

	private fun text(b: ByteArray) = String(b, Charsets.ISO_8859_1)

	/** The end of the header line: the second line end. */
	private fun headerEnd(b: ByteArray) = b.indices.filter { b[it] == '\n'.code.toByte() }[1]

	@Test
	fun round_trip_with_the_tokens_and_nothing_of_this_phones_sealing() {
		val sealedHere = machine("c").copy(token = "token-c", tokenSealed = "sealed-here", tokenIv = "iv-here")
		val file = seal(two + sealedHere)
		assertTrue(text(file).startsWith("elamon-pairings\n{\"format\":1,\"kdf\":\"PBKDF2WithHmacSHA256\",\"iterations\":1000,"))
		// The tokens and names only ever sealed.
		for (secret in listOf("token-a", "Computer a", "sealed-here", "192.168.200.101")) assertFalse(secret in text(file), secret)
		val opened = assertIs<PairingsFile.Opened.Pairings>(PairingsFile.open(file, pass()))
		assertEquals(two + sealedHere.copy(tokenSealed = null, tokenIv = null), opened.machines)
		// It reads as an export before any passphrase is asked.
		assertNull(PairingsFile.check(file))
	}

	@Test
	fun any_passphrase_in_any_script() {
		val file = seal(passphrase = "пароль із кількох слів")
		assertIs<PairingsFile.Opened.Pairings>(PairingsFile.open(file, pass("пароль із кількох слів")))
		assertEquals(PairingsFile.Opened.WrongPassphrase, PairingsFile.open(file, pass("пароль із кількох слів ")))
	}

	@Test
	fun the_default_count_is_written_and_read() {
		val file = PairingsFile.seal(two, pass(), exportedAt = 0, appVersion = "")
		assertTrue("\"iterations\":${PairingsFile.ITERATIONS}," in text(file))
		assertIs<PairingsFile.Opened.Pairings>(PairingsFile.open(file, pass()))
	}

	@Test
	fun a_wrong_passphrase_or_any_byte_changed_opens_nothing() {
		val file = seal()
		assertEquals(PairingsFile.Opened.WrongPassphrase, PairingsFile.open(file, pass("two words or less")))
		val end = headerEnd(file)
		// The sealed list, its tag at the end, and the file cut short.
		for (at in listOf(end + 1, end + 20, file.size - 1)) {
			val b = file.copyOf().also { it[at] = (it[at].toInt() xor 0x20).toByte() }
			assertEquals(PairingsFile.Opened.WrongPassphrase, PairingsFile.open(b, pass()), "byte $at")
		}
		assertEquals(PairingsFile.Opened.WrongPassphrase, PairingsFile.open(file.copyOf(file.size - 1), pass()))
		// The header: what makes the key, and what doesn't but is authenticated all the same.
		val header = text(file.copyOf(end))
		for (changed in listOf(header.replace("\"iterations\":1000", "\"iterations\":1001"), header.replace("{\"format\":1", "{\"note\":1,\"format\":1"))) {
			val b = changed.toByteArray(Charsets.ISO_8859_1) + file.copyOfRange(end, file.size)
			assertEquals(PairingsFile.Opened.WrongPassphrase, PairingsFile.open(b, pass()), changed)
		}
	}

	@Test
	fun a_newer_format_is_told_before_the_passphrase() {
		for (rest in listOf("{\"format\":2,\"kdf\":\"x\"}\n\u0000\u0001", "{\"format\":3}\n")) {
			val b = ("elamon-pairings\n$rest").toByteArray()
			assertEquals(PairingsFile.Opened.Newer(rest[10].digitToInt()), PairingsFile.check(b))
			assertEquals(PairingsFile.Opened.Newer(rest[10].digitToInt()), PairingsFile.open(b, pass()))
		}
	}

	@Test
	fun what_isnt_an_export_or_is_broken_says_so() {
		assertEquals(PairingsFile.Opened.NotPairings, PairingsFile.check("elamond-backup\n{}".toByteArray()))
		assertEquals(PairingsFile.Opened.NotPairings, PairingsFile.check(ByteArray(0)))
		val header = text(seal().copyOf(headerEnd(seal())))
		for (broken in listOf(
			"elamon-pairings\n{\"format\":1",
			"elamon-pairings\nnot json\n",
			"elamon-pairings\n{\"kdf\":\"PBKDF2WithHmacSHA256\"}\n",
			header.replace("PBKDF2WithHmacSHA256", "PBKDF2WithHmacSHA1") + "\n",
			header.replace("AES-256-GCM", "AES-128-CBC") + "\n",
			// More work than a file may ask for.
			header.replace("\"iterations\":1000", "\"iterations\":${PairingsFile.ITERATIONS_MAX + 1}") + "\n",
			header.replace(Regex("\"nonce\":\"[^\"]*\""), "\"nonce\":\"AAAA\"") + "\n",
		)) {
			assertEquals(PairingsFile.Opened.Damaged, PairingsFile.check(broken.toByteArray(Charsets.ISO_8859_1)), broken)
		}
	}

	@Test
	fun a_list_that_opens_but_doesnt_hold_usable_computers_is_damaged() {
		for (bad in listOf(
			machine("a").copy(fingerprint = "zz"),
			machine("a").copy(token = ""),
			machine("a").copy(hosts = emptyList()),
			machine("a").copy(hosts = listOf("not a host!")),
			machine("a").copy(port = 0),
			machine("").copy(token = "t"),
		)) {
			assertEquals(PairingsFile.Opened.Damaged, PairingsFile.open(seal(listOf(machine("b"), bad)), pass()), bad.toString())
		}
	}

	@Test
	fun a_salt_and_a_nonce_of_its_own_each_time() {
		val random = SecureRandom()
		val a = PairingsFile.seal(two, pass(), 0, "", random, quick)
		val b = PairingsFile.seal(two, pass(), 0, "", random, quick)
		assertFalse(a.contentEquals(b))
		assertFalse(text(a).lines()[1] == text(b).lines()[1])
	}

	@Test
	fun imports_replace_by_id_in_place_and_add_the_rest_at_the_end() {
		val current = listOf(machine("a"), machine("b"), machine("c"))
		val b2 = machine("b", token = "new-b", name = "Renamed")
		val merged = mergeImported(current, listOf(machine("d"), b2, machine("e")))
		assertEquals(listOf("a", "b", "c", "d", "e"), merged.list.map { it.id })
		assertEquals(b2, merged.list[1])
		assertEquals(2 to 1, merged.added to merged.replaced)
		// The same computer twice in a file counts once, as the last.
		val twice = mergeImported(current, listOf(machine("d", token = "1"), machine("d", token = "2")))
		assertEquals(listOf("token-a", "token-b", "token-c", "2"), twice.list.map { it.token })
		assertEquals(1 to 0, twice.added to twice.replaced)
		// Nothing to import changes nothing.
		assertEquals(Merged(current, 0, 0), mergeImported(current, emptyList()))
	}
}
