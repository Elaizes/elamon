package io.github.elaizes.elamon

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import io.github.elaizes.elamon.data.Profile

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
		val DEFAULT_UNLOCK = setOf(Guarded.Power, Guarded.Processes, Guarded.Actions, Guarded.Units)

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
			unlockFor = p.unlock.kinds.mapNotNull { k -> Guarded.entries.find { it.key == k && !it.fixed } }.toSet(),
			unlockGrace = UnlockGrace.of(p.unlock.grace),
			haptics = p.android.haptics,
			fahrenheit = p.fahrenheit,
			networkBits = p.networkBits,
		)
	}
}

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
 * each computer) and whether the settings of 0.5 were carried over. How it looks and behaves comes
 * from the computers' profiles.
 */
class Settings private constructor(context: Context) {
	private val state: SharedPreferences = context.getSharedPreferences(UI_STATE, Context.MODE_PRIVATE)
	private val old: SharedPreferences = context.getSharedPreferences(Migration.OLD_FILE, Context.MODE_PRIVATE)

	private val _migration = MutableStateFlow(state.getString(MIGRATION_TOML, null))

	/** The profile the settings of 0.5 come to, as TOML, until the card that shows it is closed. */
	val migration: StateFlow<String?> = _migration.asStateFlow()

	init {
		migrate()
	}

	/**
	 * Once: the settings 0.5 kept on the phone, turned into a profile to put on the computers
	 * ([Migration.toml]), kept until its card is closed, and deleted.
	 */
	private fun migrate() {
		if (state.getBoolean(MIGRATED, false)) return
		val toml = Migration.toml(old.all)
		state.edit {
			if (toml != null) putString(MIGRATION_TOML, toml)
			putBoolean(MIGRATED, true)
		}
		old.edit { for (k in Migration.OLD_KEYS + Migration.unlockKeys()) remove(k) }
		_migration.value = toml
	}

	fun dismissMigration() {
		state.edit { remove(MIGRATION_TOML) }
		_migration.value = null
	}

	/** Whether the group or table [widget] of the computer [machine] is folded; [initial] until it was folded or opened. */
	fun folded(machine: String, widget: String, initial: Boolean): Boolean = state.getBoolean("$FOLDED$machine/$widget", initial)

	fun setFolded(machine: String, widget: String, folded: Boolean) = state.edit { putBoolean("$FOLDED$machine/$widget", folded) }

	/** The tab last shown for the computer [machine]. */
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
		private const val MIGRATED = "migrated_0_5"
		private const val MIGRATION_TOML = "migration_toml"

		@Volatile private var instance: Settings? = null
		fun get(context: Context): Settings =
			instance ?: synchronized(this) { instance ?: Settings(context.applicationContext).also { instance = it } }
	}
}

/**
 * The settings Elamon 0.5 kept on the phone, as the profile that gives the same on a computer
 * (docs/config.md, "Profiles"): `[profiles.phone]` starting from `default`, with only what differs.
 */
object Migration {
	/** Where 0.5 kept them. */
	const val OLD_FILE = "settings"
	const val PROFILE_ID = "phone"

	val OLD_KEYS = listOf(
		"theme", "pure_black", "dynamic_color", "blur", "animations", "interval_ms", "keep_screen_on",
		"fahrenheit", "network_bits", "confirm_actions", "unlock_grace", "haptics", "displays_open",
	)

	/** 0.5's names for the kinds an unlock was asked for, to the service's. */
	private val UNLOCK_KINDS = listOf("power" to "power", "processes" to "signal", "actions" to "actions", "units" to "units", "profile" to "profile", "media" to "media")

	fun unlockKeys(): List<String> = (UNLOCK_KINDS.map { it.first } + "transfer").map { "unlock_$it" }

	/**
	 * The TOML for [old], the settings 0.5 kept; null when there were none, or none differ from
	 * the `default` profile.
	 */
	fun toml(old: Map<String, *>): String? {
		if (old.keys.none { it in OLD_KEYS || it in unlockKeys() }) return null
		val d = Profile()
		val common = mutableListOf<String>()
		val android = mutableListOf<String>()
		fun bool(key: String, default: Boolean, into: MutableList<String>) {
			val v = old[key] as? Boolean ?: return
			if (v != default) into += "$key = $v"
		}
		(old["theme"] as? String)?.let { t ->
			val key = when (t) {
				"Light" -> "light"
				"Dark" -> "dark"
				else -> "system"
			}
			if (key != d.theme) common += "theme = \"$key\""
		}
		bool("pure_black", d.pureBlack, common)
		bool("animations", d.animations, common)
		(old["interval_ms"] as? Int)?.let { if (it != d.intervalMs) common += "interval_ms = ${it.coerceIn(Intervals.MIN_MS, Intervals.MAX_MS)}" }
		bool("fahrenheit", d.fahrenheit, common)
		bool("network_bits", d.networkBits, common)
		bool("confirm_actions", d.confirmActions, common)
		val kinds = if (UNLOCK_KINDS.any { "unlock_${it.first}" in old }) {
			// As 0.5 asked by default where a kind isn't there: power, processes and actions.
			UNLOCK_KINDS.filter { (k, _) -> old["unlock_$k"] as? Boolean ?: (k in setOf("power", "processes", "actions")) }.map { it.second }
		} else d.unlock.kinds
		val grace = when (old["unlock_grace"] as? String) {
			"OneMinute" -> "1m"
			"FiveMinutes" -> "5m"
			else -> "every_time"
		}
		if (kinds.toSet() != d.unlock.kinds.toSet() || grace != d.unlock.grace) {
			common += "unlock = { for = [${kinds.joinToString(", ") { "\"$it\"" }}], grace = \"$grace\" }"
		}
		bool("dynamic_color", d.android.dynamicColor, android)
		bool("blur", d.android.blur, android)
		bool("haptics", d.android.haptics, android)
		bool("keep_screen_on", d.android.keepScreenOn, android)
		if (common.isEmpty() && android.isEmpty()) return null
		return buildString {
			append("[profiles.$PROFILE_ID]\n")
			append("use = \"default\"\n")
			for (line in common) append(line).append('\n')
			if (android.isNotEmpty()) {
				append("\n[profiles.$PROFILE_ID.android]\n")
				for (line in android) append(line).append('\n')
			}
		}
	}
}
