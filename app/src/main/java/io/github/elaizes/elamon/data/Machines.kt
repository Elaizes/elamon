package io.github.elaizes.elamon.data

import android.content.Context
import android.net.Uri
import androidx.core.content.edit
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString

const val DEFAULT_PORT = 47823

/** A paired computer. Its token never leaves the phone: sealed with a key in the Android key store, and kept out of backups. */
@Serializable
data class Machine(
	val id: String,
	val name: String,
	val hosts: List<String>,
	val port: Int,
	val fingerprint: String,
	/**
	 * The token in plain text, as apps before 0.5 kept it; the store seals one it finds. Written
	 * even when empty: an older app going back over the list still reads it (and finds the phone
	 * unpaired) rather than lose every computer.
	 */
	@EncodeDefault val token: String = "",
	/** The token sealed ([MachineStore.token] opens it), and the IV it was sealed with, in base64. */
	val tokenSealed: String? = null,
	val tokenIv: String? = null,
	val lastHost: String? = null,
	val os: String? = null,
	val addedAt: Long = 0,
	/** This phone's id in the computer's list of paired phones (`elamond clients`); unknown for pairings before 0.5. */
	val clientId: String? = null,
	/** This phone's role there as last heard, from pairing or the control panel; null without roles. */
	val role: String? = null,
) {
	/** The address that worked last time first, so reconnecting is quick. */
	fun hostsToTry(): List<String> = (listOfNotNull(lastHost) + hosts).distinct()
}

/** Where the list of paired computers is kept: one JSON text, written whole. */
interface MachineFile {
	fun read(): String?
	fun write(json: String)
}

/**
 * The paired computers, kept in [file] with their tokens sealed by [cipher]. A token is only ever
 * written sealed, but for one an app before 0.5 kept in plain text that can't be sealed yet (the
 * key store fails): that one stays as it was, so the pairing isn't lost, and each save tries again.
 */
class MachineStore(private val file: MachineFile, private val cipher: TokenCipher) {
	/** Tokens opened this run, by their sealed text: the key store is asked once for each. */
	private val opened = ConcurrentHashMap<String, String>()

	private val _machines = MutableStateFlow(load())
	val machines: StateFlow<List<Machine>> = _machines.asStateFlow()

	/** Entries paired again under a new id, by the id they had; for as long as the app runs. */
	private val replacedBy = ConcurrentHashMap<String, String>()

	/**
	 * The list as kept, its plain tokens sealed and the list written again at once. One write
	 * replaces the whole list, so an app stopped midway finds either the old list, to seal again,
	 * or the new one: never one that lost a token.
	 */
	private fun load(): List<Machine> {
		val kept = runCatching { ProtocolJson.decodeFromString<List<Machine>>(file.read() ?: "[]") }.getOrDefault(emptyList())
		val sealed = kept.map(::sealedIfPlain)
		if (sealed != kept) file.write(ProtocolJson.encodeToString(sealed))
		return sealed
	}

	private fun save(list: List<Machine>) {
		val sealed = list.map(::sealedIfPlain)
		file.write(ProtocolJson.encodeToString(sealed))
		_machines.value = sealed
	}

	/** [m] with its plain token sealed; as it is when it has none, or the key store fails. */
	private fun sealedIfPlain(m: Machine): Machine =
		if (m.token.isEmpty()) m else runCatching { sealed(m, m.token) }.getOrDefault(m)

	/** [m] with [token] sealed in it and no plain one; throws [KeystoreException] when it can't be sealed. */
	private fun sealed(m: Machine, token: String): Machine {
		val s = try {
			cipher.seal(token.toByteArray(Charsets.UTF_8))
		} catch (e: Exception) {
			throw KeystoreException(e)
		}
		val data = B64.encodeToString(s.data)
		opened[data] = token
		return m.copy(token = "", tokenSealed = data, tokenIv = B64.encodeToString(s.iv))
	}

