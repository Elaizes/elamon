package ua.elamon

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** App-wide preferences. */
class Settings private constructor(context: Context) {
	private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

	private val _intervalMs = MutableStateFlow(prefs.getInt("interval_ms", 1000))
	val intervalFlow: StateFlow<Int> = _intervalMs.asStateFlow()
	val intervalMs: Int get() = _intervalMs.value

	fun setInterval(ms: Int) {
		prefs.edit().putInt("interval_ms", ms).apply()
		_intervalMs.value = ms
	}

	private val _keepScreenOn = MutableStateFlow(prefs.getBoolean("keep_screen_on", false))
	val keepScreenOn: StateFlow<Boolean> = _keepScreenOn.asStateFlow()

	fun setKeepScreenOn(on: Boolean) {
		prefs.edit().putBoolean("keep_screen_on", on).apply()
		_keepScreenOn.value = on
	}

	companion object {
		val INTERVALS = listOf(1000, 2000, 5000)

		@Volatile private var instance: Settings? = null
		fun get(context: Context): Settings =
			instance ?: synchronized(this) { instance ?: Settings(context.applicationContext).also { instance = it } }
	}
}
