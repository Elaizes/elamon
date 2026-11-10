package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest

@OptIn(ExperimentalCoroutinesApi::class)
class PollTest {
	@Test
	fun oneRequestAtATimePerItem() = runTest {
		val running = mutableMapOf<String, Int>()
		var most = 0
		val seen = mutableListOf<Triple<Long, String, Int>>()
		var n = 0
		val job = launch {
			pollEach(
				listOf("slow", "quick"),
				everyMs = 5_000,
				probe = { item ->
					running[item] = (running[item] ?: 0) + 1
					most = maxOf(most, running.getValue(item))
					// The slow one takes longer than the interval: the next must wait for it.
					delay(if (item == "slow") 7_000 else 100)
					running[item] = running.getValue(item) - 1
					++n
				},
				report = { item, r -> seen += Triple(currentTime, item, r) },
			)
		}
		advanceTimeBy(20_000)
		runCurrent()
		job.cancel()
		assertEquals(1, most)
		// Each answer reported in the order it was asked for, the next asked [everyMs] after it.
		assertEquals(listOf(7_000L, 19_000L), seen.filter { it.second == "slow" }.map { it.first })
		assertEquals(listOf(100L, 5_200L, 10_300L, 15_400L), seen.filter { it.second == "quick" }.map { it.first })
	}

	@Test
	fun stopsWhenCancelled() = runTest {
		var asked = 0
		val job = launch { pollEach(listOf(1), everyMs = 1_000, probe = { asked++ }, report = { _, _ -> }) }
		advanceTimeBy(2_500)
		job.cancel()
		advanceTimeBy(10_000)
		assertEquals(3, asked)
	}

	@Test
	fun failuresAreNullButCancellationIsNot() {
		assertNull(orNull<Int> { throw IllegalStateException("down") })
		assertEquals(4, orNull { 2 + 2 })
		assertFailsWith<CancellationException> { orNull<Int> { throw CancellationException("gone") } }
	}
}
