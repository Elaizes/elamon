package io.github.elaizes.elamon.data

import android.content.Context
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer

/**
 * The last `ui` document of each paired computer, the last readings of its dashboards and its
 * profile, kept in the app's cache (not its settings): a computer's screen starts with them at once
 * and shows them while the computer can't be reached, and the app looks as the computer's profile
 * says before it has been asked. Anything here can go: the computer is asked again.
 */
class UiCache(private val dir: File) {
	/** Writes that outlive the screen that asked for them. */
	private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

	private val _profiles = MutableStateFlow(readProfiles())

	/** Each computer's profile as last heard, by its id. */
	val profiles: StateFlow<Map<String, Profile>> = _profiles.asStateFlow()

	private fun file(id: String, kind: String): File? {
		// An id is the service's: 32 hex characters. Anything else names no file here.
		if (!isServiceId(id)) return null
		return File(dir, "$id.$kind.json")
	}

	private fun readProfiles(): Map<String, Profile> =
		dir.listFiles { f -> f.name.endsWith(".$PROFILE.json") }.orEmpty().mapNotNull { f ->
			val id = f.name.removeSuffix(".$PROFILE.json")
			runCatching { id to ProtocolJson.decodeFromString(Profile.serializer(), f.readText()) }.getOrNull()
		}.toMap()

	/** The `ui` document last kept for the computer [id]; null when there's none, or it no longer reads. */
	suspend fun ui(id: String): UiDoc? = withContext(Dispatchers.IO) {
		val f = file(id, UI) ?: return@withContext null
		runCatching { ProtocolJson.decodeFromString(UiDoc.serializer(), f.readText()) }.getOrNull()
	}

	/** The last data line of each of its dashboards, by the dashboard's id. */
	suspend fun data(id: String): Map<String, DataLine> = withContext(Dispatchers.IO) {
		val f = file(id, DATA) ?: return@withContext emptyMap()
		runCatching { ProtocolJson.decodeFromString(DATA_LINES, f.readText()) }.getOrDefault(emptyMap())
	}

	suspend fun putUi(id: String, ui: UiDoc) {
		_profiles.update { it + (id to ui.profile) }
		withContext(Dispatchers.IO) {
			write(file(id, UI), ProtocolJson.encodeToString(UiDoc.serializer(), ui))
			write(file(id, PROFILE), ProtocolJson.encodeToString(Profile.serializer(), ui.profile))
		}
	}

	/** [putData] in the background, for a screen that is going away. */
	fun keep(id: String, data: Map<String, DataLine>) {
		// Not for a computer forgotten meanwhile: only while its ui is still kept.
		scope.launch { if (file(id, UI)?.exists() == true) putData(id, data) }
	}

	/** Its dashboards' last readings, without history: the graphs ask the service for theirs again. */
	suspend fun putData(id: String, data: Map<String, DataLine>) = withContext(Dispatchers.IO) {
		write(file(id, DATA), ProtocolJson.encodeToString(DATA_LINES, data.mapValues { it.value.copy(history = null) }))
	}

	/** A computer forgotten: nothing of it is kept. */
	fun remove(id: String) {
		_profiles.update { it - id }
		for (kind in listOf(UI, DATA, PROFILE)) file(id, kind)?.delete()
	}

	/** Written whole or not at all: to a file beside it, then moved over it. */
	private fun write(f: File?, text: String) {
		f ?: return
		try {
			dir.mkdirs()
			val tmp = File(dir, f.name + ".tmp")
			tmp.writeText(text)
			if (!tmp.renameTo(f)) {
				f.delete()
				tmp.renameTo(f)
			}
		} catch (e: IOException) {
			// A cache: what isn't kept is asked for again.
		}
	}

	companion object {
		private const val UI = "ui"
		private const val DATA = "data"
		private const val PROFILE = "profile"
		private val DATA_LINES = MapSerializer(String.serializer(), DataLine.serializer())

		@Volatile private var instance: UiCache? = null
		fun get(context: Context): UiCache =
			instance ?: synchronized(this) {
				instance ?: UiCache(File(context.applicationContext.cacheDir, "ui")).also { instance = it }
			}
	}
}
