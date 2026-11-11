package io.github.elaizes.elamon

import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL

/**
 * Kinds of action the phone can be made to unlock for first, and whether it asks by default.
 * [level] orders them by what they can do: an unlock for one covers, within its grace, only those
 * at its level or below. A [fixed] kind is always asked for, can't be turned off, and is never
 * covered by a grace: every time, unlocking anew.
 */
enum class Guarded(val key: String, val byDefault: Boolean, val level: Int, val fixed: Boolean = false) {
	/** Shut down, restart, sleep and lock. */
	Power("power", true, 5),

	/** Quit and force kill. */
	Processes("processes", true, 4),

	/** The actions in the computer's settings. */
	Actions("actions", true, 3),

	/** Starting, stopping and restarting a link's unit. */
	Units("units", false, 2),
	Profile("profile", false, 1),

	/** Playback, seeking and the volume. */
	Media("media", false, 0),

	/**
	 * Exporting pairings, which carries every computer's key off the phone, and importing them,
	 * which can put others in their place; and asking less here, which would outlast any grace.
	 */
	Transfer("transfer", true, 6, fixed = true);

	companion object {
		val DEFAULTS: Set<Guarded> = entries.filter { it.byDefault }.toSet()
	}
}

/** How long a successful unlock lasts before the next action asks again. */
enum class UnlockGrace(val minutes: Int) {
	EveryTime(0),
	OneMinute(1),
	FiveMinutes(5);

	val ms: Long get() = minutes * MS_PER_MINUTE

	private companion object {
		const val MS_PER_MINUTE = 60_000L
	}
}

/** What an action waits for before it runs. */
enum class Before { Nothing, Confirm, Unlock }

/** What an action waits for, and whether its confirmation first says why the phone isn't unlocked instead. */
data class Gate(val before: Before, val noLockScreen: Boolean = false)

/**
 * For as long as the app runs: when the phone was last unlocked for an action (elapsed realtime)
 * and for what kind, and whether a phone without a screen lock was explained.
 */
data class UnlockMemory(val unlockedAt: Long? = null, val unlockedFor: Guarded? = null, val explained: Boolean = false) {
	/** Unlocked for [kind] at [now]. */
	fun unlocked(kind: Guarded, now: Long): UnlockMemory = copy(unlockedAt = now, unlockedFor = kind)

	/** The screen went off or the app to the background: the next action asks again. */
	fun forgotten(): UnlockMemory = copy(unlockedAt = null, unlockedFor = null)
}

/**
 * What an action of [kind] waits for. [confirmable]: whether Confirm actions covers it. Where
 * [prefs] ask to unlock for [kind] (a fixed kind always does), and no unlock within their grace
 * covers it, unlocking takes the confirmation's place. A phone without a screen lock can't be
 * unlocked: it confirms instead, whatever Confirm actions says, and the first time says why.
 */
fun gate(kind: Guarded, confirmable: Boolean, prefs: Prefs, lockScreen: Boolean, now: Long, memory: UnlockMemory): Gate {
	val asks = kind.fixed || kind in prefs.unlockFor
	val unlock = asks && !covered(kind, prefs.unlockGrace, now, memory)
	return when {
		!unlock -> Gate(if (confirmable && prefs.confirmActions) Before.Confirm else Before.Nothing)
		lockScreen -> Gate(Before.Unlock)
		else -> Gate(Before.Confirm, noLockScreen = !memory.explained)
	}
}

/** Whether the last unlock still covers [kind] at [now]: within [grace], for a kind at its level or above; never a fixed kind. */
fun covered(kind: Guarded, grace: UnlockGrace, now: Long, memory: UnlockMemory): Boolean {
	val by = memory.unlockedFor ?: return false
	return !kind.fixed && kind.level <= by.level && withinGrace(grace, now, memory.unlockedAt)
}

/** Whether an unlock at [unlockedAt] still counts at [now]; a clock that went back doesn't stretch it. */
fun withinGrace(grace: UnlockGrace, now: Long, unlockedAt: Long?): Boolean =
	unlockedAt != null && now >= unlockedAt && now - unlockedAt < grace.ms

/** Whether changing [from] to [to] asks less: a kind no longer asked for, or a longer grace. Asking more is free. */
fun asksLess(from: Prefs, to: Prefs): Boolean =
	(from.unlockFor - to.unlockFor).isNotEmpty() || to.unlockGrace.minutes > from.unlockGrace.minutes

/**
 * What the unlock prompt takes on Android [sdk]: a strong biometric or the screen lock. Android 9
 * and 10 don't take the two together, so there any biometric the phone trusts goes with the screen
 * lock instead (AndroidX Biometric's documented exception).
 */
fun authenticators(sdk: Int): Int =
	if (sdk in WEAK_ONLY) BIOMETRIC_WEAK or DEVICE_CREDENTIAL else BIOMETRIC_STRONG or DEVICE_CREDENTIAL

private val WEAK_ONLY = 28..29
