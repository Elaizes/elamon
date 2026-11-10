package ua.elamon.data

import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.concurrent.atomic.AtomicReference
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.X509TrustManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

class ElamondException(
	val kind: Kind,
	message: String? = null,
	cause: Throwable? = null,
	/** The service's error code, like "no_such_process", when it gave one. */
	val code: String? = null,
	/** The HTTP status, when the service answered at all. */
	val status: Int = 0,
) : IOException(message, cause) {
	enum class Kind { Unreachable, Unauthorized, WrongCode, NoPairing, CertificateChanged, NotElamond, Protocol, ControlDisabled, NotPermitted, Forbidden, NotFound, BadRequest, Failed }

	companion object {
		/** An error answer: its status and the body, {"error": code} or {"error": "failed", "message": text}. */
		fun fromResponse(status: Int, body: String): ElamondException {
			val obj = runCatching { ProtocolJson.parseToJsonElement(body).jsonObject }.getOrNull()
			val error = runCatching { obj?.get("error")?.jsonPrimitive?.content }.getOrNull()
			val message = runCatching { obj?.get("message")?.jsonPrimitive?.content }.getOrNull()
			val kind = when {
				status == 401 || error == "unauthorized" -> Kind.Unauthorized
				error == "wrong_code" -> Kind.WrongCode
				error == "no_pairing" -> Kind.NoPairing
				error == "control_disabled" -> Kind.ControlDisabled
				error == "not_permitted" || error == "protected_process" -> Kind.NotPermitted
				status == 403 -> Kind.Forbidden
				status == 404 -> Kind.NotFound
				status == 400 -> Kind.BadRequest
				error == "failed" || status >= 500 -> Kind.Failed
				else -> Kind.Protocol
			}
			// A failed command carries the computer's own words; anything else is known by its code.
			val text = if (error == "failed") message else error ?: "HTTP $status"
			return ElamondException(kind, text, code = error, status = status)
		}
	}
}

/**
 * Runs [block], which blocks on I/O that ignores coroutine cancellation, and calls [close] (closing
 * the connection) as soon as the coroutine is cancelled, so the block ends at once; [close] also
 * runs when the block ends by itself. Whatever the block throws once cancelled, like the read that
 * failed because the socket was closed under it, comes out as the cancellation it is: an error
 * escaping a cancelled coroutine would fail its parent instead of letting it carry on. Run it on a
 * dispatcher with threads to spare (Dispatchers.IO): the closing runs beside the blocked block.
 */
internal suspend fun <T> closeOnCancel(close: () -> Unit, block: suspend CoroutineScope.() -> T): T = coroutineScope {
	// Started at once: a closer cancelled before it ever ran would never close anything.
	val closer = launch(start = CoroutineStart.UNDISPATCHED) {
		try {
			awaitCancellation()
		} finally {
			close()
		}
	}
	try {
		block()
	} catch (e: Throwable) {
		currentCoroutineContext().ensureActive()
		throw e
	} finally {
		closer.cancel()
	}
}

/**
 * Talks to one service at [host]:[port] over HTTPS.
 *
 * The service's certificate is self-signed, so instead of a certificate authority the app trusts
 * exactly one certificate: [pin], its SHA-256. Without a pin (pairing by code) any certificate is
 * accepted and remembered in [seenCertificate] for the user to compare with the computer's.
 */
