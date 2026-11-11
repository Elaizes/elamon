package io.github.elaizes.elamon.data

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield

class OwnKeysTest {
	private fun machine(id: String) = Machine(id = id, name = id, hosts = listOf("h-$id"), port = DEFAULT_PORT, fingerprint = "f-$id")

	/** What the computer answers a key of this phone's own with: as pairing does. */
	private fun own(token: String, clientId: String = "c-$token", role: String? = "monitor") =
		PairResponse(token = token, clientId = clientId, id = "b", name = "b", role = role)

	private val tooOld = ElamondException(ElamondException.Kind.NotFound, "not_found", code = "not_found", status = 404)

	/** A phone that paired with "a" itself and imported "b" (the old phone's client id and role with it). */
	private class Phone {
		val cipher = FakeCipher()
		val store = MachineStore(MemoryFile(), cipher)
		var clock = 1_000L
		val keys = OwnKeys(store) { clock }

		init {
			store.put(Machine(id = "a", name = "a", hosts = listOf("h-a"), port = DEFAULT_PORT, fingerprint = "f-a"), "secret-a")
			store.addImported(listOf(Machine(id = "b", name = "b", hosts = listOf("h-b"), port = DEFAULT_PORT, fingerprint = "f-b", token = "secret-old", clientId = "old", role = "monitor")))
		}

		fun token(id: String) = store.token(store.get(id)!!)
	}

	@Test
	fun anImportMarksWhatItBroughtAsSharedAndPairingDoesnt() {
		val p = Phone()
		assertFalse(p.store.get("a")!!.tokenShared)
		assertTrue(p.store.get("b")!!.tokenShared)
		assertFalse(p.keys.due(p.store.get("a")!!))
		assertTrue(p.keys.due(p.store.get("b")!!))
		// Kept across a restart of the app, and an older app reading the list ignores it.
		val again = MachineStore(MemoryFile(ProtocolJson.encodeToString(kotlinx.serialization.builtins.ListSerializer(Machine.serializer()), p.store.machines.value)), p.cipher)
		assertTrue(again.get("b")!!.tokenShared)
		// Paired again with a code, the computer gave this phone its own.
		p.store.put(machine("b"), "secret-paired")
		assertFalse(p.store.get("b")!!.tokenShared)
	}

	@Test
	fun aSharedTokenIsReplacedByOneOfItsOwnOnce() = runTest {
		val p = Phone()
		val asked = mutableListOf<String>()
		val got = p.keys.renew("b", "New phone") { asked += it; own("secret-own") }
		assertEquals("secret-own", got)
		assertEquals(listOf("New phone"), asked)
		val b = p.store.get("b")!!
		assertFalse(b.tokenShared)
		assertEquals("c-secret-own" to "monitor", b.clientId to b.role)
		assertEquals("secret-own", p.token("b"))
		assertEquals("", b.token)
		// The next connection has nothing to ask, nor does a computer paired here.
		assertNull(p.keys.renew("b", "New phone") { asked += it; own("again") })
		assertNull(p.keys.renew("a", "New phone") { asked += it; own("again") })
		assertEquals(1, asked.size)
		assertEquals("secret-a", p.token("a"))
	}

	@Test
	fun anOlderServiceKeepsTheSharedTokenAndIsAskedOnceARun() = runTest {
		val p = Phone()
		var asked = 0
		assertNull(p.keys.renew("b", "New phone") { asked++; throw tooOld })
		assertTrue(p.store.get("b")!!.tokenShared)
		assertEquals("secret-old", p.token("b"))
		// Not again this run, however many connections and however long.
		p.clock += 24 * 3_600_000L
		assertNull(p.keys.renew("b", "New phone") { asked++; throw tooOld })
		assertFalse(p.keys.due(p.store.get("b")!!))
		assertEquals(1, asked)
		// The app run again asks again: the service may have been updated meanwhile.
		val nextRun = OwnKeys(p.store) { p.clock }
		assertEquals("secret-own", nextRun.renew("b", "New phone") { asked++; own("secret-own") })
		assertEquals(2, asked)
		assertFalse(p.store.get("b")!!.tokenShared)
	}

