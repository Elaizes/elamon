package io.github.elaizes.elamon.data

/** How long the app waits, how often it asks, and how much it takes: the numbers of talking to elamond, in one place. */
object Limits {
	// ---- timeouts, ms ---------------------------------------------------------------------------

	/** Connecting to elamond, any request. */
	const val CONNECT_TIMEOUT_MS = 4_000

	/** Reading an answer: hello, a pairing request, a cover, anything else. */
	const val HELLO_TIMEOUT_MS = 4_000
	/**
	 * A pairing request: the service holds it open while `elamond pair` asks at the computer whether
	 * to allow the phone, for up to 90 s, and answers then.
	 */
	const val PAIR_TIMEOUT_MS = 100_000

	/** How long a pairing request goes unanswered before the screen says to confirm it on the computer. */
	const val PAIR_APPROVAL_HINT_MS = 2_000L
	const val BYTES_TIMEOUT_MS = 10_000
	const val REQUEST_TIMEOUT_MS = 15_000

	/** An action may run as long as the service lets it (an hour), and its answer needs a moment more. */
	const val ACTION_TIMEOUT_MS = 3_700_000

	/** A stream that says nothing for this long, or for this many of its lines, is a dead connection. */
	const val STREAM_SILENCE_MS = 15_000
	const val STREAM_MISSED_SAMPLES = 4

	/** How long a stream asked for a line every [intervalMs] may stay silent. */
	fun streamReadTimeoutMs(intervalMs: Int): Int = maxOf(STREAM_SILENCE_MS, intervalMs * STREAM_MISSED_SAMPLES)

	// ---- sizes, bytes ---------------------------------------------------------------------------
	// Whatever answers, nothing it sends can take the phone's memory: a longer answer is refused,
	// up front when it says its length.

	/** Hello, a pairing or a key of the phone's own, the certificate's rotations, an error: a little JSON. */
	const val SMALL_BODY_MAX = 64 * 1024

	/**
	 * Any other answer. The largest the service sends is a snapshot or a data line with a table of every process it lists:
	 * 10,000 of them, with command lines of up to 400 bytes, come to about 6 MB.
	 */
	const val BODY_MAX = 8 * 1024 * 1024

	/** A cover image. */
	const val ART_MAX = 4 * 1024 * 1024

	/**
	 * One line of the stream: a data line, with a table of every process (as
	 * large as [BODY_MAX] says), or the history, about 1 MB at the 7,200 readings the service keeps at most.
	 */
	const val STREAM_LINE_MAX = BODY_MAX

	/**
	 * Objects and arrays inside each other in anything the service sends. Its deepest, a `ui` with
	 * groups in groups, is under 10; this takes some 30 groups inside each other. Reading JSON
	 * into the app's types is recursive: a document much deeper would overflow the stack.
	 */
	const val JSON_DEPTH_MAX = 64

	// ---- retrying ---------------------------------------------------------------------------------

	/** The first wait before looking for a lost computer again; it doubles this many times, never past the cap. */
	const val BACKOFF_FIRST_MS = 1_000L
	const val BACKOFF_DOUBLINGS = 3
	const val BACKOFF_CAP_MS = 8_000L

	/** How long to wait before the [attempt]th try (counting from 0). */
	fun backoffMs(attempt: Int): Long = (BACKOFF_FIRST_MS shl attempt.coerceIn(0, BACKOFF_DOUBLINGS)).coerceAtMost(BACKOFF_CAP_MS)

	// ---- pacing -----------------------------------------------------------------------------------

	/** How often the home screen reads each computer. */
	const val HOME_POLL_MS = 5_000L

	/** How long a stopped screen keeps its stream: enough for a rotation. */
	const val STOP_GRACE_MS = 5_000L

	/** How long a new dashboard, sort or interval has to stay before the stream is told of it. */
	const val PARAMS_SETTLE_MS = 150L

