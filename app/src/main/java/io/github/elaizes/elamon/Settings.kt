package io.github.elaizes.elamon

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import io.github.elaizes.elamon.data.Profile
import io.github.elaizes.elamon.data.UiCache

enum class ThemeMode(val key: String) {
	System("system"),
	Light("light"),
	Dark("dark");

	companion object {
		fun of(key: String?): ThemeMode = entries.find { it.key == key } ?: System
	}
}

/**
 * How the app looks and behaves: the profile the computer gives this phone (docs/config.md,
 * "Profiles"), read into the app's terms. Immutable, so the UI can take it as one state. Nothing of
 * it is kept in the app's settings.
 */
data class Prefs(
	val theme: ThemeMode = ThemeMode.System,
	/** Black instead of dark grey in dark mode; saves power on OLED screens. */
	val pureBlack: Boolean = false,
	/** Colours from the wallpaper (Android 12+). */
	val dynamicColor: Boolean = true,
	/** Frosted-glass bars and dialogs (Android 12+). */
	val blur: Boolean = true,
	/** Rolling numbers, moving gauges, transitions. */
	val animations: Boolean = true,
	val intervalMs: Int = Intervals.DEFAULT_MS,
	val keepScreenOn: Boolean = false,
	/** Ask before actions that ask; the service has applied it to each action's `confirm` already. */
	val confirmActions: Boolean = true,
	/** Kinds of action the phone is unlocked for first; the service has applied it to each action's `unlock` already. */
	val unlockFor: Set<Guarded> = DEFAULT_UNLOCK,
	/** How long an unlock covers later actions of its kind or a lesser one. */
	val unlockGrace: UnlockGrace = UnlockGrace.EveryTime,
	val haptics: Boolean = true,
	/** Temperatures in °F and network rates in bits: the service sends them so; shown in the settings only. */
	val fahrenheit: Boolean = false,
	val networkBits: Boolean = false,
) {
	val blurAvailable: Boolean get() = Build.VERSION.SDK_INT >= 31
	val useBlur: Boolean get() = blur && blurAvailable

	companion object {
		val DEFAULT_UNLOCK = setOf(Guarded.Power, Guarded.Nut, Guarded.Processes, Guarded.Actions, Guarded.Units)

		/** [p], the profile a computer gives this phone, as the app takes it. */
		fun of(p: Profile): Prefs = Prefs(
			theme = ThemeMode.of(p.theme),
			pureBlack = p.pureBlack,
			dynamicColor = p.android.dynamicColor,
			blur = p.android.blur,
			animations = p.animations,
			intervalMs = p.intervalMs.coerceIn(Intervals.MIN_MS, Intervals.MAX_MS),
			keepScreenOn = p.android.keepScreenOn,
			confirmActions = p.confirmActions,
			unlockFor = p.unlock.kinds.mapNotNull { k -> Guarded.entries.find { it.key == k } }.toSet(),
			unlockGrace = UnlockGrace.of(p.unlock.grace),
			haptics = p.android.haptics,
			fahrenheit = p.fahrenheit,
			networkBits = p.networkBits,
		)
	}
}

/**
 * The app's own scale, in percent, over the phone's density and font size: one for the whole app,
 * not a computer's, so it is kept on the phone (ui/Theme.kt, AppScale, applies it).
 */
object Scale {
	const val MIN = 80
	const val MAX = 140
	const val STEP = 5
	const val DEFAULT = 100

	/** The stops a slider has between its ends. */
	const val STOPS = (MAX - MIN) / STEP - 1

	/** [percent] as one of the scales there are: the nearest step, within the bounds. */
	fun of(percent: Int): Int = (Math.round(percent.toFloat() / STEP) * STEP).coerceIn(MIN, MAX)

	/** What the phone's density is multiplied by at [percent]. */
	fun factor(percent: Int): Float = of(percent) / PERCENT

	private const val PERCENT = 100f
}

/** How the app looks whatever the computers' profiles say, kept on the phone: its [scale]. */
data class Look(val scale: Int = Scale.DEFAULT)

/** How often to ask for readings, in milliseconds. */
object Intervals {
	const val DEFAULT_MS = 1000
	const val MIN_MS = 250
	const val MAX_MS = 60_000

	/** How often readings really come: the service never sends faster than it reads. */
	fun effective(profileMs: Int, serviceMs: Int?): Int = maxOf(profileMs, serviceMs ?: 0).coerceAtMost(MAX_MS)
}

/**
 * What the app keeps of its own: the state of what is on screen (folded sections, the last tab of
 * each computer) and its [look], the scale. How it looks and behaves otherwise comes from the
 * computers' profiles.
 */
class Settings private constructor(context: Context) {
	private val state: SharedPreferences = context.getSharedPreferences(UI_STATE, Context.MODE_PRIVATE)

	private val _look = MutableStateFlow(Look(scale = Scale.of(state.getInt(SCALE, Scale.DEFAULT))))

	/** The app's own look, which the whole app follows at once. */
	val look: StateFlow<Look> = _look.asStateFlow()

	fun setScale(percent: Int) {
		val scale = Scale.of(percent)
		state.edit { putInt(SCALE, scale) }
		_look.update { it.copy(scale = scale) }
	}

	/** Whether the group or table [widget] of the computer [machine] is folded; [initial] until it was folded or opened. */
	fun folded(machine: String, widget: String, initial: Boolean): Boolean = state.getBoolean("$FOLDED$machine/$widget", initial)

	fun setFolded(machine: String, widget: String, folded: Boolean) = state.edit { putBoolean("$FOLDED$machine/$widget", folded) }

	fun lastTab(machine: String): String? = state.getString("$TAB$machine", null)

	fun setLastTab(machine: String, tab: String) = state.edit { putString("$TAB$machine", tab) }

	/** A computer forgotten: what was kept of its screen goes too. */
	fun forget(machine: String) = state.edit {
		for (k in state.all.keys) if (k.startsWith("$FOLDED$machine/") || k == "$TAB$machine") remove(k)
	}

	companion object {
		private const val UI_STATE = "ui_state"
		private const val FOLDED = "folded/"
		private const val TAB = "tab/"
		private const val SCALE = "scale"

		@Volatile private var instance: Settings? = null
		fun get(context: Context): Settings =
			instance ?: synchronized(this) { instance ?: Settings(context.applicationContext).also { instance = it } }
	}
}

/**
 * What the phone kept of the computer [id] (its ui and readings, its folded cards and last tab),
 * gone: the computer was forgotten, or paired again under a new id (its service was reset).
 */
fun forgetKept(context: Context, id: String) {
	UiCache.get(context).remove(id)
	Settings.get(context).forget(id)
}

/** The entry a pairing that [replaces] one, of the computer [paired], leaves behind: none when it is the same. */
fun leftBehind(replaces: String?, paired: String): String? = replaces?.takeIf { it != paired }
