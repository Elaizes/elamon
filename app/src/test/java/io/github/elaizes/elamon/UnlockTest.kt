package io.github.elaizes.elamon

import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
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

	/** Unlocked for [kind] at [at]. */
	private fun unlockedFor(kind: Guarded, at: Long) = UnlockMemory().unlocked(kind, at)

	@Test
	fun defaultsAskForPowerEndingProcessesActionsAndPairings() {
		assertEquals(setOf(Guarded.Power, Guarded.Processes, Guarded.Actions, Guarded.Transfer), Prefs().unlockFor)
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
		for (kind in Guarded.entries.filterNot { it.fixed }) {
			assertEquals(Gate(Before.Confirm), gateFor(kind, true, none))
			assertEquals(Gate(Before.Nothing), gateFor(kind, false, none))
			assertEquals(Gate(Before.Nothing), gateFor(kind, true, none.copy(confirmActions = false)))
		}
	}

	@Test
	fun eachKindAsksOnlyWhenTurnedOn() {
		val prefs = Prefs(unlockFor = setOf(Guarded.Media), confirmActions = false)
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Media, false, prefs))
		for (kind in Guarded.entries - Guarded.Media - Guarded.Transfer) assertEquals(Gate(Before.Nothing), gateFor(kind, false, prefs))
	}

	@Test
	fun exportAndImportAlwaysAskEveryTime() {
		val transfer = Guarded.entries.single { it.fixed }
		assertEquals(Guarded.Transfer, transfer)
		// Turned off in the settings, or never in them: asked all the same.
		val off = Prefs(unlockFor = emptySet(), confirmActions = false, unlockGrace = UnlockGrace.FiveMinutes)
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Transfer, false, off))
		// No grace covers it, not even its own, a moment ago.
		val at = 10 * minute
		for (kind in Guarded.entries) {
			assertEquals(Gate(Before.Unlock), gateFor(Guarded.Transfer, true, off, now = at + 1, memory = unlockedFor(kind, at)), "$kind")
		}
		// Without a screen lock: a confirmation, as elsewhere, saying why the first time.
		assertEquals(Gate(Before.Confirm, noLockScreen = true), gateFor(Guarded.Transfer, false, off, lockScreen = false))
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Transfer, false, off, lockScreen = false, memory = UnlockMemory(explained = true)))
	}

	@Test
	fun kindsAreOrderedByWhatTheyCanDo() {
		val order = listOf(Guarded.Media, Guarded.Profile, Guarded.Units, Guarded.Actions, Guarded.Processes, Guarded.Power, Guarded.Transfer)
		assertEquals(order, Guarded.entries.sortedBy { it.level })
		assertEquals(Guarded.entries.size, Guarded.entries.map { it.level }.toSet().size)
	}

	@Test
	fun graceWindowAfterAnUnlock() {
		val prefs = Prefs(unlockGrace = UnlockGrace.OneMinute)
		val at = 10 * minute
		val power = unlockedFor(Guarded.Power, at)
		// Within the minute the action is as if unlocking weren't asked: Confirm actions decides.
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Power, true, prefs, now = at + minute - 1, memory = power))
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Actions, false, prefs, now = at, memory = power))
		// After it, unlocking again.
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, true, prefs, now = at + minute, memory = power))
		// Five minutes last five.
		val five = Prefs(unlockGrace = UnlockGrace.FiveMinutes)
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Power, true, five, now = at + 5 * minute - 1, memory = power))
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, true, five, now = at + 5 * minute, memory = power))
	}

	@Test
	fun aGraceCoversOnlyKindsAtOrBelowTheOneUnlocked() {
		val all = Prefs(unlockFor = Guarded.entries.toSet(), unlockGrace = UnlockGrace.FiveMinutes, confirmActions = false)
		val at = 10 * minute
		for (unlocked in Guarded.entries) for (kind in Guarded.entries) {
			val expected = if (!kind.fixed && kind.level <= unlocked.level) Before.Nothing else Before.Unlock
			assertEquals(Gate(expected), gateFor(kind, false, all, now = at + minute, memory = unlockedFor(unlocked, at)), "$kind after $unlocked")
		}
		// Unlocked for the volume: ending a process asks; then, unlocked for that, the volume is covered too.
		val media = unlockedFor(Guarded.Media, at)
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Processes, false, all, now = at + 1, memory = media))
		val processes = media.unlocked(Guarded.Processes, at + 2)
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Media, false, all, now = at + 3, memory = processes))
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, false, all, now = at + 3, memory = processes))
	}

	@Test
	fun theScreenGoingOffOrTheAppLeavingEndsTheGrace() {
		val all = Prefs(unlockFor = Guarded.entries.toSet(), unlockGrace = UnlockGrace.FiveMinutes, confirmActions = false)
		val at = 10 * minute
		val unlocked = unlockedFor(Guarded.Power, at).copy(explained = true)
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Media, false, all, now = at + 1, memory = unlocked))
		val forgotten = unlocked.forgotten()
		assertEquals(UnlockMemory(explained = true), forgotten)
		for (kind in Guarded.entries) assertEquals(Gate(Before.Unlock), gateFor(kind, false, all, now = at + 1, memory = forgotten), "$kind")
		assertFalse(covered(Guarded.Media, UnlockGrace.FiveMinutes, at + 1, forgotten))
	}

	@Test
	fun everyTimeHasNoGrace() {
		val at = 10 * minute
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, now = at, memory = unlockedFor(Guarded.Power, at)))
		assertFalse(withinGrace(UnlockGrace.EveryTime, at, at))
	}

	@Test
	fun graceNeedsAnUnlockAndAClockGoingForward() {
		assertFalse(withinGrace(UnlockGrace.FiveMinutes, 10 * minute, null))
		assertTrue(withinGrace(UnlockGrace.FiveMinutes, 10 * minute, 10 * minute))
		assertFalse(withinGrace(UnlockGrace.FiveMinutes, 10 * minute, 10 * minute + 1))
		// A time without the kind it was for covers nothing.
		assertFalse(covered(Guarded.Media, UnlockGrace.FiveMinutes, 10 * minute, UnlockMemory(unlockedAt = 10 * minute)))
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

	@Test
	fun askingLessIsTold() {
		val p = Prefs(unlockGrace = UnlockGrace.OneMinute)
		// A kind turned off, or a longer grace: less.
		assertTrue(asksLess(p, p.copy(unlockFor = p.unlockFor - Guarded.Power)))
		assertTrue(asksLess(p, p.copy(unlockGrace = UnlockGrace.FiveMinutes)))
		assertTrue(asksLess(p, p.copy(unlockFor = p.unlockFor - Guarded.Power + Guarded.Media)))
		// A kind turned on, a shorter grace, or anything else: not.
		assertFalse(asksLess(p, p.copy(unlockFor = p.unlockFor + Guarded.Media)))
		assertFalse(asksLess(p, p.copy(unlockGrace = UnlockGrace.EveryTime)))
		assertFalse(asksLess(p, p.copy(theme = ThemeMode.Dark, confirmActions = false)))
		assertFalse(asksLess(p, p))
	}

	@Test
	fun strongBiometricsOrTheScreenLockWeakOnlyOnAndroid9And10() {
		for (sdk in listOf(26, 27, 30, 31, 34, 36)) assertEquals(BIOMETRIC_STRONG or DEVICE_CREDENTIAL, authenticators(sdk), "$sdk")
		for (sdk in listOf(28, 29)) assertEquals(BIOMETRIC_WEAK or DEVICE_CREDENTIAL, authenticators(sdk), "$sdk")
	}
}
