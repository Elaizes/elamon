package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class UnlockTest {
	private val minute = UnlockGrace.OneMinute.ms

	private fun gateFor(
		kind: Guarded,
		confirmable: Boolean = true,
		prefs: Prefs = Prefs(),
		lockScreen: Boolean = true,
		now: Long = 10 * minute,
		memory: UnlockMemory = UnlockMemory(),
	) = gate(kind, confirmable, prefs, lockScreen, now, memory)

	@Test
	fun defaultsAskForPowerEndingProcessesAndActions() {
		assertEquals(setOf(Guarded.Power, Guarded.Processes, Guarded.Actions), Prefs().unlockFor)
		assertEquals(UnlockGrace.EveryTime, Prefs().unlockGrace)
		for (kind in Guarded.entries) {
			val expected = if (kind.byDefault) Before.Unlock else Before.Confirm
			assertEquals(Gate(expected), gateFor(kind), "$kind")
		}
	}

	@Test
	fun unlockingReplacesTheConfirmationWhateverConfirmActionsSays() {
		for (confirm in listOf(true, false)) for (confirmable in listOf(true, false)) {
			assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, confirmable, Prefs(confirmActions = confirm)))
		}
	}

	@Test
	fun withoutUnlockingConfirmActionsDecidesAsBefore() {
		val none = Prefs(unlockFor = emptySet())
		for (kind in Guarded.entries) {
			assertEquals(Gate(Before.Confirm), gateFor(kind, true, none))
			assertEquals(Gate(Before.Nothing), gateFor(kind, false, none))
			assertEquals(Gate(Before.Nothing), gateFor(kind, true, none.copy(confirmActions = false)))
		}
	}

	@Test
	fun eachKindAsksOnlyWhenTurnedOn() {
		val prefs = Prefs(unlockFor = setOf(Guarded.Media), confirmActions = false)
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Media, false, prefs))
		for (kind in Guarded.entries - Guarded.Media) assertEquals(Gate(Before.Nothing), gateFor(kind, false, prefs))
	}

	@Test
	fun graceWindowAfterAnUnlock() {
		val prefs = Prefs(unlockGrace = UnlockGrace.OneMinute)
		val at = 10 * minute
		// Within the minute the action is as if unlocking weren't asked: Confirm actions decides.
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Power, true, prefs, now = at + minute - 1, memory = UnlockMemory(unlockedAt = at)))
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Actions, false, prefs, now = at, memory = UnlockMemory(unlockedAt = at)))
		// After it, unlocking again.
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, true, prefs, now = at + minute, memory = UnlockMemory(unlockedAt = at)))
		// Five minutes last five.
		val five = Prefs(unlockGrace = UnlockGrace.FiveMinutes)
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Power, true, five, now = at + 5 * minute - 1, memory = UnlockMemory(unlockedAt = at)))
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, true, five, now = at + 5 * minute, memory = UnlockMemory(unlockedAt = at)))
	}

	@Test
	fun everyTimeHasNoGrace() {
		val at = 10 * minute
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, now = at, memory = UnlockMemory(unlockedAt = at)))
		assertFalse(withinGrace(UnlockGrace.EveryTime, at, at))
	}

	@Test
	fun graceNeedsAnUnlockAndAClockGoingForward() {
		assertFalse(withinGrace(UnlockGrace.FiveMinutes, 10 * minute, null))
		assertTrue(withinGrace(UnlockGrace.FiveMinutes, 10 * minute, 10 * minute))
		assertFalse(withinGrace(UnlockGrace.FiveMinutes, 10 * minute, 10 * minute + 1))
	}

	@Test
	fun noScreenLockConfirmsInsteadAndSaysWhyOnce() {
		// Confirmed even where Confirm actions is off, or the action isn't one it covers.
		val off = Prefs(confirmActions = false)
		assertEquals(Gate(Before.Confirm, noLockScreen = true), gateFor(Guarded.Power, false, off, lockScreen = false))
		assertEquals(Gate(Before.Confirm, noLockScreen = false), gateFor(Guarded.Power, false, off, lockScreen = false, memory = UnlockMemory(explained = true)))
		// What doesn't ask to unlock is as before.
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Media, false, off, lockScreen = false))
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Units, true, Prefs(), lockScreen = false))
	}
}
