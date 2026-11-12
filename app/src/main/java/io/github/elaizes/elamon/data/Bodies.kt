package io.github.elaizes.elamon.data

import io.github.elaizes.elamon.BuildConfig
import java.io.ByteArrayOutputStream
import java.io.InputStream

/**
 * What the app says it is to the service, and nothing more: Android's own User-Agent names the
 * phone's model and build.
 */
val USER_AGENT = "Elamon/${BuildConfig.VERSION_NAME}"

/**
 * What kind of client this is, for the service (protocol API 2): it shows it in `elamond clients`
 * and gives the app the profile's part for Android.
 */
val CLIENT = "android/${BuildConfig.VERSION_NAME}"

/**
 * The headers every request carries. "identity": answers come as they are, never compressed, so
 * the length of what is read is the length of what was sent ([readCapped]); and Android doesn't
 * unpack anything on the app's behalf.
 */
internal fun requestHeaders(token: String?): List<Pair<String, String>> = listOfNotNull(
	"Accept" to "application/json",
	"Accept-Encoding" to "identity",
	"User-Agent" to USER_AGENT,
	"Elamon-Client" to CLIENT,
	token?.let { "Authorization" to "Bearer $it" },
)

private fun tooLarge(max: Int) = ElamondException(ElamondException.Kind.Protocol, "answer over $max bytes")

/** An answer that says it is [declared] bytes long (-1: it doesn't say) is refused before it is read when that is over [max]. */
internal fun checkLength(declared: Long, max: Int) {
	if (declared > max) throw tooLarge(max)
}

/** All of [input], refused ([ElamondException], Protocol) as soon as it goes over [max] bytes. */
internal fun readCapped(input: InputStream, max: Int): ByteArray {
	val out = ByteArrayOutputStream()
	val buf = ByteArray(BUFFER)
	while (true) {
		val n = input.read(buf)
		if (n < 0) return out.toByteArray()
		if (n > max - out.size()) throw tooLarge(max)
		out.write(buf, 0, n)
	}
}

/** Up to [max] bytes of [input], the rest left unread: an error's body, which is told by its status anyway. */
internal fun readUpTo(input: InputStream, max: Int): ByteArray {
	val out = ByteArrayOutputStream()
	val buf = ByteArray(BUFFER)
	while (out.size() < max) {
		val n = input.read(buf, 0, minOf(buf.size, max - out.size()))
		if (n < 0) break
		out.write(buf, 0, n)
	}
	return out.toByteArray()
}

/**
 * The stream's lines, UTF-8, each refused ([ElamondException], Protocol) once it goes over [max]
 * bytes without ending: unlike BufferedReader.readLine, which would hold however much came.
 */
internal class LineReader(private val input: InputStream, private val max: Int) {
	private val buf = ByteArray(BUFFER)
	private var pos = 0
	private var end = 0
	private val line = ByteArrayOutputStream()

	/** The next line without its "\n" (and a "\r" before it); null at the end, a last line without one still read. */
	fun readLine(): String? {
		line.reset()
		while (true) {
			if (pos == end) {
				val n = input.read(buf)
				if (n < 0) return if (line.size() > 0) text() else null
				pos = 0
				end = n
			}
			var i = pos
			while (i < end && buf[i] != NEWLINE) i++
			if (i - pos > max - line.size()) throw tooLarge(max)
			line.write(buf, pos, i - pos)
			if (i < end) {
				pos = i + 1
				return text()
			}
			pos = end
		}
	}

	private fun text(): String {
		val bytes = line.toByteArray()
		val n = if (bytes.isNotEmpty() && bytes.last() == RETURN) bytes.size - 1 else bytes.size
		return String(bytes, 0, n, Charsets.UTF_8)
	}

	private companion object {
		const val NEWLINE = '\n'.code.toByte()
		const val RETURN = '\r'.code.toByte()
	}
}

private const val BUFFER = 8192
