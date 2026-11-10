package ua.elamon.data

import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class CallbackQueueTest {
	@Test
	fun runsWhatIsPosted() {
		val q = CallbackQueue()
		val done = CountDownLatch(2)
		q.post { done.countDown() }
		q.post { done.countDown() }
		assertTrue(done.await(5, TimeUnit.SECONDS))
		q.close()
	}

	@Test
	fun lateCallbacksAreDropped() {
		// NsdManager can call back after discovery stopped: that must not throw or run anything.
		val q = CallbackQueue()
		val ran = AtomicInteger()
		q.close()
		q.post { ran.incrementAndGet() }
		q.close()
		assertEquals(0, ran.get())
	}

	@Test
	fun anExecutorShutDownUnderneathIsNoCrash() {
		val executor = Executors.newSingleThreadExecutor()
		val q = CallbackQueue(executor)
		executor.shutdown()
		q.post { }
	}

	@Test
	fun queuedTasksDontRunAfterClose() {
		val executor = Executors.newSingleThreadExecutor()
		val q = CallbackQueue(executor)
		val gate = CountDownLatch(1)
		val ran = AtomicInteger()
		q.post { gate.await(5, TimeUnit.SECONDS) }
		q.post { ran.incrementAndGet() }
		q.close()
		gate.countDown()
		assertTrue(executor.awaitTermination(5, TimeUnit.SECONDS))
		assertEquals(0, ran.get())
	}
}
