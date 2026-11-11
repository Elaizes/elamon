package io.github.elaizes.elamon.data

import android.annotation.SuppressLint
import android.content.Context
import android.net.Uri
import androidx.core.content.edit
import java.io.IOException
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
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
	 * The token in plain text, only in an export of pairings ([PairingsFile]) and on its way into or
	 * out of one: the store keeps it sealed, and never this.
	 */
	val token: String = "",
	/** The token sealed ([MachineStore.token] opens it), and the IV it was sealed with, in base64. */
	val tokenSealed: String? = null,
	val tokenIv: String? = null,
	/**
	 * The token was sealed bound to this entry's id ([tokenAad]). One sealed without it is sealed
	 * again bound as the list is loaded.
	 */
	val tokenBound: Boolean = false,
	val lastHost: String? = null,
	val os: String? = null,
	val addedAt: Long = 0,
	/** This phone's id in the computer's list of paired phones (`elamond clients`); null while it isn't known. */
	val clientId: String? = null,
	/** This phone's role there as last heard, from pairing or the control panel; null while it isn't known. */
	val role: String? = null,
	/**
	 * The token came with Import pairings and is still the exporting phone's too: to the computer
	 * the two phones are one entry. At the next connection that works the app asks for a key of this
	 * phone's own ([OwnKeys]) and clears this; a computer that refuses keeps it.
	 */
	val tokenShared: Boolean = false,
) {
	/**
	 * The address or name that worked last time first, so reconnecting is quick, then the others in
	 * the order the pairing link gave them: addresses, then names.
	 */
	fun hostsToTry(): List<String> = (listOfNotNull(lastHost) + hosts).distinct()
}

/** Where the list of paired computers is kept: one JSON text, written whole. */
interface MachineFile {
	fun read(): String?

	/** Written soon: what can be written again if it is lost (an address that worked, a name). */
	fun write(json: String)

	/** Written to disk before it returns, or it throws: a new token, which the computer already knows the phone by. */
	fun writeNow(json: String) = write(json)
}

/**
 * The paired computers, kept in [file] with their tokens sealed by [cipher], each bound to its
 * computer's id ([tokenAad]). A token is only ever kept sealed: an entry without a sealed one is as
 * good as unpaired.
 *
 * Changes are made one at a time, whichever thread they come from. One that writes a token writes
 * it to disk at once ([MachineFile.writeNow]): call it off the main thread.
 */
class MachineStore(private val file: MachineFile, private val cipher: TokenCipher) {
	/** Tokens opened this run, by their computer's id and their sealed text: the key store is asked once for each. */
	private val opened = ConcurrentHashMap<String, String>()

	private val _machines = MutableStateFlow(load())
	val machines: StateFlow<List<Machine>> = _machines.asStateFlow()

	/** Entries paired again under a new id, by the id they had; for as long as the app runs. */
	private val replacedBy = ConcurrentHashMap<String, String>()

	/**
	 * The list as kept, tokens sealed without their computer's id sealed again bound to it, and the
	 * list written again at once when that changed anything. One write replaces the whole list, so an
	 * app stopped midway finds either the old list, to do it again, or the new one: never one that
	 * lost a token.
	 */
	private fun load(): List<Machine> {
		val kept = runCatching { ProtocolJson.decodeFromString<List<Machine>>(file.read() ?: "[]") }.getOrDefault(emptyList())
		// Names are cleaned as they are read too, and a token is taken only sealed.
		val loaded = kept.map { boundIfUnbound(it.copy(name = cleanName(it.name), token = "")) }
		if (loaded != kept) file.write(ProtocolJson.encodeToString(loaded))
		return loaded
	}

	/** Writes [list], any token in plain text left out: only the sealed one is kept. */
	private fun save(list: List<Machine>, now: Boolean = false) {
		val kept = list.map { if (it.token.isEmpty()) it else it.copy(token = "") }
		val json = ProtocolJson.encodeToString(kept)
		if (now) file.writeNow(json) else file.write(json)
		_machines.value = kept
	}

	/**
	 * [m] whose token was sealed without its id, opened and sealed again bound to it; as it is when
	 * it can't be: the key store fails, or lost the key.
	 */
	private fun boundIfUnbound(m: Machine): Machine {
		if (m.tokenBound) return m
		val token = open(m, aad = null) ?: return m
		return runCatching { sealed(m, token) }.getOrDefault(m)
	}

	/** [m] with [token] sealed in it, bound to its id, and no plain one; throws [KeystoreException] when it can't be sealed. */
	private fun sealed(m: Machine, token: String): Machine {
		val s = try {
			cipher.seal(token.toByteArray(Charsets.UTF_8), tokenAad(m.id))
		} catch (e: Exception) {
			throw e as? KeystoreException ?: KeystoreException(e)
		}
		val data = B64.encodeToString(s.data)
		opened[cacheKey(m.id, data)] = token
		return m.copy(token = "", tokenSealed = data, tokenIv = B64.encodeToString(s.iv), tokenBound = true)
	}

	/** What [m]'s sealed token holds, opened with [aad]; null when it can't be opened. */
	private fun open(m: Machine, aad: ByteArray?): String? {
		val data = m.tokenSealed ?: return null
		val iv = m.tokenIv ?: return null
		return try {
			String(cipher.open(Sealed(B64D.decode(data), B64D.decode(iv)), aad), Charsets.UTF_8)
		} catch (e: Exception) {
			null
		}
	}

