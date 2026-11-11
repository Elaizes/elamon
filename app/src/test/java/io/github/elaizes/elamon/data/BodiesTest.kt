package io.github.elaizes.elamon.data

import io.github.elaizes.elamon.BuildConfig
import java.io.ByteArrayInputStream
import java.io.InputStream
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class BodiesTest {
	/** [bytes] handed out [chunk] at a time, as a socket does. */
	private class Trickle(private val bytes: ByteArray, private val chunk: Int) : InputStream() {
		private var at = 0
		var read = 0
			private set

		override fun read(): Int = if (at < bytes.size) bytes[at++].toInt() and 0xff else -1

		override fun read(b: ByteArray, off: Int, len: Int): Int {
			if (at >= bytes.size) return -1
			val n = minOf(len, chunk, bytes.size - at)
			System.arraycopy(bytes, at, b, off, n)
			at += n
			read += n
			return n
		}
	}

	/** Never ends: an answer that would go on for ever. */
	private object Endless : InputStream() {
		override fun read(): Int = 'x'.code
	}

	private fun tooLarge(block: () -> Unit) {
		val e = assertFailsWith<ElamondException> { block() }
		assertEquals(ElamondException.Kind.Protocol, e.kind)
	}

	@Test
	fun anAnswerUpToItsCapIsReadWhole() {
		val body = ByteArray(1000) { it.toByte() }
		assertContentEquals(body, readCapped(Trickle(body, 7), 1000))
		assertContentEquals(ByteArray(0), readCapped(ByteArrayInputStream(ByteArray(0)), 0))
	}

	@Test
	fun aLongerOneIsRefusedAsItComes() {
		tooLarge { readCapped(ByteArrayInputStream(ByteArray(1001)), 1000) }
		// Not read to its end first: one that never ends is refused all the same.
		tooLarge { readCapped(Endless, Limits.SMALL_BODY_MAX) }
		val big = Trickle(ByteArray(10 * Limits.SMALL_BODY_MAX), 8192)
		tooLarge { readCapped(big, Limits.SMALL_BODY_MAX) }
		assertTrue(big.read <= Limits.SMALL_BODY_MAX + 8192, "read ${big.read}")
	}

	@Test
	fun aLengthSaidUpFrontIsCheckedBeforeReading() {
		checkLength(-1, 10)
		checkLength(10, 10)
		tooLarge { checkLength(11, 10) }
		tooLarge { checkLength(Long.MAX_VALUE, Limits.BODY_MAX) }
	}

	@Test
	fun anErrorsBodyIsCutRatherThanRefused() {
		assertEquals(5, readUpTo(Endless, 5).size)
		assertContentEquals("abc".toByteArray(), readUpTo(ByteArrayInputStream("abc".toByteArray()), 5))
	}

	@Test
	fun theCapsLeaveRoomForWhatTheServiceSends() {
		assertEquals(64 * 1024, Limits.SMALL_BODY_MAX)
		assertEquals(4 * 1024 * 1024, Limits.ART_MAX)
		// 10,000 processes with 400-byte command lines and the rest of each, about 600 bytes.
		assertTrue(Limits.BODY_MAX >= 10_000 * 600)
		assertTrue(Limits.STREAM_LINE_MAX >= 10_000 * 600)
	}

	@Test
	fun streamLines() {
		val text = "{\"type\":\"info\"}\n\n{\"type\":\"sample\",\"x\":\"я😀\"}\r\nlast"
		for (chunk in listOf(1, 2, 3, 5, 8192)) {
			val r = LineReader(Trickle(text.toByteArray(), chunk), 64)
			assertEquals("{\"type\":\"info\"}", r.readLine())
			assertEquals("", r.readLine())
			// A character split between two reads comes out whole.
			assertEquals("{\"type\":\"sample\",\"x\":\"я😀\"}", r.readLine(), "chunk $chunk")
			assertEquals("last", r.readLine())
			assertNull(r.readLine())
		}
		assertNull(LineReader(ByteArrayInputStream(ByteArray(0)), 10).readLine())
	}

	@Test
	fun aStreamLineOverItsCapIsRefusedWithoutHoldingIt() {
		val max = 20_000
		val exact = "x".repeat(max) + "\n" + "y".repeat(max + 1) + "\n"
		for (chunk in listOf(3, 8192, 100_000)) {
			val r = LineReader(Trickle(exact.toByteArray(), chunk), max)
			assertEquals(max, r.readLine()?.length)
			tooLarge { r.readLine() }
		}
		tooLarge { LineReader(Endless, Limits.STREAM_LINE_MAX).readLine() }
	}

	@Test
	fun everyRequestSaysOnlyThatItIsElamonAndTakesNothingCompressed() {
		val h = requestHeaders(null).toMap()
		assertEquals("Elamon/${BuildConfig.VERSION_NAME}", h["User-Agent"])
		assertEquals("identity", h["Accept-Encoding"])
		assertEquals("application/json", h["Accept"])
		assertFalse("Authorization" in h)
		assertEquals("Bearer t0ken", requestHeaders("t0ken").toMap()["Authorization"])
		// Nothing of the phone: Android's own service names its model and build.
		for (word in listOf("Android", "Dalvik", "Linux", "Build")) assertFalse(word in USER_AGENT, word)
	}
}
