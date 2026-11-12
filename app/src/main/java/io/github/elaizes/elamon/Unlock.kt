package io.github.elaizes.elamon

import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL

/**
 * Kinds of action the phone can be made to unlock for first, by the action's `kind` in the
 * computer's settings ([key]). [level] orders them by what they can do: an unlock for one covers,
 * within the profile's grace, only those at its level or below (`power`, `signal`, `actions`,
 * `units`, `profile`, `media`). A [fixed] kind is the app's own: always unlocked for, never covered
 * by a grace.
 */
enum class Guarded(val key: String, val level: Int, val fixed: Boolean = false) {
	/** Shut down, restart, sleep and lock. */
	Power("power", 5),

	/** Quit and force kill. */
	Processes("signal", 4),

	/** The actions in the computer's settings. */
	Actions("actions", 3),

	/** Starting, stopping and restarting a link's unit. */
	Units("units", 2),
	Profile("profile", 1),

	/** Playback, seeking and the volume. */
	Media("media", 0),

	/**
	 * Exporting pairings, which carries every computer's key off the phone, and importing them,
	 * which can put others in their place.
	 */
	Transfer("transfer", 6, fixed = true);

	companion object {
		/** An action's kind as the service names it; an unknown one (a newer service's) counts as the settings' actions. */
		fun of(kind: String): Guarded = entries.find { it.key == kind && !it.fixed } ?: Actions
	}
}

/** How long a successful unlock lasts before the next action asks again: the profile's `unlock.grace`. */
enum class UnlockGrace(val key: String, val minutes: Int) {
	EveryTime("every_time", 0),
	OneMinute("1m", 1),
	FiveMinutes("5m", 5);

	val ms: Long get() = minutes * MS_PER_MINUTE

	companion object {
		private const val MS_PER_MINUTE = 60_000L

		/** A grace the service names; one this app doesn't know asks every time. */
		fun of(key: String?): UnlockGrace = entries.find { it.key == key } ?: EveryTime
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
 * What an action of [kind] waits for. [confirm] and [unlock] are the action's own, as the service
 * sends them (the profile's `confirm_actions` and `unlock.for` already applied); a fixed kind always
 * unlocks. Where it unlocks and no unlock within [grace] covers it, unlocking takes the
 * confirmation's place. A phone without a screen lock can't be unlocked: it confirms instead, and
 * the first time says why.
 */
fun gate(kind: Guarded, confirm: Boolean, unlock: Boolean, grace: UnlockGrace, lockScreen: Boolean, now: Long, memory: UnlockMemory): Gate {
	val asks = kind.fixed || unlock
	val needed = asks && !covered(kind, grace, now, memory)
	return when {
		!needed -> Gate(if (confirm) Before.Confirm else Before.Nothing)
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

/**
 * What the unlock prompt takes on Android [sdk]: a strong biometric or the screen lock. Android 9
 * and 10 don't take the two together, so there any biometric the phone trusts goes with the screen
 * lock instead (AndroidX Biometric's documented exception).
 */
fun authenticators(sdk: Int): Int =
	if (sdk in WEAK_ONLY) BIOMETRIC_WEAK or DEVICE_CREDENTIAL else BIOMETRIC_STRONG or DEVICE_CREDENTIAL

private val WEAK_ONLY = 28..29