	/**
	 * The token to send to [machine]; null when it can't be read back (the key store lost its key,
	 * the app's data came from elsewhere, or the token is another computer's), which leaves the
	 * phone as good as unpaired there. The key store opens each once a run, which may take a moment.
	 */
	fun token(machine: Machine): String? {
		val data = machine.tokenSealed ?: return null
		opened[cacheKey(machine.id, data)]?.let { return it }
		// One still unbound is sealed again bound the next time the app starts.
		val aad = if (machine.tokenBound) tokenAad(machine.id) else null
		return open(machine, aad)?.also { opened[cacheKey(machine.id, data)] = it }
	}

	fun get(id: String): Machine? = _machines.value.find { it.id == id }

	/**
	 * Adds a computer paired with [token], replacing an older pairing with it or the entry
	 * [replacing] (a reset computer has a new id), on disk before it returns. Throws
	 * [KeystoreException], saving nothing, when the token can't be sealed.
	 */
	@Synchronized
	fun put(machine: Machine, token: String, replacing: String? = null) {
		val m = sealed(machine, token)
		val list = _machines.value
		if (replacing != null && replacing != m.id && list.any { it.id == replacing }) replacedBy[replacing] = m.id
		save(withMachine(list, m, replacing), now = true)
	}

	/** The entry that took the place of the gone entry [id] (paired again, the computer had a new id), if one is still here. */
	fun successor(id: String): String? = successorOf(id, replacedBy) { get(it) != null }

	@Synchronized
	fun update(id: String, change: (Machine) -> Machine) {
		save(_machines.value.map { if (it.id == id) change(it) else it })
	}

	@Synchronized
	fun remove(id: String) {
		save(_machines.value.filter { it.id != id })
	}

	/**
	 * The computer [id] pinned to the certificate [to], which its rotations lead to from [from]; only
	 * while it is still pinned to [from], so a pairing made meanwhile isn't pinned to where an
	 * older one led. Returns whether it was.
	 */
	@Synchronized
	fun repin(id: String, from: String, to: String): Boolean {
		if (get(id)?.fingerprint?.equals(from, ignoreCase = true) != true) return false
		save(_machines.value.map { if (it.id == id) it.copy(fingerprint = to) else it })
		return true
	}

	/**
	 * The computer [was]'s token replaced by [token], a key of this phone's own where it shared one
	 * ([Machine.tokenShared] cleared), with this phone's [clientId] and [role] there; only while the
	 * entry still holds the token it held as [was], so one paired again meanwhile is left as it is.
	 * Returns whether it was kept, on disk; throws [KeystoreException], saving nothing, when it can't be sealed.
	 */
	@Synchronized
	fun ownToken(was: Machine, token: String, clientId: String?, role: String?): Boolean {
		val m = get(was.id)?.takeIf { it.tokenShared && it.token == was.token && it.tokenSealed == was.tokenSealed } ?: return false
		val own = sealed(m.copy(tokenShared = false, clientId = clientId ?: m.clientId, role = role ?: m.role), token)
		save(_machines.value.map { if (it.id == own.id) own else it }, now = true)
		return true
	}

	/**
	 * Every computer with its token opened into [Machine.token], for an export, and how many were
	 * left out because this phone can no longer open their tokens. Nothing of it is kept.
	 */
	fun withTokens(): Pair<List<Machine>, Int> {
		val list = _machines.value
		val opened = list.mapNotNull { m -> token(m)?.let { m.copy(token = it, tokenSealed = null, tokenIv = null, tokenBound = false) } }
		return opened to list.size - opened.size
	}

	/**
	 * Adds imported computers, each with its token in [Machine.token]: one with an id already here
	 * takes that entry's place, the others go at the end. Every token is sealed with this phone's
	 * key before anything is saved, so a key store that fails changes nothing ([KeystoreException]),
	 * and marked as shared with the phone it came from, until the computer gives this one its own.
	 * On disk before it returns.
	 */
	@Synchronized
	fun addImported(imported: List<Machine>): Merged {
		val sealed = imported.map { sealed(it.copy(token = "", tokenSealed = null, tokenIv = null, tokenShared = true), it.token) }
		val merged = mergeImported(_machines.value, sealed)
		save(merged.list, now = true)
		return merged
	}

	companion object {
		private val B64 = Base64.getEncoder()
		private val B64D = Base64.getDecoder()

		private fun cacheKey(id: String, sealed: String) = "$id\n$sealed"

		@Volatile private var instance: MachineStore? = null
		fun get(context: Context): MachineStore =
			instance ?: synchronized(this) {
				instance ?: MachineStore(PrefsMachineFile(context.applicationContext), GcmTokenCipher(KeystoreKeys)).also { instance = it }
			}
	}
}

/**
 * The preferences file "servers", under the key "list". SharedPreferences writes the file anew and
 * keeps the one before aside until the new one is complete, so a write is all or nothing.
 */
private class PrefsMachineFile(context: Context) : MachineFile {
	private val prefs = context.getSharedPreferences("servers", Context.MODE_PRIVATE)

	override fun read(): String? = prefs.getString("list", null)

	override fun write(json: String) = prefs.edit { putString("list", json) }

	// The KTX edit {} drops commit()'s answer, which says whether the token reached the disk.
	@SuppressLint("UseKtx")
	override fun writeNow(json: String) {
		if (!prefs.edit().putString("list", json).commit()) throw IOException("the list of computers wasn't written")
	}
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
	/** What a computer paired through this keeps: every address and name in their order, [reached] among them. */
	fun hostsToKeep(reached: String): List<String> = (hosts + reached).distinct()

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
			val name = param("name")?.let { cleanName(it, Limits.PAIR_LINK_MAX_NAME) }?.ifEmpty { null }
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
