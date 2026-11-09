package ua.elamon.data

import android.content.Context
import android.net.Uri
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

	private fun load(): List<Server> = runCatching {
		ProtocolJson.decodeFromString<List<Server>>(prefs.getString("list", null) ?: "[]")
	}.getOrDefault(emptyList())

	private fun save(list: List<Server>) {
		prefs.edit().putString("list", ProtocolJson.encodeToString(list)).apply()
		_servers.value = list
	}

	fun get(id: String): Server? = _servers.value.find { it.id == id }

	/** Adds a computer, replacing an older pairing with it. */
	fun put(server: Server) {
		save(_servers.value.filter { it.id != server.id } + server)
	}

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
) {
	companion object {
		/** elamon://pair?v=1&id=…&name=…&port=…&fp=…&code=…&h=a,b */
		fun fromUri(uri: Uri): PairTarget? {
			if (uri.scheme != "elamon" || uri.host != "pair") return null
			val hosts = uri.getQueryParameter("h")?.split(',')?.map { it.trim() }?.filter { it.isNotEmpty() }.orEmpty()
			val fp = uri.getQueryParameter("fp")?.lowercase()?.takeIf { it.matches(Regex("[0-9a-f]{64}")) }
			val code = uri.getQueryParameter("code")?.filter { it.isDigit() }?.takeIf { it.length == 6 }
			if (hosts.isEmpty() || fp == null || code == null) return null
			return PairTarget(
				hosts = hosts.take(8),
				port = uri.getQueryParameter("port")?.toIntOrNull()?.takeIf { it in 1..65535 } ?: DEFAULT_PORT,
				name = uri.getQueryParameter("name")?.take(64),
				id = uri.getQueryParameter("id")?.take(64),
				fingerprint = fp,
				code = code,
			)
		}
	}
}
