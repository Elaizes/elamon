package io.github.elaizes.elamon.data

import android.content.Context
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
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
 *
 * What is written for a computer goes with a [ticket] taken before asking it: forgotten meanwhile,
 * its files aren't made again by an answer still on its way.
 */
class UiCache(private val dir: File) {
	/** Writes that outlive the screen that asked for them. */
	private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

	private val _profiles = MutableStateFlow(readProfiles())

	/** Each computer's profile as last heard, by its id. */
	val profiles: StateFlow<Map<String, Profile>> = _profiles.asStateFlow()

	/** Held while a computer's files are written or removed, and its tickets looked at. */
	private val lock = Any()

	/** How often each computer was forgotten this run: a ticket from before no longer writes. */
	private val forgotten = HashMap<String, Long>()

	/** The documents read or kept this run, by computer: read from their files once, not at every look. */
	private val docs = ConcurrentHashMap<String, UiDoc>()

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

	/**
	 * What writing for the computer [id] takes: taken before asking the computer, so what it answers
	 * isn't kept once it has been forgotten (and maybe paired again) meanwhile.
	 */
	fun ticket(id: String): Long = synchronized(lock) { forgotten[id] ?: 0L }

	/** Runs [write] unless the computer [id] was forgotten since [ticket] was taken; never for what isn't a service's id. */
	private inline fun ifCurrent(id: String, ticket: Long, write: () -> Unit) = synchronized(lock) {
		if (isServiceId(id) && ticket == (forgotten[id] ?: 0L)) write()
	}

	/** The `ui` document last kept for the computer [id]; null when there's none, or it no longer reads. */
	suspend fun ui(id: String): UiDoc? {
		docs[id]?.let { return it }
		val ticket = ticket(id)
		val doc = withContext(Dispatchers.IO) {
			val f = file(id, UI) ?: return@withContext null
			runCatching { decodeChecked(UiDoc.serializer(), f.readText()) }.getOrNull()
		} ?: return null
		ifCurrent(id, ticket) { docs.putIfAbsent(id, doc) }
		return doc
	}

	/** The last data line of each of its dashboards, by the dashboard's id. */
	suspend fun data(id: String): Map<String, DataLine> = withContext(Dispatchers.IO) {
		val f = file(id, DATA) ?: return@withContext emptyMap()
		runCatching { decodeChecked(DATA_LINES, f.readText()) }.getOrDefault(emptyMap())
	}

	suspend fun putUi(id: String, ui: UiDoc, ticket: Long = ticket(id)) = withContext(Dispatchers.IO) {
		ifCurrent(id, ticket) {
			docs[id] = ui
			_profiles.update { it + (id to ui.profile) }
			write(file(id, UI), ProtocolJson.encodeToString(UiDoc.serializer(), ui))
			write(file(id, PROFILE), ProtocolJson.encodeToString(Profile.serializer(), ui.profile))
		}
	}

	/** [putData] in the background, for a screen that is going away. */
	fun keep(id: String, data: Map<String, DataLine>, ticket: Long) {
		scope.launch { putData(id, data, ticket) }
	}

	/** Its dashboards' last readings, without history: the graphs ask the service for theirs again. */
	suspend fun putData(id: String, data: Map<String, DataLine>, ticket: Long = ticket(id)) = withContext(Dispatchers.IO) {
		ifCurrent(id, ticket) {
			write(file(id, DATA), ProtocolJson.encodeToString(DATA_LINES, data.mapValues { it.value.copy(history = null) }))
		}
	}

	/** A computer forgotten: nothing of it is kept, nor written for it by an answer asked for before. */
	fun remove(id: String) = synchronized(lock) {
		forgotten[id] = (forgotten[id] ?: 0L) + 1
		docs.remove(id)
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
