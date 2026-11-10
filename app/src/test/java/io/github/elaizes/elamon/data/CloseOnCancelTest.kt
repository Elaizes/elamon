package io.github.elaizes.elamon.data

import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout

class CloseOnCancelTest {
	/** Like a socket: reading blocks until it is closed, then fails the way `call {}` reports it. */
	private class FakeSocket {
		private val closed = CountDownLatch(1)
		val reading = CountDownLatch(1)

		fun close() = closed.countDown()

		fun readLine(): String {
			reading.countDown()
			if (!closed.await(10, TimeUnit.SECONDS)) error("never closed")
			throw ElamondException(ElamondException.Kind.Unreachable, "Socket closed")
		}
	}

	private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

	@Test
	fun aReadCutShortByCancellingIsTheCancellation() = runBlocking {
		val socket = FakeSocket()
		val read = scope.async { closeOnCancel(close = socket::close) { socket.readLine() } }
		socket.reading.await(10, TimeUnit.SECONDS)
		read.cancel()
		val e = runCatching { withTimeout(10_000) { read.await() } }.exceptionOrNull()
		// Before the fix this was the ElamondException from the closed socket.
		assertIs<CancellationException>(e)
		scope.cancel()
	}

	@Test
	fun aFailureOfItsOwnStaysThatFailure() = runBlocking {
		var closed = false
		val read = scope.async {
			closeOnCancel(close = { closed = true }) { throw ElamondException(ElamondException.Kind.Unreachable, "refused") }
		}
		val e = runCatching { withTimeout(10_000) { read.await() } }.exceptionOrNull()
		assertIs<ElamondException>(e)
		assertTrue(closed, "the connection is closed whichever way the block ends")
		scope.cancel()
	}

	/** A stream as ElamondClient.stream makes it: it sends [p], then blocks in a read until cancelled. */
	private fun fakeStream(p: Int): Flow<Int> = flow {
		val socket = FakeSocket()
		closeOnCancel(close = socket::close) {
			emit(p)
			socket.readLine()
		}
	}.flowOn(Dispatchers.IO)

	@Test
	fun newParametersReopenTheStreamQuietly() = runBlocking {
		val params = MutableStateFlow(1)
		val opened = MutableStateFlow<List<Int>>(emptyList())
		// What MachineViewModel.streamOn does, on a thread of its own as on the main one, the stream on
		// Dispatchers.IO: a new value cancels the stream for the old one.
		val main = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
		val streaming = scope.async(main) {
			params.collectLatest { p -> fakeStream(p).collect { opened.value += it } }
		}
		withTimeout(10_000) { opened.first { it == listOf(1) } }
		params.value = 2
		withTimeout(10_000) { opened.first { it == listOf(1, 2) } }
		params.value = 3
		withTimeout(10_000) { opened.first { it == listOf(1, 2, 3) } }
		// Still streaming: the cancelled reads didn't end it as a failure.
		assertTrue(streaming.isActive)
		assertEquals(listOf(1, 2, 3), opened.value)
		streaming.cancel()
		main.close()
		scope.cancel()
	}
}
