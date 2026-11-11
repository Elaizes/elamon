package io.github.elaizes.elamon.data

import io.github.elaizes.elamon.data.Passphrases.Weakness
import java.security.SecureRandom
import kotlin.math.log2
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class PassphrasesTest {
	@Test
	fun twelveCharactersAtLeast() {
		assertEquals(12, PairingsFile.PASSPHRASE_MIN)
		assertEquals(Weakness.TooShort, Passphrases.weakness(""))
		assertEquals(Weakness.TooShort, Passphrases.weakness("tr4in-c0ld!"))
		assertNull(Passphrases.weakness("tr4in-c0ld!x"))
		// Counted as characters: an emoji is one.
		assertEquals(Weakness.TooShort, Passphrases.weakness("😀".repeat(5) + "kzqwvb"))
	}

	@Test
	fun theObviouslyWeakAreRefused() {
		for (p in listOf("aaaaaaaaaaaa", "AAAAAAAAAAAAAAAA", "............", "abababababab", "123123123123", "abcdabcdabcd")) {
			assertEquals(Weakness.Repeated, Passphrases.weakness(p), p)
		}
		for (p in listOf("abcdefghijkl", "LKJIHGFEDCBA", "123456789012", "098765432109", "qwertyuiopas", "asdfghjklzxc", "1qaz2wsx3edc", "lkjhgfdsapoi", "azertyuiopqs")) {
			assertEquals(Weakness.Sequence, Passphrases.weakness(p), p)
		}
		for (p in listOf("password1234", "Password123!", "P@ssw0rd2024!", "p4ssw0rd!!!!", "@dmin1234567", "iloveyou2024", "letmein letmein", "passwordpassword", "!!!iloveyou!!!", "Welcome12345", "elamon123456")) {
			assertEquals(Weakness.Common, Passphrases.weakness(p), p)
		}
	}

	@Test
	fun ordinaryPassphrasesPass() {
		for (p in listOf("correct horse battery", "Tr0ub4dor&3xyz", "пароль із кількох слів", "my cat eats paper", "839205718364", "password is not my password")) {
			assertNull(Passphrases.weakness(p), p)
		}
	}

	@Test
	fun generatedPassphrasesAreSixRandomCommonWords() {
		val words = Passphrases.WORDS
		// Enough words for about 50 bits in six, each simple to type.
		assertEquals(words.size, words.toSet().size, "a word listed twice")
		assertTrue(words.size >= 256, "${words.size} words")
		assertTrue(Passphrases.GENERATED_WORDS * log2(words.size.toDouble()) >= 48)
		for (w in words) assertTrue(Regex("[a-z]{3,8}").matches(w), w)

		val random = SecureRandom()
		val made = List(500) { Passphrases.generate(random) }
		for (p in made) {
			val parts = p.split(" ")
			assertEquals(Passphrases.GENERATED_WORDS, parts.size, p)
			assertTrue(parts.all { it in words }, p)
			assertNull(Passphrases.weakness(p), p)
		}
		assertTrue(made.toSet().size > 495, "the same passphrase made again and again")
		assertNotEquals(Passphrases.generate(random), Passphrases.generate(random))
	}
}
