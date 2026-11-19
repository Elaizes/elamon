package io.github.elaizes.elamon.widget

import android.content.Context
import java.io.File
import java.io.IOException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.isServiceId

/**
 * What each computer's home-screen widget shows, by the computer's id: kept in the app's cache, so
 * the widget has it after the phone restarts and while the computer can't be reached. Anything here
 * can go: the computer is read again.
 */
class WidgetStore(private val dir: File) {
	private val _shown = MutableStateFlow(readAll())

	/** Each computer's, as last read. */
	val shown: StateFlow<Map<String, WidgetShown>> = _shown.asStateFlow()

	private val _reading = MutableStateFlow(emptySet<String>())

	/** The computers being read for their widget now, in this run of the app. */
	val reading: StateFlow<Set<String>> = _reading.asStateFlow()

	private fun file(id: String): File? = if (isServiceId(id)) File(dir, "$id.json") else null

	private fun readAll(): Map<String, WidgetShown> =
		dir.listFiles { f -> f.name.endsWith(".json") }.orEmpty().mapNotNull { f ->
			val id = f.name.removeSuffix(".json")
			if (!isServiceId(id)) return@mapNotNull null
			runCatching { id to ProtocolJson.decodeFromString(WidgetShown.serializer(), f.readText()) }.getOrNull()
		}.toMap()

	fun get(id: String): WidgetShown = _shown.value[id] ?: WidgetShown()

	/** Kept for the computer [id]; written whole or not at all. */
	@Synchronized
	fun put(id: String, shown: WidgetShown) {
		val f = file(id) ?: return
		_shown.update { it + (id to shown) }
		try {
			dir.mkdirs()
			val tmp = File(dir, f.name + ".tmp")
			tmp.writeText(ProtocolJson.encodeToString(WidgetShown.serializer(), shown))
			if (!tmp.renameTo(f)) {
				f.delete()
				tmp.renameTo(f)
			}
		} catch (e: IOException) {
			// A cache: what isn't kept is read again.
		}
	}

	/** Whether the computer [id] is being read now. */
	fun reading(id: String, now: Boolean) = _reading.update { if (now) it + id else it - id }

	/** A computer forgotten: nothing of it is kept. */
	@Synchronized
	fun remove(id: String) {
		_shown.update { it - id }
		file(id)?.delete()
	}

	companion object {
		@Volatile private var instance: WidgetStore? = null
		fun get(context: Context): WidgetStore =
			instance ?: synchronized(this) {
				instance ?: WidgetStore(File(context.applicationContext.cacheDir, "widget")).also { instance = it }
			}
	}
}

/**
 * The computer a widget shows: the one chosen for it while it is still paired, else the first
 * paired (one forgotten since, or none chosen: a single computer isn't asked for); null with none.
 */
fun shownMachine(chosen: String?, paired: List<Machine>): Machine? = paired.find { it.id == chosen } ?: paired.firstOrNull()
