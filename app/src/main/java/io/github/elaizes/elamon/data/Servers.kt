package io.github.elaizes.elamon.data

import android.content.Context
import android.net.Uri
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString

const val DEFAULT_PORT = 47823

/** A paired computer. The token never leaves the phone: backups exclude the app's data. */
@Serializable
data class Server(
	val id: String,
	val name: String,
	val hosts: List<String>,
	val port: Int,
	val fingerprint: String,
	val token: String,
	val lastHost: String? = null,
	val os: String? = null,
	val addedAt: Long = 0,
) {
	/** The address that worked last time first, so reconnecting is quick. */
	fun hostsToTry(): List<String> = (listOfNotNull(lastHost) + hosts).distinct()
}

class ServerStore(context: Context) {
	private val prefs = context.getSharedPreferences("servers", Context.MODE_PRIVATE)
	private val _servers = MutableStateFlow(load())
	val servers: StateFlow<List<Server>> = _servers.asStateFlow()

	/** Entries paired again under a new id, by the id they had; for as long as the app runs. */
	private val replacedBy = ConcurrentHashMap<String, String>()

	private fun load(): List<Server> = runCatching {
		ProtocolJson.decodeFromString<List<Server>>(prefs.getString("list", null) ?: "[]")
	}.getOrDefault(emptyList())

	private fun save(list: List<Server>) {
		prefs.edit().putString("list", ProtocolJson.encodeToString(list)).apply()
		_servers.value = list
	}

	fun get(id: String): Server? = _servers.value.find { it.id == id }

	/** Adds a computer, replacing an older pairing with it or the entry [replacing] (a reset computer has a new id). */
	fun put(server: Server, replacing: String? = null) {
		val list = _servers.value
		if (replacing != null && replacing != server.id && list.any { it.id == replacing }) replacedBy[replacing] = server.id
		save(withServer(list, server, replacing))
	}

	/** The entry that took the place of the gone entry [id] (paired again, the computer had a new id), if one is still here. */
	fun successor(id: String): String? = successorOf(id, replacedBy) { get(it) != null }

	fun update(id: String, change: (Server) -> Server) {
		save(_servers.value.map { if (it.id == id) change(it) else it })
	}

	fun remove(id: String) {
		save(_servers.value.filter { it.id != id })
	}

	companion object {
		@Volatile private var instance: ServerStore? = null
		fun get(context: Context): ServerStore =
			instance ?: synchronized(this) { instance ?: ServerStore(context.applicationContext).also { instance = it } }
	}
}

/**
 * [list] with [server] where the entry with its id was, or else where [replacing] was, or else at
 * the end; the entries it replaces are gone.
 */
fun withServer(list: List<Server>, server: Server, replacing: String? = null): List<Server> {
	val gone = { s: Server -> s.id == server.id || (replacing != null && s.id == replacing) }
	val at = list.indexOfFirst(gone)
	val rest = list.filterNot(gone)
	// Nothing before the first match is removed, so its index is where the new entry goes.
	return if (at < 0) rest + server else rest.toMutableList().apply { add(at, server) }
}

/**
 * Where the entry [id] lives on: following [replacedBy] from it, the first id that [exists]; null
 * when the trail ends, or goes round in a circle, without one.
 */
fun successorOf(id: String, replacedBy: Map<String, String>, exists: (String) -> Boolean): String? {
	val seen = mutableSetOf(id)
	var at = id
	while (true) {
		at = replacedBy[at] ?: return null
		if (!seen.add(at)) return null
		if (exists(at)) return at
	}
}

/** What `elamond pair` puts in its QR code, or what the user picked and typed. */
data class PairTarget(
	val hosts: List<String>,
	val port: Int,
	val name: String?,
	val id: String?,
	/** Full certificate SHA-256 from a QR code: nothing to compare by eye. */
	val fingerprint: String?,
	/** The first 16 hex digits announced over mDNS, checked against what the service presents. */
	val fingerprintPrefix: String? = null,
	val code: String? = null,
	/** The paired computer this pairing takes the place of, whatever id the computer has now. */
	val replaces: String? = null,
) {
	companion object {
		/** The pairing links this app reads; a newer elamond may write others. */
		const val LINK_VERSION = 1

		private val HEX_ID = Regex("[0-9a-f]{${Limits.ID_HEX}}")
		private val HEX_FINGERPRINT = Regex("[0-9a-fA-F]{${Limits.FINGERPRINT_HEX}}")
		private val CODE = Regex("[0-9]{${Limits.PAIRING_DIGITS}}")

		fun fromUri(uri: Uri): PairLink? = fromQuery(uri.scheme, uri.host) { uri.getQueryParameter(it) }

		/**
		 * elamon://pair?v=1&id=…&name=…&port=…&fp=…&code=…&h=a,b, as its scheme, host and decoded
		 * query parameters. Null when it isn't a pairing link at all. Any web page can open such a
		 * link, so every part is checked, and a link with anything out of place is refused whole.
		 */
		fun fromQuery(scheme: String?, host: String?, param: (String) -> String?): PairLink? {
			if (!scheme.equals("elamon", ignoreCase = true) || !host.equals("pair", ignoreCase = true)) return null
			val v = param("v")?.trim() ?: return PairLink.Invalid
			if (v != LINK_VERSION.toString()) {
				// A number past ours is a newer elamond; anything else is a broken link.
				val n = v.takeIf { it.length in 1..Limits.LINK_VERSION_DIGITS && it.all { c -> c in '0'..'9' } }?.toInt()
				return if (n != null && n > LINK_VERSION) PairLink.Newer else PairLink.Invalid
			}
			val id = param("id")?.takeIf { HEX_ID.matches(it) } ?: return PairLink.Invalid
			val fp = param("fp")?.takeIf { HEX_FINGERPRINT.matches(it) }?.lowercase() ?: return PairLink.Invalid
			val code = param("code")?.takeIf { CODE.matches(it) } ?: return PairLink.Invalid
			val hosts = param("h")?.split(',')?.map { it.trim().removeSurrounding("[", "]") }?.filter { it.isNotEmpty() }.orEmpty()
			if (hosts.isEmpty() || hosts.size > Limits.LINK_MAX_HOSTS || !hosts.all(Hosts::isValid)) return PairLink.Invalid
			val port = when (val p = param("port")) {
				null -> DEFAULT_PORT
				else -> parsePort(p) ?: return PairLink.Invalid
			}
			val name = param("name")?.filterNot { it.isISOControl() }?.trim()?.take(Limits.LINK_MAX_NAME)?.ifEmpty { null }
			return PairLink.Target(PairTarget(hosts = hosts.distinct(), port = port, name = name, id = id, fingerprint = fp, code = code))
		}
	}
}

/** What a pairing link turned out to be. */
sealed interface PairLink {
	data class Target(val target: PairTarget) : PairLink

	/** From a newer elamond: the app needs updating to read it. */
	data object Newer : PairLink

	/** Something is missing or out of place. */
	data object Invalid : PairLink
}