	@Test
	fun otherFailuresAreAskedAgainAtALaterConnection() = runTest {
		val p = Phone()
		var asked = 0
		val failures = listOf(
			ElamondException(ElamondException.Kind.Unreachable, "timeout"),
			ElamondException(ElamondException.Kind.TooManyRequests, "too_many_requests", code = "too_many_requests", status = 429),
			ElamondException(ElamondException.Kind.Failed, "storage", code = "storage", status = 500),
			// A 404 that isn't the service missing the path at all.
			ElamondException(ElamondException.Kind.NotFound, "HTTP 404", status = 404),
		)
		for (e in failures) {
			assertNull(p.keys.renew("b", "New phone") { asked++; throw e })
			assertTrue(p.store.get("b")!!.tokenShared)
			assertEquals("secret-old", p.token("b"))
			// The home screen connects every few seconds: not asked again at once.
			p.clock += Limits.HOME_POLL_MS
			assertNull(p.keys.renew("b", "New phone") { asked++; own("too soon") })
			p.clock += Limits.OWN_KEY_RETRY_MS
		}
		assertEquals(failures.size, asked)
		assertEquals("secret-own", p.keys.renew("b", "New phone") { asked++; own("secret-own") })
		assertEquals("secret-own", p.token("b"))
		// An answer without a token is a failure too.
		val q = Phone()
		assertNull(q.keys.renew("b", "New phone") { own("") })
		assertTrue(q.store.get("b")!!.tokenShared)
	}

	@Test
	fun twoScreensConnectingAtOnceAskOnce() = runTest {
		val p = Phone()
		val answer = CompletableDeferred<PairResponse>()
		var asked = 0
		val first = async { p.keys.renew("b", "New phone") { asked++; answer.await() } }
		val second = async { p.keys.renew("b", "New phone") { asked++; own("second") } }
		yield()
		answer.complete(own("secret-own"))
		assertEquals("secret-own", first.await())
		assertNull(second.await())
		assertEquals(1, asked)
		assertEquals("secret-own", p.token("b"))
	}

	@Test
	fun aKeyAskedForIsKeptEvenWhenTheScreenGoesMeanwhile() = runTest {
		val p = Phone()
		val answer = CompletableDeferred<PairResponse>()
		val started = CompletableDeferred<Unit>()
		val job = launch {
			p.keys.renew("b", "New phone") {
				started.complete(Unit)
				answer.await()
			}
		}
		started.await()
		job.cancel()
		answer.complete(own("secret-own"))
		job.join()
		assertEquals("secret-own", p.token("b"))
		assertFalse(p.store.get("b")!!.tokenShared)
	}

	@Test
	fun aKeyThisPhoneCantKeepIsntAskedForAgainThisRun() = runTest {
		val p = Phone()
		var asked = 0
		p.cipher.sealing = false
		assertNull(p.keys.renew("b", "New phone") { asked++; own("secret-own") })
		p.cipher.sealing = true
		p.clock += Limits.OWN_KEY_RETRY_MS * 10
		assertNull(p.keys.renew("b", "New phone") { asked++; own("again") })
		assertEquals(1, asked)
		assertEquals("secret-old", p.token("b"))
		assertTrue(p.store.get("b")!!.tokenShared)
	}

	@Test
	fun aComputerPairedAgainMeanwhileKeepsThatPairing() = runTest {
		val p = Phone()
		val got = p.keys.renew("b", "New phone") {
			p.store.put(machine("b"), "secret-paired")
			own("secret-own")
		}
		assertNull(got)
		assertEquals("secret-paired", p.token("b"))
		assertFalse(p.store.get("b")!!.tokenShared)
		assertNotEquals("c-secret-own", p.store.get("b")!!.clientId)
	}
}
