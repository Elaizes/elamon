package io.github.elaizes.elamon.data

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
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.serializer

class ElamondException(
	val kind: Kind,
	message: String? = null,
	cause: Throwable? = null,
	/** The service's error code, like "no_such_process", when it gave one. */
	val code: String? = null,
	/** The HTTP status, when the service answered at all. */
	val status: Int = 0,
	/** What the service said beside its code: the parameter of a `bad_param`, what `failed`. */
	val detail: String? = null,
) : IOException(message, cause) {
	/** The service doesn't let this phone do that: its role on the computer is too low. */
	val notAllowed: Boolean get() = kind == Kind.NotAllowed

	enum class Kind {
		Unreachable, Unauthorized, WrongCode, NoPairing, CertificateChanged, NotElamond, Protocol, ControlDisabled, NotPermitted,
		NotAllowed, TooManyRequests, Conflict, Forbidden, NotFound, BadRequest, Failed,

		/** The computer was asked whether to allow the phone, and said no or didn't answer. */
		PairDeclined,
	}

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
				error == "pair_declined" -> Kind.PairDeclined
				error == "control_disabled" -> Kind.ControlDisabled
				error == "not_permitted" || error == "protected_process" -> Kind.NotPermitted
				error == "not_allowed" -> Kind.NotAllowed
				// Acting too often, or too many wrong pairing codes from this address.
				status == 429 || error == "too_many_requests" -> Kind.TooManyRequests
				// A signal's process isn't the one that has its pid now (process_changed).
				status == 409 -> Kind.Conflict
				status == 403 -> Kind.Forbidden
				status == 404 -> Kind.NotFound
				status == 400 -> Kind.BadRequest
				error == "failed" || status >= 500 -> Kind.Failed
				else -> Kind.Protocol
			}
			// A failed command carries the computer's own words; anything else is known by its code.
			val text = if (error == "failed") message else error ?: "HTTP $status"
			return ElamondException(kind, text, code = error, status = status, detail = message)
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
 * Talks to elamond at [host]:[port] over HTTPS.
 *
 * Its certificate is self-signed, so instead of a certificate authority the app trusts
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
		// elamond never redirects; an answer that tries is not followed anywhere.
		c.instanceFollowRedirects = false
		for ((name, value) in requestHeaders(token)) c.setRequestProperty(name, value)
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
		val body = runCatching { c.errorStream?.use { String(readUpTo(it, Limits.SMALL_BODY_MAX), Charsets.UTF_8) } }.getOrNull().orEmpty()
		throw ElamondException.fromResponse(code, body)
	}

	/**
	 * Opens [path] and runs [block] on the connection, on Dispatchers.IO. Connecting and reading
	 * ignore coroutine cancellation, so a cancelled exchange closes the connection at once rather
	 * than wait out its timeout: an address given up on (another one answered first) costs nothing
	 * more. With [finishOnCancel] it is left open to finish instead, for a request the service undoes
	 * when it goes away (an action is killed with it).
	 */
	private suspend fun <T> exchange(path: String, readTimeoutMs: Int, finishOnCancel: Boolean = false, block: (HttpsURLConnection) -> T): T =
		withContext(Dispatchers.IO) {
			val c = call { open(path, readTimeoutMs) }
			if (finishOnCancel) {
				try {
					call { block(c) }
				} finally {
					c.disconnect()
				}
			} else {
				closeOnCancel(close = { c.disconnect() }) { call { block(c) } }
			}
		}

	/** Sends [body] as JSON. */
	private fun send(c: HttpURLConnection, body: String) {
		c.doOutput = true
		c.setRequestProperty("Content-Type", "application/json")
		c.outputStream.use { it.write(body.toByteArray()) }
	}

	/** The answer's body as text, refused when it is longer than [max] bytes: up front when it says so, else as it comes. */
	private fun readBody(c: HttpURLConnection, max: Int): String {
		checkLength(c.contentLengthLong, max)
		return String(c.inputStream.use { readCapped(it, max) }, Charsets.UTF_8)
	}

	/**
	 * A request with an optional JSON body; returns the response body, of at most [maxBytes].
	 * [finishOnCancel] keeps the connection when the caller is cancelled, as [exchange] says.
	 */
	suspend fun request(
		method: String,
		path: String,
		body: String? = null,
		timeoutMs: Int = Limits.REQUEST_TIMEOUT_MS,
		finishOnCancel: Boolean = false,
		maxBytes: Int = Limits.BODY_MAX,
	): String = exchange(path, timeoutMs, finishOnCancel) { c ->
		c.requestMethod = method
		if (body != null) send(c, body)
		checkStatus(c)
		readBody(c, maxBytes)
	}

	suspend inline fun <reified T> get(path: String, maxBytes: Int = Limits.BODY_MAX): T =
		decode(request("GET", path, maxBytes = maxBytes))

	suspend inline fun <reified B, reified T> post(
		path: String,
		body: B,
		timeoutMs: Int = Limits.REQUEST_TIMEOUT_MS,
		finishOnCancel: Boolean = false,
		maxBytes: Int = Limits.BODY_MAX,
	): T = decode(request("POST", path, ProtocolJson.encodeToString(body), timeoutMs, finishOnCancel, maxBytes))

	/**
	 * An answer read as [T], off the main thread whoever asks (a `ui` or a snapshot with a table of
	 * processes is large), as [decodeChecked] reads it: broken or nested too deep, a protocol error.
	 */
	suspend inline fun <reified T> decode(text: String): T = withContext(Dispatchers.Default) { decodeChecked(serializer<T>(), text) }

	suspend fun delete(path: String) {
		request("DELETE", path)
	}

	/** Raw bytes, for album art: at most [Limits.ART_MAX]. */
	suspend fun bytes(path: String): ByteArray = exchange(path, Limits.BYTES_TIMEOUT_MS) { c ->
		checkStatus(c)
		checkLength(c.contentLengthLong, Limits.ART_MAX)
		c.inputStream.use { readCapped(it, Limits.ART_MAX) }
	}

	suspend fun hello(): Hello = exchange("/api/v1/hello", Limits.HELLO_TIMEOUT_MS) { c ->
		checkStatus(c)
		val h = ProtocolJson.decodeFromString<Hello>(readBody(c, Limits.SMALL_BODY_MAX))
		if (h.app != "elamon") throw ElamondException(ElamondException.Kind.NotElamond)
		// Shown on the pairing screen: a name that can't pass for another, or reorder what follows it.
		h.copy(name = cleanName(h.name))
	}

	suspend fun pair(code: String, device: String, previous: PreviousPairing? = null): PairResponse = exchange("/api/v1/pair", Limits.PAIR_TIMEOUT_MS) { c ->
		c.requestMethod = "POST"
		send(c, ProtocolJson.encodeToString(PairRequest.serializer(), PairRequest(code, device, previous)))
		checkStatus(c)
		ProtocolJson.decodeFromString<PairResponse>(readBody(c, Limits.SMALL_BODY_MAX))
	}

	/**
	 * A key of this phone's own, for one whose token came with Import pairings: a new entry named
	 * [device] in the computer's list of phones, with the role of the token sent, which stays good.
	 */
	suspend fun rekey(device: String): PairResponse =
		post<RekeyRequest, PairResponse>(REKEY_PATH, RekeyRequest(device), maxBytes = Limits.SMALL_BODY_MAX)

	/** What this phone shows here, described with the widget types it draws ([WIDGET_TYPES]). */
	suspend fun ui(): UiDoc = get("$UI_PATH?widgets=" + WIDGET_TYPES.joinToString(","))

	/** One reading of [dashboard], with its graphs' history, after the service has waited for one that has everything. */
	suspend fun snapshot(dashboard: String): DataLine = get("$SNAPSHOT_PATH?dashboard=" + query(dashboard))

	/** Changes what the open stream [stream] sends, from its next line, without reconnecting. */
	suspend fun subscribe(stream: String, change: Subscribe) {
		post<Subscribe, Ok>("$STREAM_PATH/" + query(stream), change, maxBytes = Limits.SMALL_BODY_MAX)
	}

	/**
	 * Runs the action [id] with [params]. The service kills an action whose request goes away, so
	 * leaving the screen lets it finish.
	 */
	suspend fun action(id: String, params: kotlinx.serialization.json.JsonObject): ActionResult =
		post<ActionRequest, ActionResult>("$ACTIONS_PATH/" + query(id), ActionRequest(params), timeoutMs = Limits.ACTION_TIMEOUT_MS, finishOnCancel = true)

	/**
	 * The stream of [dashboard] (null: the first tab), a line every [intervalMs]: open first, then
	 * data, and ui whenever what this phone is shown changes; until cancelled.
	 */
	fun stream(dashboard: String?, intervalMs: Int): Flow<StreamLine> = flow {
		val query = listOfNotNull(dashboard?.let { "dashboard=" + query(it) }, "interval=$intervalMs").joinToString("&")
		// A silent service is a dead connection after a few missed lines.
		val c = call { open("$STREAM_PATH?$query", Limits.streamReadTimeoutMs(intervalMs)) }
		// Socket reads ignore coroutine cancellation; closing the connection ends a blocked one, and
		// the failed read that follows is that cancellation, not a lost computer.
		closeOnCancel(close = { c.disconnect() }) {
			call { checkStatus(c) }
			// Line by line, none longer than the largest the service sends.
			val reader = call { LineReader(c.inputStream, Limits.STREAM_LINE_MAX) }
			while (true) {
				currentCoroutineContext().ensureActive()
				val line = call { reader.readLine() } ?: throw ElamondException(ElamondException.Kind.Unreachable, "closed")
				if (line.isBlank()) continue
				// A kind of line from a newer service is skipped; a broken one ends the stream.
				val msg = call { decodeStreamLine(line) } ?: continue
				emit(msg)
			}
		}
	}.flowOn(Dispatchers.IO)

	companion object {
		/** A path's or a query's part, as the service reads it back. */
		fun query(s: String): String = java.net.URLEncoder.encode(s, "UTF-8").replace("+", "%20")

		fun sha256Hex(data: ByteArray): String =
			MessageDigest.getInstance("SHA-256").digest(data).joinToString("") { "%02x".format(it) }

		/** The same grouping `elamond pair` prints, for comparing by eye. */
		fun prettyFingerprint(fp: String): String =
			fp.take(Limits.FINGERPRINT_SHOWN).uppercase().chunked(Limits.FINGERPRINT_GROUP).joinToString(" ")

		/**
		 * Tries every address at once and returns the first that answers as the expected elamond.
		 * A computer has several (Wi-Fi, cable, VPN) and only some are reachable from where the phone is.
		 */
		suspend fun firstReachable(
			hosts: List<String>,
			port: Int,
			pin: String?,
			expectId: String?,
		): Pair<ElamondClient, Hello>? {
			if (hosts.isEmpty()) return null
			var certificateChanged = false
			val result = firstAnswer(hosts.distinct()) { h ->
				try {
					val client = ElamondClient(h, port, pin)
					val hello = client.hello()
					if (expectId != null && hello.id != expectId) null else client to hello
				} catch (e: ElamondException) {
					if (e.kind == ElamondException.Kind.CertificateChanged) certificateChanged = true
					null
				}
			}
			// Reached, but presenting another certificate: worth telling apart from "offline".
			if (result == null && certificateChanged) throw ElamondException(ElamondException.Kind.CertificateChanged)
			return result
		}

		/**
		 * The fingerprint to pin for the computer [expectId] that presents another certificate than
		 * [pinned]: on the first of [hosts] whose rotations lead from [pinned] to the certificate it
		 * presents. They are read over a connection that takes any certificate and remembers it, as
		 * the steps prove themselves. Null when none leads there.
		 */
		suspend fun renewedPin(hosts: List<String>, port: Int, pinned: String, expectId: String?): String? =
			firstAnswer(hosts.distinct()) { h ->
				try {
					val probe = ElamondClient(h, port, pin = null)
					if (expectId != null && probe.hello().id != expectId) return@firstAnswer null
					val chain = probe.get<List<Rotation>>(Rotations.PATH, Limits.SMALL_BODY_MAX)
					val presented = probe.seenCertificate.get() ?: return@firstAnswer null
					Rotations.follow(chain, pinned, presented)
				} catch (e: ElamondException) {
					null
				}
			}
	}
}

