package io.github.elaizes.elamon.data

import java.security.SecureRandom

/**
 * Passphrases for exported pairings: what makes one too easy to guess, and new ones made of random
 * common words, easy to write down and type on another phone.
 */
object Passphrases {
	/** Why a passphrase isn't taken. */
	enum class Weakness {
		/** Shorter than [PairingsFile.PASSPHRASE_MIN] characters. */
		TooShort,

		/** One character, or a few, over and over: "aaaaaaaaaaaa", "abababababab". */
		Repeated,

		/** A run along the alphabet, the digits or the keyboard: "abcdefghijkl", "123456789012", "qwertyuiopas". */
		Sequence,

		/** A well-known password, perhaps with digits or signs around it or letters swapped for look-alikes: "P@ssw0rd2024!". */
		Common,
	}

	/** Words in a passphrase [generate] makes. */
	const val GENERATED_WORDS = 6

	/** Why [passphrase] is too easy to guess, or null when it isn't obviously so. */
	fun weakness(passphrase: CharSequence): Weakness? {
		if (Character.codePointCount(passphrase, 0, passphrase.length) < PairingsFile.PASSPHRASE_MIN) return Weakness.TooShort
		val s = passphrase.toString().lowercase()
		if (shortestPeriod(s) <= REPEAT_UNIT_MAX) return Weakness.Repeated
		if (isRun(s)) return Weakness.Sequence
		if (isCommon(s)) return Weakness.Common
		return null
	}

	/**
	 * [words] words drawn from [WORDS] with [random], with spaces between: about 50 bits for
	 * [GENERATED_WORDS]. Drawn again in the rare case that the draw is itself too easy to guess.
	 */
	fun generate(random: SecureRandom = SecureRandom(), words: Int = GENERATED_WORDS): String =
		generateSequence { List(words) { WORDS[random.nextInt(WORDS.size)] }.joinToString(" ") }.first { weakness(it) == null }

	/** The length of the shortest piece [s] is that piece over and over (the last time perhaps cut short). */
	private fun shortestPeriod(s: String): Int =
		(1..s.length).first { p -> (p until s.length).all { s[it] == s[it - p] } }

	/** Whether [s] goes along the alphabet or the digits one step at a time, up or down, or along a run of the keyboard. */
	private fun isRun(s: String): Boolean {
		val steps = s.zipWithNext { a, b -> step(a, b) }
		if (steps.all { it == 1 } || steps.all { it == -1 }) return true
		return KEYBOARD.any { run -> run.repeat(RUN_REPEATS).contains(s) || run.reversed().repeat(RUN_REPEATS).contains(s) }
	}

	/** From [a] to [b]: one step on (1), one back (-1), or anything else (0). Digits go round, 9 to 0. */
	private fun step(a: Char, b: Char): Int = when {
		a.isDigit() && b.isDigit() -> when ((b - a + DIGITS) % DIGITS) {
			1 -> 1
			DIGITS - 1 -> -1
			else -> 0
		}
		a.isLetter() && b.isLetter() -> (b - a).takeIf { it == 1 || it == -1 } ?: 0
		else -> 0
	}

	/**
	 * Whether [s] is a common password, or one over and over, once what people put around one is
	 * taken away (digits and signs before or after it, spaces) and look-alikes read as the letters
	 * they stand for: "P@ssw0rd2024!", "@dmin1234567", "letmein letmein".
	 */
	private fun isCommon(s: String): Boolean {
		val t = s.filterNot { it.isWhitespace() }
		val cores = listOf(
			lookAlikes(t.trim { !it.isLetter() }),
			lookAlikes(t.trimEnd { !it.isLetter() }).trimStart { !it.isLetter() },
		)
		return cores.any { core -> core.isNotEmpty() && COMMON.any { w -> core.length % w.length == 0 && w.repeat(core.length / w.length) == core } }
	}

	private fun lookAlikes(s: String): String = s.map { LOOK_ALIKES[it] ?: it }.joinToString("")

	private const val REPEAT_UNIT_MAX = 4
	private const val RUN_REPEATS = 3
	private const val DIGITS = 10