class ElamondClient(
	val host: String,
	val port: Int,
	private val pin: String?,
	private val token: String? = null,
) {
	val seenCertificate = AtomicReference<String?>(null)

	private val socketFactory: SSLSocketFactory by lazy {
		val trust = object : X509TrustManager {
			override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) =
				throw CertificateException("no client certificates")

			override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
				val leaf = chain?.firstOrNull() ?: throw CertificateException("no certificate")
				val fp = sha256Hex(leaf.encoded)
				seenCertificate.set(fp)
				if (pin != null && !pin.equals(fp, ignoreCase = true)) throw CertificateException("certificate changed")
			}

			override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
		}
		SSLContext.getInstance("TLS").apply { init(null, arrayOf(trust), null) }.socketFactory
	}

	private fun url(path: String): URL {
		val h = if (host.contains(':') && !host.startsWith("[")) "[$host]" else host
		return URL("https://$h:$port$path")
	}

	private fun open(path: String, readTimeoutMs: Int): HttpsURLConnection {
		val c = url(path).openConnection() as HttpsURLConnection
		c.sslSocketFactory = socketFactory
		// The pin replaces name checks: the certificate is this service's or the handshake failed.
		c.hostnameVerifier = javax.net.ssl.HostnameVerifier { _, _ -> true }
		c.connectTimeout = Limits.CONNECT_TIMEOUT_MS
		c.readTimeout = readTimeoutMs
		c.useCaches = false
		c.setRequestProperty("Accept", "application/json")
		token?.let { c.setRequestProperty("Authorization", "Bearer $it") }
		return c
	}

	private fun <T> call(block: () -> T): T = try {
		block()
	} catch (e: ElamondException) {
		throw e
	} catch (e: javax.net.ssl.SSLHandshakeException) {
		if (pin != null && seenCertificate.get()?.equals(pin, ignoreCase = true) == false) {
			throw ElamondException(ElamondException.Kind.CertificateChanged, cause = e)
		}
		throw ElamondException(ElamondException.Kind.Unreachable, e.message, e)
	} catch (e: kotlinx.serialization.SerializationException) {
		throw ElamondException(ElamondException.Kind.Protocol, e.message, e)
	} catch (e: IllegalArgumentException) {
		throw ElamondException(ElamondException.Kind.Protocol, e.message, e)
	} catch (e: IOException) {
		throw ElamondException(ElamondException.Kind.Unreachable, e.message, e)
	}

	private fun checkStatus(c: HttpURLConnection) {
		val code = c.responseCode
		if (code in 200..299) return
		val body = runCatching { c.errorStream?.bufferedReader()?.readText() }.getOrNull().orEmpty()
		throw ElamondException.fromResponse(code, body)
	}

	/** A request with an optional JSON body; returns the response body. */
	suspend fun request(method: String, path: String, body: String? = null, timeoutMs: Int = Limits.REQUEST_TIMEOUT_MS): String = withContext(Dispatchers.IO) {
		call {
			val c = open(path, timeoutMs)
			try {
				c.requestMethod = method
				if (body != null) {
					c.doOutput = true
					c.setRequestProperty("Content-Type", "application/json")
					c.outputStream.use { it.write(body.toByteArray()) }
				}
				checkStatus(c)
				c.inputStream.bufferedReader().readText()
			} finally {
				c.disconnect()
			}
		}
	}

	suspend inline fun <reified T> get(path: String): T = ProtocolJson.decodeFromString(request("GET", path))

	suspend inline fun <reified B, reified T> post(path: String, body: B, timeoutMs: Int = Limits.REQUEST_TIMEOUT_MS): T =
		ProtocolJson.decodeFromString(request("POST", path, ProtocolJson.encodeToString(body), timeoutMs))

	suspend fun delete(path: String) {
		request("DELETE", path)
	}

	/** Raw bytes, for album art. */
	suspend fun bytes(path: String): ByteArray = withContext(Dispatchers.IO) {
		call {
			val c = open(path, Limits.BYTES_TIMEOUT_MS)
			try {
				checkStatus(c)
				c.inputStream.use { it.readBytes() }
			} finally {
				c.disconnect()
			}
		}
	}

	suspend fun hello(): Hello = withContext(Dispatchers.IO) {
		call {
			val c = open("/api/v1/hello", Limits.HELLO_TIMEOUT_MS)
			try {
				checkStatus(c)
				val h = ProtocolJson.decodeFromString<Hello>(c.inputStream.bufferedReader().readText())
				if (h.app != "elamon") throw ElamondException(ElamondException.Kind.NotElamond)
				h
			} finally {
				c.disconnect()
			}
		}
	}

	suspend fun pair(code: String, device: String): PairResponse = withContext(Dispatchers.IO) {
		call {
			val c = open("/api/v1/pair", Limits.PAIR_TIMEOUT_MS)
			try {
				c.requestMethod = "POST"
				c.doOutput = true
				c.setRequestProperty("Content-Type", "application/json")
				val body = buildJsonObject {
					put("code", code)
					put("device", device)
				}
				c.outputStream.use { it.write(ProtocolJson.encodeToString(body).toByteArray()) }
				checkStatus(c)
				ProtocolJson.decodeFromString<PairResponse>(c.inputStream.bufferedReader().readText())
			} finally {
				c.disconnect()
			}
		}
	}

	/** Info, then optionally history, then a sample every [intervalMs] until cancelled. */
	fun stream(intervalMs: Int, procs: Int, sort: String, history: Boolean, media: Boolean = false): Flow<StreamMessage> = flow {
		val query = "interval=$intervalMs&procs=$procs&sort=$sort&history=${if (history) 1 else 0}&media=${if (media) 1 else 0}"
		// A silent service is a dead connection after a few missed samples.
		val c = call { open("/api/v1/stream?$query", Limits.streamReadTimeoutMs(intervalMs)) }
		// Socket reads ignore coroutine cancellation; closing the connection ends a blocked one, and
		// the failed read that follows is that cancellation, not a lost computer.
		closeOnCancel(close = { c.disconnect() }) {
			call { checkStatus(c) }
			val reader = call { c.inputStream.bufferedReader() }
			while (true) {
				currentCoroutineContext().ensureActive()
				val line = call { reader.readLine() } ?: throw ElamondException(ElamondException.Kind.Unreachable, "closed")
				if (line.isBlank()) continue
				// A kind of message from a newer service is skipped; a broken one ends the stream.
				val msg = call { decodeStreamLine(line) } ?: continue
				emit(msg)
			}
		}
	}.flowOn(Dispatchers.IO)

	companion object {
		fun sha256Hex(data: ByteArray): String =
			MessageDigest.getInstance("SHA-256").digest(data).joinToString("") { "%02x".format(it) }

		/** The same grouping `elamond pair` prints, for comparing by eye. */
		fun prettyFingerprint(fp: String): String =
			fp.take(Limits.FINGERPRINT_SHOWN).uppercase().chunked(Limits.FINGERPRINT_GROUP).joinToString(" ")

		/**
		 * Tries every address at once and returns the first that answers as the expected service.
		 * A computer has several (Wi-Fi, cable, VPN) and only some are reachable from where the phone is.
		 */
		suspend fun firstReachable(
			hosts: List<String>,
			port: Int,
			pin: String?,
			expectId: String?,
		): Pair<ElamondClient, Hello>? = coroutineScope {
			if (hosts.isEmpty()) return@coroutineScope null
			var certificateChanged = false
			val attempts = hosts.distinct().map { h ->
				async {
					try {
						val client = ElamondClient(h, port, pin)
						val hello = client.hello()
						if (expectId != null && hello.id != expectId) null else client to hello
					} catch (e: ElamondException) {
						if (e.kind == ElamondException.Kind.CertificateChanged) certificateChanged = true
						null
					}
				}
			}
			var result: Pair<ElamondClient, Hello>? = null
			val pending = attempts.toMutableList()
			while (pending.isNotEmpty() && result == null) {
				val (done, value) = select {
					pending.forEach { d -> d.onAwait { d to it } }
				}
				pending.remove(done)
				result = value
			}
			pending.forEach { it.cancel() }
			// Reached, but presenting another certificate: worth telling apart from "offline".
			if (result == null && certificateChanged) throw ElamondException(ElamondException.Kind.CertificateChanged)
			result
		}
	}
}
