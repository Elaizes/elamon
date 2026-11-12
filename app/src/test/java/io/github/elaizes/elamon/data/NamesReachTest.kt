package io.github.elaizes.elamon.data

import java.io.File
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.nio.file.Files
import java.security.KeyStore
import java.util.concurrent.TimeUnit
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLServerSocket
import kotlin.concurrent.thread
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assume.assumeTrue

/**
 * A pairing link may carry host names besides addresses. A name that leads to another computer (a
 * stale DNS record, a name taken over) must never be taken for the one paired: the phone checks
 * the id the service answers with and the certificate it presents, as for an address.
 */
class NamesReachTest {
	/** An elamond stand-in: TLS with its own self-signed certificate, answering hello and rotations. */
	private class Service(val id: String, address: InetAddress, port: Int = 0) : AutoCloseable {
		private val keys = File(Files.createTempDirectory("elamon-test").toFile(), "key.p12")
		private val socket: SSLServerSocket
		val fingerprint: String

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
								val path = reader.readLine()?.split(' ')?.getOrNull(1).orEmpty()
								while (!reader.readLine().isNullOrEmpty()) { /* the headers */ }
								val body = when (path) {
									"/api/v1/hello" -> """{"app":"elamon","api":1,"id":"$id","name":"Computer $id","version":"0.5.0","min_app":"0.5.0"}"""
									Rotations.PATH -> "[]"
									else -> """{"error":"not_found"}"""
								}
								val status = if (body.contains("not_found")) "404 Not Found" else "200 OK"
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
			// Paired with A: B isn't A, so A is offline, and nothing new is pinned.
			val machine = Machine(id = idA, name = "A", hosts = listOf(loopbackName), port = b.port, fingerprint = pinA, token = "t")
			var repinned: String? = null
			assertNull(reachPaired(machine) { _, to -> repinned = to })
			assertNull(repinned)
			// B itself, by the same name, is B.
			val reached = assertNotNull(ElamondClient.firstReachable(listOf(loopbackName), b.port, b.fingerprint, idB))
			assertEquals(idB, reached.second.id)
		}
	}

	@Test
	fun theComputerItselfWithAnotherCertificateIsSaidToHaveChangedIt() = runBlocking {
		// A answers as A, but with another certificate than the one pinned for it, and nothing leads to it.
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

	private companion object {
		const val PASSWORD = "elamon-test"
	}
}