	private val KEYBOARD = listOf("qwertyuiopasdfghjklzxcvbnm", "1234567890", "1qaz2wsx3edc4rfv5tgb6yhn7ujm8ik9ol0p", "qazwsxedcrfvtgbyhnujmikolp", "azertyuiopqsdfghjklmwxcvbn", "qwertzuiopasdfghjklyxcvbnm")

	private val LOOK_ALIKES = mapOf('0' to 'o', '1' to 'i', '3' to 'e', '4' to 'a', '5' to 's', '7' to 't', '@' to 'a', '$' to 's', '!' to 'i')

	/** A short list of the passwords tried first, as the letters they come to. */
	private val COMMON = listOf(
		"password", "passwort", "passphrase", "qwerty", "qwertyuiop", "azerty", "letmein", "iloveyou", "admin", "administrator",
		"welcome", "monkey", "dragon", "football", "baseball", "soccer", "sunshine", "princess", "master", "shadow", "superman",
		"batman", "trustno", "starwars", "whatever", "freedom", "changeme", "secret", "login", "hello", "helloworld", "mustang",
		"michael", "jennifer", "hunter", "ashley", "charlie", "computer", "internet", "android", "google", "elamon", "elamond",
		"opensesame", "correcthorsebatterystaple", "qazwsx", "asdfgh", "asdfghjkl", "zxcvbnm", "abc", "test", "guest", "root",
		"default", "pass", "access", "summer", "winter", "spring", "autumn", "flower", "lovely", "love", "money", "loveme",
	)

	/** Common, short English words: [GENERATED_WORDS] of them drawn at random are a passphrase. */
	val WORDS: List<String> = """
		able acid aged also apple april arch area army atom aunt away baby back bake ball band bank barn base basket
		bath beach bean bear beef bell belt bench berry bike bird black blade blank blue boat body bone book boot bottle
		bowl brain brave bread brick bridge brown brush bucket build bulb bunny burst cabin cable cake calm camel camp
		candle candy canoe card cargo carpet carrot castle cedar chain chair chalk cheese cherry chess chief child chip
		circle city clay clock cloud coast coat cocoa coffee coin comet coral corn cotton couch cousin crane cream crisp
		crown cube cycle daisy dance desert desk diary dinner disk dock dolphin donkey door dove dragon drawer dream
		dress drum duck eagle earth easel echo eight elbow empty engine envelope fabric farm feather fence fern ferry
		field fifty finger fire fish flag flame flute foam forest fork fossil fountain fox frame frog fruit garden garlic
		gate gift ginger glass glove goat gold grape grass gravel green guitar hammer harbor hat hazel heart helmet herb
		hill honey horse house island ivory jacket jelly jewel juice jungle kayak kettle key kitten kiwi knife ladder lake
		lamp lantern laptop lemon letter lily lime lion lizard lobster magnet mango maple marble market meadow melon
		metal milk mint mirror moon moss motor mountain mouse muffin music needle nest night noodle north nutmeg oak
		ocean olive onion orange orbit otter owl paddle paint palm panda paper parrot pasta peach peanut pear pebble
		pencil pepper piano pickle pillow pilot pine planet plate plum pocket pond potato pumpkin puzzle quartz quilt
		rabbit radio rain raven ribbon river robin rocket roof rose ruby saddle salad salmon salt sand scarf school
		seed shell ship shoe silver sky sled snow sofa spoon spring squid stamp star stone storm straw sugar summer
		sun swan table tiger timber toast tomato tower train tree tulip tunnel turtle umbrella valley velvet violin
		wagon walnut water whale wheat wheel willow window winter wolf wood yarn yellow zebra anchor apron badge bamboo
		banjo beetle blanket breeze bubble butter cactus canyon cereal circus clover cobalt cookie copper cricket
		crystal dinosaur domino falcon flannel forty galaxy gecko glacier gopher granite hamster hedge iceberg igloo
		jasmine kernel koala lagoon lava locket meteor mitten nickel noble oasis oyster pantry pelican pigeon prism
		quiver raccoon rhythm saffron sapphire shovel sparrow spider sponge summit teapot thistle thunder trumpet
		tundra unicorn vanilla vapor walrus wizard yogurt zipper
	""".trimIndent().split(Regex("\\s+")).filter { it.isNotEmpty() }
}
