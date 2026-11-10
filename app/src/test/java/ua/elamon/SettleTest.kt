package ua.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest

@OptIn(ExperimentalCoroutinesApi::class)
class SettleTest {
	@Test
	fun firstAtOnceThenOnlyWhatStays() = runTest {
		val wanted = MutableStateFlow("cpu")
		val seen = mutableListOf<Pair<Long, String>>()
		val job = launch { wanted.settle(400).collect { seen += currentTime to it } }
		runCurrent()
		assertEquals(listOf(0L to "cpu"), seen)

		// Typing a search: each letter restarts the wait, only the last one reopens the stream.
		wanted.value = "a"
		advanceTimeBy(100)
		wanted.value = "ab"
		advanceTimeBy(399)
		runCurrent()
		assertEquals(1, seen.size)
		advanceTimeBy(2)
		runCurrent()
		assertEquals(500L to "ab", seen.last())
		assertEquals(2, seen.size)
		job.cancel()
	}
}
