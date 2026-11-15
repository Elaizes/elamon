package io.github.elaizes.elamon.data

import java.io.File
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.nio.file.Files
import java.security.KeyStore
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLServerSocket
import kotlin.concurrent.thread
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assume.assumeTrue

/**
 * A pairing link may carry host names besides addresses. A name that leads to another computer (a
 * stale DNS record, a name taken over) must never be taken for the one paired: the phone checks
 * the id the service answers with and the certificate it presents, as for an address. A phone
 * that forgets the computer asks it, so found, to unpair it.
 */
class NamesReachTest {
	/** An elamond stand-in: TLS with its own self-signed certificate, answering hello, and unpair as [unpair] says. */
	private class Service(val id: String, address: InetAddress, port: Int = 0) : AutoCloseable {
		private val keys = File(Files.createTempDirectory("elamon-test").toFile(), "key.p12")
		private val socket: SSLServerSocket
		val fingerprint: String

		/** Its answer to unpair, a status line and a body; null as before 0.6.2, which hasn't it. */
		@Volatile var unpair: Pair<String, String>? = null

		/** The requests it had: the method, the path, and the token when one came. */
		val asked = CopyOnWriteArrayList<String>()

		init {
			// keytool comes with the JDK the tests run on.
			val keytool = File(System.getProperty("java.home"), "bin/keytool").path
			val p = ProcessBuilder(
				keytool, "-genkeypair", "-alias", "elamond", "-keyalg", "EC", "-groupname", "secp256r1",
				"-dname", "CN=elamond", "-validity", "2", "-storetype", "PKCS12",
				"-keystore", keys.path, "-storepass", PASSWORD, "-keypass", PASSWORD,
			).redirectErrorStream(true).start()
			p.inputStream.readBytes()
			check(p.waitFor(60, TimeUnit.SECONDS) && p.exitValue() == 0) { "keytool failed" }
			val store = KeyStore.getInstance("PKCS12").apply { keys.inputStream().use { load(it, PASSWORD.toCharArray()) } }
			fingerprint = ElamondClient.sha256Hex(store.getCertificate("elamond").encoded)
			val kmf = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm()).apply { init(store, PASSWORD.toCharArray()) }
			val tls = SSLContext.getInstance("TLS").apply { init(kmf.keyManagers, null, null) }
			socket = (tls.serverSocketFactory.createServerSocket() as SSLServerSocket).apply { bind(InetSocketAddress(address, port)) }
			thread(isDaemon = true) { serve() }
		}

		val port: Int get() = socket.localPort

		private fun serve() {
			try {
				while (true) {
					val s = socket.accept()
					thread(isDaemon = true) {
						runCatching {
							s.use {
								val reader = s.inputStream.bufferedReader()
								val request = reader.readLine()?.split(' ').orEmpty()
								val headers = generateSequence { reader.readLine()?.ifEmpty { null } }.toList()
								asked += (request.take(2) + headers.filter { it.startsWith("Authorization:") }).joinToString(" ")
								val (status, body) = when (val path = request.getOrNull(1)) {
									"/api/v1/hello" -> "200 OK" to """{"app":"elamon","api":1,"id":"$id","name":"Computer $id","version":"0.5.0","min_app":"0.5.0"}"""
									else -> unpair?.takeIf { path == "/api/v1/unpair" } ?: ("404 Not Found" to """{"error":"not_found"}""")
								}
								val bytes = body.toByteArray()
								s.outputStream.write("HTTP/1.1 $status\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray() + bytes)
								s.outputStream.flush()
							}
						}
					}
				}
			} catch (_: IOException) {
				// Closed at the end of the test.
			}
		}

		override fun close() {
			socket.close()
			keys.parentFile.deleteRecursively()
		}
	}

	private val services = mutableListOf<Service>()

	private fun service(id: String, address: String, port: Int = 0) = Service(id, InetAddress.getByName(address), port).also { services += it }

	@AfterTest
	fun stop() = services.forEach { it.close() }

	private val idA = "a".repeat(Limits.ID_HEX)
	private val idB = "b".repeat(Limits.ID_HEX)

	/** "localhost" stands for a name; it leads to computer B. */
	private val loopbackName = "localhost"

