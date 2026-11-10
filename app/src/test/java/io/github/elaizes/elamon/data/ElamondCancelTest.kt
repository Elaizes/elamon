package io.github.elaizes.elamon.data

import java.io.IOException
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.withTimeout

/** Requests to an address that takes the connection and then says nothing, given up on. */
class ElamondCancelTest {
	/** Accepts connections and never answers: what a half-dead address looks like from the phone. */
	private class SilentServer : AutoCloseable {
		private val server = ServerSocket(0, 0, InetAddress.getLoopbackAddress())
		private val held = CopyOnWriteArrayList<Socket>()

		/** Counted down once a client has sent the start of its TLS handshake and waits for the answer. */
		val heard = CountDownLatch(1)

		val port: Int get() = server.localPort

		init {
			thread(isDaemon = true) {
				try {
					while (true) {
						val s = server.accept()
						held += s
						if (s.getInputStream().read() >= 0) heard.countDown()
					}
				} catch (_: IOException) {
					// Closed at the end of the test.
				}
			}
		}

		override fun close() {
			server.close()
			held.forEach { runCatching { it.close() } }
		}
	}

	private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

	private fun millisSince(nanos: Long) = (System.nanoTime() - nanos) / 1_000_000

	@Test
	fun theFirstAnswerDoesNotWaitForASilentAddress() = runBlocking {
		SilentServer().use { server ->
			val silent = ElamondClient("127.0.0.1", server.port, pin = null)
			var answeredAt = 0L
			val result = withTimeout(10_000) {
				firstAnswer(listOf("silent", "quick")) { h ->
					if (h == "silent") {
						silent.hello().id
					} else {
						// Answers once the other one is stuck waiting for its handshake.
						runInterruptible(Dispatchers.IO) { server.heard.await(10, TimeUnit.SECONDS) }
						answeredAt = System.nanoTime()
						"quick"
					}
				}
			}
			assertEquals("quick", result)
			// Before the fix this waited out the hello's read timeout for the silent address.
			val waited = millisSince(answeredAt)
			assertTrue(waited < Limits.HELLO_TIMEOUT_MS / 2, "waited $waited ms for an address already given up on")
		}
		scope.cancel()
	}

	@Test
	fun aCancelledRequestEndsAtOnce() = runBlocking {
		SilentServer().use { server ->
			val client = ElamondClient("127.0.0.1", server.port, pin = null)
			val request = scope.async { client.request("GET", "/api/v1/control") }
			assertTrue(server.heard.await(10, TimeUnit.SECONDS))
			val cancelledAt = System.nanoTime()
			request.cancel()
			withTimeout(10_000) { request.join() }
			val waited = millisSince(cancelledAt)
			assertTrue(request.isCancelled)
			assertTrue(waited < Limits.REQUEST_TIMEOUT_MS / 2, "a cancelled request went on for $waited ms")
		}
		scope.cancel()
	}

	@Test
	fun anActionIsLeftToFinish() = runBlocking {
		SilentServer().use { server ->
			val client = ElamondClient("127.0.0.1", server.port, pin = null)
			val timeoutMs = 500
			val request = scope.async { client.request("POST", "/api/v1/control/action", "{}", timeoutMs, finishOnCancel = true) }
			assertTrue(server.heard.await(10, TimeUnit.SECONDS))
			val cancelledAt = System.nanoTime()
			request.cancel()
			withTimeout(10_000) { request.join() }
			// Nothing closed the connection: only the read running out of time ended it.
			val waited = millisSince(cancelledAt)
			assertTrue(waited >= timeoutMs / 2, "the action's request was cut after $waited ms")
		}
		scope.cancel()
	}
}