	/**
	 * The token to send to [machine]; null when it can't be read back (the key store lost its key,
	 * or the app's data came from elsewhere), which leaves the phone as good as unpaired there. The
	 * key store opens each once a run, which may take a moment.
	 */
	fun token(machine: Machine): String? {
		if (machine.token.isNotEmpty()) return machine.token
		val data = machine.tokenSealed ?: return null
		val iv = machine.tokenIv ?: return null
		opened[data]?.let { return it }
		return try {
			String(cipher.open(Sealed(B64D.decode(data), B64D.decode(iv))), Charsets.UTF_8).also { opened[data] = it }
		} catch (e: Exception) {
			null
		}
	}

	fun get(id: String): Machine? = _machines.value.find { it.id == id }

	/**
	 * Adds a computer paired with [token], replacing an older pairing with it or the entry
	 * [replacing] (a reset computer has a new id). Throws [KeystoreException], saving nothing, when
	 * the token can't be sealed.
	 */
	fun put(machine: Machine, token: String, replacing: String? = null) {
		val m = sealed(machine, token)
		val list = _machines.value
		if (replacing != null && replacing != m.id && list.any { it.id == replacing }) replacedBy[replacing] = m.id
		save(withMachine(list, m, replacing))
	}

	/** The entry that took the place of the gone entry [id] (paired again, the computer had a new id), if one is still here. */
	fun successor(id: String): String? = successorOf(id, replacedBy) { get(it) != null }

	fun update(id: String, change: (Machine) -> Machine) {
		save(_machines.value.map { if (it.id == id) change(it) else it })
	}

	fun remove(id: String) {
		save(_machines.value.filter { it.id != id })
	}

	companion object {
		private val B64 = Base64.getEncoder()
		private val B64D = Base64.getDecoder()

		@Volatile private var instance: MachineStore? = null
		fun get(context: Context): MachineStore =
			instance ?: synchronized(this) {
				instance ?: MachineStore(PrefsMachineFile(context.applicationContext), GcmTokenCipher(KeystoreKeys)).also { instance = it }
			}
	}
}

/**
 * Still the file "servers" and the key "list", as when a Machine was called a Server (0.4 and
 * before): an updated app finds its pairings where it left them. SharedPreferences writes the file
 * anew and keeps the old one aside until the new one is complete, so a write is all or nothing.
 */
private class PrefsMachineFile(context: Context) : MachineFile {
	private val prefs = context.getSharedPreferences("servers", Context.MODE_PRIVATE)

	override fun read(): String? = prefs.getString("list", null)

	override fun write(json: String) = prefs.edit { putString("list", json) }
}

/**
 * [list] with [machine] where the entry with its id was, or else where [replacing] was, or else at
 * the end; the entries it replaces are gone.
 */
fun withMachine(list: List<Machine>, machine: Machine, replacing: String? = null): List<Machine> {
	val gone = { s: Machine -> s.id == machine.id || (replacing != null && s.id == replacing) }
	val at = list.indexOfFirst(gone)
	val rest = list.filterNot(gone)
	// Nothing before the first match is removed, so its index is where the new entry goes.
	return if (at < 0) rest + machine else rest.toMutableList().apply { add(at, machine) }
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
				val n = v.takeIf { it.length in 1..Limits.PAIR_LINK_VERSION_DIGITS && it.all { c -> c in '0'..'9' } }?.toInt()
				return if (n != null && n > LINK_VERSION) PairLink.Newer else PairLink.Invalid
			}
			val id = param("id")?.takeIf { HEX_ID.matches(it) } ?: return PairLink.Invalid
			val fp = param("fp")?.takeIf { HEX_FINGERPRINT.matches(it) }?.lowercase() ?: return PairLink.Invalid
			val code = param("code")?.takeIf { CODE.matches(it) } ?: return PairLink.Invalid
			val hosts = param("h")?.split(',')?.map { it.trim().removeSurrounding("[", "]") }?.filter { it.isNotEmpty() }.orEmpty()
			if (hosts.isEmpty() || hosts.size > Limits.PAIR_LINK_MAX_HOSTS || !hosts.all(Hosts::isValid)) return PairLink.Invalid
			val port = when (val p = param("port")) {
				null -> DEFAULT_PORT
				else -> parsePort(p) ?: return PairLink.Invalid
			}
			val name = param("name")?.filterNot { it.isISOControl() }?.trim()?.take(Limits.PAIR_LINK_MAX_NAME)?.ifEmpty { null }
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