	@Test
	fun aNameLeadingToAnotherComputerIsNotTakenForIt() = runBlocking {
		// Where the name leads: the name resolves to 127.0.0.1 on the JVM.
		assumeTrue(InetAddress.getByName(loopbackName).hostAddress == "127.0.0.1")
		val b = service(idB, "127.0.0.1")
		val pinA = "0".repeat(Limits.FINGERPRINT_HEX)
		withTimeout(30_000) {
			// From a QR code, A's certificate pinned: B's is refused, and as B says it is B, it is another
			// computer at an address A had, not A with a certificate changed: A is just not there.
			assertNull(ElamondClient.firstReachable(listOf(loopbackName), b.port, pinA, idA))
			// Nothing pinned yet (found on the network, A's id known): B answers with its own id.
			assertNull(ElamondClient.firstReachable(listOf(loopbackName), b.port, null, idA))
			// Paired with A: B isn't A, so A is offline.
			val machine = Machine(id = idA, name = "A", hosts = listOf(loopbackName), port = b.port, fingerprint = pinA)
			assertNull(reachPaired(machine))
			// B itself, by the same name, is B.
			val reached = assertNotNull(ElamondClient.firstReachable(listOf(loopbackName), b.port, b.fingerprint, idB))
			assertEquals(idB, reached.second.id)
		}
	}

	@Test
	fun theComputerItselfWithAnotherCertificateIsSaidToHaveChangedIt() = runBlocking {
		// A answers as A, but with another certificate than the one pinned for it.
		val a = service(idA, "127.0.0.1")
		val pinned = "0".repeat(Limits.FINGERPRINT_HEX)
		withTimeout(30_000) {
			val e = assertFailsWith<ElamondException> { ElamondClient.firstReachable(listOf("127.0.0.1"), a.port, pinned, idA) }
			assertEquals(ElamondException.Kind.CertificateChanged, e.kind)
		}
	}

	@Test
	fun theComputerIsFoundOnItsAddressWhileItsNameLeadsElsewhere() = runBlocking {
		assumeTrue(InetAddress.getByName(loopbackName).hostAddress == "127.0.0.1")
		// A on another loopback address, B on the name's, both on the same port as one computer's
		// link gives: Linux routes all of 127.0.0.0/8 to loopback, other systems may not.
		val maybeA = runCatching { service(idA, "127.0.0.2") }.getOrNull()
		assumeTrue("127.0.0.2 isn't loopback here", maybeA != null)
		val a = checkNotNull(maybeA)
		val b = runCatching { service(idB, "127.0.0.1", a.port) }.getOrNull()
		assumeTrue("port ${a.port} is taken on 127.0.0.1", b != null)
		withTimeout(30_000) {
			val reached = assertNotNull(ElamondClient.firstReachable(listOf("127.0.0.2", loopbackName), a.port, a.fingerprint, idA))
			assertEquals("127.0.0.2", reached.first.host)
			assertEquals(idA, reached.second.id)
			// The name first, as the address that worked last time might be: still A.
			val again = assertNotNull(ElamondClient.firstReachable(listOf(loopbackName, "127.0.0.2"), a.port, a.fingerprint, idA))
			assertEquals("127.0.0.2", again.first.host)
		}
	}

	@Test
	fun aPhoneThatForgetsTheComputerAsksItToUnpairIt() = runBlocking {
		val a = service(idA, "127.0.0.1")
		val machine = Machine(id = idA, name = "A", hosts = listOf("127.0.0.1"), port = a.port, fingerprint = a.fingerprint)
		withTimeout(30_000) {
			a.unpair = "200 OK" to """{"ok":true}"""
			assertTrue(unpair(machine, "t0ken"))
			assertEquals(listOf("GET /api/v1/hello", "POST /api/v1/unpair Authorization: Bearer t0ken"), a.asked)
			// Unpaired there already (elamond revoke): it is all the same.
			a.unpair = "401 Unauthorized" to """{"error":"unauthorized"}"""
			assertTrue(unpair(machine, "t0ken"))
			// Not told: an error, a service before 0.6.2, no service there.
			a.unpair = "500 Internal Server Error" to """{"error":"storage"}"""
			assertFalse(unpair(machine, "t0ken"))
			a.unpair = null
			assertFalse(unpair(machine, "t0ken"))
			assertFalse(unpair(machine.copy(port = ServerSocket(0).use { it.localPort }), "t0ken"))
			// Another certificate than the one pinned is never given the token.
			a.asked.clear()
			assertFalse(unpair(machine.copy(fingerprint = "0".repeat(Limits.FINGERPRINT_HEX)), "t0ken"))
			assertTrue(a.asked.none { "unpair" in it }, a.asked.toString())
		}
	}

	private companion object {
		const val PASSWORD = "elamon-test"
	}
}