/** Where a phone asks for a key of its own. */
const val REKEY_PATH = "/api/v1/rekey"

const val UI_PATH = "/api/v1/ui"
const val STREAM_PATH = "/api/v1/stream"
const val SNAPSHOT_PATH = "/api/v1/snapshot"
const val ACTIONS_PATH = "/api/v1/actions"
const val ART_PATH = "/api/v1/media/art"

/**
 * The widget types this app draws, as it tells the service (`GET /api/v1/ui?widgets=`): it
 * describes the dashboards with these alone, replacing any other by the nearest of them.
 */
val WIDGET_TYPES = listOf(
	"value", "gauge", "meter", "status", "graph", "bars", "list", "table",
	"button", "toggle", "select", "slider", "text", "group", "media",
)

/**
 * Runs [probe] on all of [hosts] at once and returns the first answer that isn't null, or null when
 * none gives one. The slower probes are cancelled and waited for, so a probe has to end as soon as it
 * is cancelled (hello() closes its connection) for a dead address not to hold this up until it times out.
 */
internal suspend fun <T : Any> firstAnswer(hosts: List<String>, probe: suspend (String) -> T?): T? = coroutineScope {
	val pending = hosts.map { h -> async { probe(h) } }.toMutableList()
	var result: T? = null
	while (pending.isNotEmpty() && result == null) {
		val (done, value) = select {
			pending.forEach { d -> d.onAwait { d to it } }
		}
		pending.remove(done)
		result = value
	}
	pending.forEach { it.cancel() }
	result
}
