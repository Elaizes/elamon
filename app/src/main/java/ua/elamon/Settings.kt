package ua.elamon

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

enum class ThemeMode { System, Light, Dark }

/** Everything the Settings screen changes. Immutable, so the UI can take it as one state. */
data class Prefs(
	val theme: ThemeMode = ThemeMode.System,
	/** Black instead of dark grey in dark mode; saves power on OLED screens. */
	val pureBlack: Boolean = false,
	/** Colours from the wallpaper (Android 12+). */
	val dynamicColor: Boolean = true,
	/** Frosted-glass bars and dialogs (Android 12+). */
	val blur: Boolean = true,
	val animations: Boolean = true,
	val intervalMs: Int = 1000,
	val keepScreenOn: Boolean = false,
	val fahrenheit: Boolean = false,
	/** Network speed in bits per second, the way providers state it. */
	val networkBits: Boolean = false,
	/** Ask before power, kill and other actions that can't be undone. */
	val confirmActions: Boolean = true,
	val haptics: Boolean = true,
) {
	val blurAvailable: Boolean get() = Build.VERSION.SDK_INT >= 31
	val useBlur: Boolean get() = blur && blurAvailable
}

class Settings private constructor(context: Context) {
	private val prefs: SharedPreferences = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
	private val _state = MutableStateFlow(load())
	val state: StateFlow<Prefs> = _state.asStateFlow()

	val current: Prefs get() = _state.value
	val intervalMs: Int get() = _state.value.intervalMs

	private fun load(): Prefs {
		val d = Prefs()
		return Prefs(
			theme = runCatching { ThemeMode.valueOf(prefs.getString("theme", d.theme.name)!!) }.getOrDefault(d.theme),
			pureBlack = prefs.getBoolean("pure_black", d.pureBlack),
			dynamicColor = prefs.getBoolean("dynamic_color", d.dynamicColor),
			blur = prefs.getBoolean("blur", d.blur),
			animations = prefs.getBoolean("animations", d.animations),
			intervalMs = prefs.getInt("interval_ms", d.intervalMs),
			keepScreenOn = prefs.getBoolean("keep_screen_on", d.keepScreenOn),
			fahrenheit = prefs.getBoolean("fahrenheit", d.fahrenheit),
			networkBits = prefs.getBoolean("network_bits", d.networkBits),
			confirmActions = prefs.getBoolean("confirm_actions", d.confirmActions),
			haptics = prefs.getBoolean("haptics", d.haptics),
		)
	}

	fun update(change: (Prefs) -> Prefs) {
		val p = change(_state.value)
		prefs.edit()
			.putString("theme", p.theme.name)
			.putBoolean("pure_black", p.pureBlack)
			.putBoolean("dynamic_color", p.dynamicColor)
			.putBoolean("blur", p.blur)
			.putBoolean("animations", p.animations)
			.putInt("interval_ms", p.intervalMs)
			.putBoolean("keep_screen_on", p.keepScreenOn)
			.putBoolean("fahrenheit", p.fahrenheit)
			.putBoolean("network_bits", p.networkBits)
			.putBoolean("confirm_actions", p.confirmActions)
			.putBoolean("haptics", p.haptics)
			.apply()
		_state.value = p
	}

	companion object {
		val INTERVALS = listOf(1000, 2000, 5000)

		@Volatile private var instance: Settings? = null
		fun get(context: Context): Settings =
			instance ?: synchronized(this) { instance ?: Settings(context.applicationContext).also { instance = it } }
	}
}
