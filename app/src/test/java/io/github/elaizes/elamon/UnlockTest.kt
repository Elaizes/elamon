package io.github.elaizes.elamon

import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * What an action waits for: its own `confirm` and `unlock` as the service sends them (the profile's
 * `confirm_actions` and `unlock.for` applied there), within the profile's `unlock.grace`.
 */
class UnlockTest {
	private val minute = UnlockGrace.OneMinute.ms

	private fun gateFor(
		kind: Guarded,
		confirm: Boolean = true,
		unlock: Boolean = false,
		grace: UnlockGrace = UnlockGrace.EveryTime,
		lockScreen: Boolean = true,
		now: Long = 10 * minute,
		memory: UnlockMemory = UnlockMemory(),
	) = gate(kind, confirm, unlock, grace, lockScreen, now, memory)

	/** Unlocked for [kind] at [at]. */
	private fun unlockedFor(kind: Guarded, at: Long) = UnlockMemory().unlocked(kind, at)

	@Test
	fun theActionSays() {
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Media, confirm = false, unlock = false))
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Actions, confirm = true, unlock = false))
		// Unlocking takes the confirmation's place, whatever confirm says.
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, confirm = true, unlock = true))
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Units, confirm = false, unlock = true))
	}

	@Test
	fun kindsByTheServicesNames() {
		assertEquals(Guarded.Power, Guarded.of("power"))
		assertEquals(Guarded.Processes, Guarded.of("signal"))
		assertEquals(Guarded.Actions, Guarded.of("actions"))
		assertEquals(Guarded.Units, Guarded.of("units"))
		assertEquals(Guarded.Profile, Guarded.of("profile"))
		assertEquals(Guarded.Media, Guarded.of("media"))
		// A newer service's kind counts as the settings' actions; the app's own is never a computer's.
		assertEquals(Guarded.Actions, Guarded.of("lights"))
		assertEquals(Guarded.Actions, Guarded.of("transfer"))
		assertEquals(UnlockGrace.OneMinute, UnlockGrace.of("1m"))
		assertEquals(UnlockGrace.FiveMinutes, UnlockGrace.of("5m"))
		assertEquals(UnlockGrace.EveryTime, UnlockGrace.of("every_time"))
		assertEquals(UnlockGrace.EveryTime, UnlockGrace.of("1h"))
	}

	@Test
	fun exportAndImportAlwaysAskEveryTime() {
		val transfer = Guarded.entries.single { it.fixed }
		assertEquals(Guarded.Transfer, transfer)
		// Never said to unlock, a long grace: asked all the same.
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Transfer, confirm = false, unlock = false, grace = UnlockGrace.FiveMinutes))
		// No grace covers it, not even its own, a moment ago.
		val at = 10 * minute
		for (kind in Guarded.entries) {
			assertEquals(Gate(Before.Unlock), gateFor(Guarded.Transfer, grace = UnlockGrace.FiveMinutes, now = at + 1, memory = unlockedFor(kind, at)), "$kind")
		}
		// Without a screen lock: a confirmation, as elsewhere, saying why the first time.
		assertEquals(Gate(Before.Confirm, noLockScreen = true), gateFor(Guarded.Transfer, confirm = false, lockScreen = false))
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Transfer, confirm = false, lockScreen = false, memory = UnlockMemory(explained = true)))
	}

	@Test
	fun kindsAreOrderedByWhatTheyCanDo() {
		// power, signal, actions, units, profile, media: an unlock covers those after it.
		val order = listOf(Guarded.Media, Guarded.Profile, Guarded.Units, Guarded.Actions, Guarded.Processes, Guarded.Power, Guarded.Transfer)
		assertEquals(order, Guarded.entries.sortedBy { it.level })
		assertEquals(Guarded.entries.size, Guarded.entries.map { it.level }.toSet().size)
	}

	@Test
	fun graceWindowAfterAnUnlock() {
		val at = 10 * minute
		val power = unlockedFor(Guarded.Power, at)
		// Within the minute the action is as if unlocking weren't asked: its confirm decides.
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Power, unlock = true, grace = UnlockGrace.OneMinute, now = at + minute - 1, memory = power))
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Actions, confirm = false, unlock = true, grace = UnlockGrace.OneMinute, now = at, memory = power))
		// After it, unlocking again.
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, unlock = true, grace = UnlockGrace.OneMinute, now = at + minute, memory = power))
		// Five minutes last five.
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Power, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + 5 * minute - 1, memory = power))
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + 5 * minute, memory = power))
	}

	@Test
	fun aGraceCoversOnlyKindsAtOrBelowTheOneUnlocked() {
		val at = 10 * minute
		for (unlocked in Guarded.entries) for (kind in Guarded.entries) {
			val expected = if (!kind.fixed && kind.level <= unlocked.level) Before.Nothing else Before.Unlock
			val g = gateFor(kind, confirm = false, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + minute, memory = unlockedFor(unlocked, at))
			assertEquals(Gate(expected), g, "$kind after $unlocked")
		}
		// Unlocked for the volume: ending a process asks; then, unlocked for that, the volume is covered too.
		val media = unlockedFor(Guarded.Media, at)
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Processes, confirm = false, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + 1, memory = media))
		val processes = media.unlocked(Guarded.Processes, at + 2)
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Media, confirm = false, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + 3, memory = processes))
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, confirm = false, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + 3, memory = processes))
	}

	@Test
	fun theScreenGoingOffOrTheAppLeavingEndsTheGrace() {
		val at = 10 * minute
		val unlocked = unlockedFor(Guarded.Power, at).copy(explained = true)
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Media, confirm = false, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + 1, memory = unlocked))
		val forgotten = unlocked.forgotten()
		assertEquals(UnlockMemory(explained = true), forgotten)
		for (kind in Guarded.entries) {
			assertEquals(Gate(Before.Unlock), gateFor(kind, confirm = false, unlock = true, grace = UnlockGrace.FiveMinutes, now = at + 1, memory = forgotten), "$kind")
		}
		assertFalse(covered(Guarded.Media, UnlockGrace.FiveMinutes, at + 1, forgotten))
	}

	@Test
	fun everyTimeHasNoGrace() {
		val at = 10 * minute
		assertEquals(Gate(Before.Unlock), gateFor(Guarded.Power, unlock = true, now = at, memory = unlockedFor(Guarded.Power, at)))
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
		// Confirmed even where the action itself wouldn't be.
		assertEquals(Gate(Before.Confirm, noLockScreen = true), gateFor(Guarded.Power, confirm = false, unlock = true, lockScreen = false))
		assertEquals(Gate(Before.Confirm, noLockScreen = false), gateFor(Guarded.Power, confirm = false, unlock = true, lockScreen = false, memory = UnlockMemory(explained = true)))
		// What doesn't ask to unlock is as before.
		assertEquals(Gate(Before.Nothing), gateFor(Guarded.Media, confirm = false, lockScreen = false))
		assertEquals(Gate(Before.Confirm), gateFor(Guarded.Units, confirm = true, lockScreen = false))
	}

	@Test
	fun strongBiometricsOrTheScreenLockWeakOnlyOnAndroid9And10() {
		for (sdk in listOf(26, 27, 30, 31, 34, 36)) assertEquals(BIOMETRIC_STRONG or DEVICE_CREDENTIAL, authenticators(sdk), "$sdk")
		for (sdk in listOf(28, 29)) assertEquals(BIOMETRIC_WEAK or DEVICE_CREDENTIAL, authenticators(sdk), "$sdk")
	}
}
