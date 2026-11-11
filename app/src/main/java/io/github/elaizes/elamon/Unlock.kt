package io.github.elaizes.elamon

/** Kinds of action the phone can be made to unlock for first, and whether it asks by default. */
enum class Guarded(val key: String, val byDefault: Boolean) {
	/** Shut down, restart, sleep and lock. */
	Power("power", true),

	/** Quit and force kill. */
	Processes("processes", true),

	/** The actions in the computer's settings. */
	Actions("actions", true),

	/** Starting, stopping and restarting a link's unit. */
	Units("units", false),
	Profile("profile", false),

	/** Playback, seeking and the volume. */
	Media("media", false);

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

/** For as long as the app runs: when the phone was last unlocked for an action (elapsed realtime), and whether a phone without a screen lock was explained. */
data class UnlockMemory(val unlockedAt: Long? = null, val explained: Boolean = false)

/**
 * What an action of [kind] waits for. [confirmable]: whether Confirm actions covers it. Where
 * [prefs] ask to unlock for [kind], and the last unlock is older than their grace, unlocking takes
 * the confirmation's place. A phone without a screen lock can't be unlocked: it confirms instead,
 * whatever Confirm actions says, and the first time says why.
 */
fun gate(kind: Guarded, confirmable: Boolean, prefs: Prefs, lockScreen: Boolean, now: Long, memory: UnlockMemory): Gate {
	val unlock = kind in prefs.unlockFor && !withinGrace(prefs.unlockGrace, now, memory.unlockedAt)
	return when {
		!unlock -> Gate(if (confirmable && prefs.confirmActions) Before.Confirm else Before.Nothing)
		lockScreen -> Gate(Before.Unlock)
		else -> Gate(Before.Confirm, noLockScreen = !memory.explained)
	}
}

/** Whether an unlock at [unlockedAt] still counts at [now]; a clock that went back doesn't stretch it. */
fun withinGrace(grace: UnlockGrace, now: Long, unlockedAt: Long?): Boolean =
	unlockedAt != null && now >= unlockedAt && now - unlockedAt < grace.ms