	/** A dashboard's readings more than this many lines older than the newest are old: the screen doesn't call them live. */
	const val LIVE_LINES = 2

	/** How often a playing track's position moves on between lines. */
	const val MEDIA_TICK_MS = 500L

	/** Results and errors kept for the snackbar while no screen shows them. */
	const val MESSAGES_KEPT = 8

	/**
	 * After asking a computer for a key of this phone's own failed, how long before a connection asks
	 * again: the home screen connects every few seconds, and each request is in the computer's log.
	 */
	const val OWN_KEY_RETRY_MS = 30_000L

	// ---- pairing ----------------------------------------------------------------------------------

	/** Digits in a pairing code. */
	const val PAIRING_DIGITS = 6

	/** Hex digits of the service's id, and of its certificate's fingerprint. */
	const val ID_HEX = 32
	const val FINGERPRINT_HEX = 64

	/** How much of the fingerprint is shown to compare by eye, in groups of so many: as `elamond pair` prints it. */
	const val FINGERPRINT_SHOWN = 32
	const val FINGERPRINT_GROUP = 4

	/** The most addresses a pairing link may list, and the longest name it may give. */
	const val PAIR_LINK_MAX_HOSTS = 8
	const val PAIR_LINK_MAX_NAME = 64

	/** Characters in a computer's or a phone's name, as the service caps them. */
	const val NAME_MAX = 64

	/**
	 * How long after the pairing screen is ready, or the app is brought to the front, a tap on what
	 * pairs is ignored: one meant for whatever was in front before may land on it.
	 */
	const val TAP_QUIET_MS = 1_000L

	/** Digits a pairing link's version may have; anything longer is broken, not new. */
	const val PAIR_LINK_VERSION_DIGITS = 9

	/** The key that seals tokens on the phone (AES), and the length of GCM's tag. */
	const val TOKEN_KEY_BITS = 256
	const val TOKEN_TAG_BITS = 128

	// ---- addresses --------------------------------------------------------------------------------

	const val PORT_MAX = 65_535
	const val PORT_DIGITS = 5

	/** The port mDNS answers on, the same everywhere: what finds computers on the network. */
	const val MDNS_PORT = 5353

	/** Computers found on the network that are listed, and announcements waiting to be resolved: a network can't flood the list. */
	const val MDNS_FOUND_MAX = 32
	const val MDNS_PENDING_MAX = 64
}

/** This text cut to [max] characters as the service counts them (code points), never inside a surrogate pair. */
fun String.takeCodePoints(max: Int): String {
	if (max <= 0) return ""
	if (codePointCount(0, length) <= max) return this
	return substring(0, offsetByCodePoints(0, max))
}

/**
 * This text without what can make it read as something else: control characters, format ones
 * (the bidi overrides and isolates, zero-width joiners and the like), line and paragraph
 * separators, and halves of a surrogate pair standing alone. [keepLineBreaks] keeps '\n'.
 */
fun String.withoutControls(keepLineBreaks: Boolean = false): String {
	val out = StringBuilder(length)
	var i = 0
	while (i < length) {
		val cp = codePointAt(i)
		val shown = when (Character.getType(cp).toByte()) {
			Character.CONTROL -> keepLineBreaks && cp == '\n'.code
			Character.FORMAT, Character.LINE_SEPARATOR, Character.PARAGRAPH_SEPARATOR, Character.SURROGATE -> false
			else -> true
		}
		if (shown) out.appendCodePoint(cp)
		i += Character.charCount(cp)
	}
	return out.toString()
}

/**
 * A name the service sent (a computer's, a phone's), as the app shows and keeps it: on one line,
 * [withoutControls], trimmed, and at most [max] characters, so it can't pass for another or push
 * what follows it out of sight.
 */
fun cleanName(name: String, max: Int = Limits.NAME_MAX): String = name.withoutControls().trim().takeCodePoints(max).trim()
