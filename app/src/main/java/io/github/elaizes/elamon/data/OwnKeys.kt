package io.github.elaizes.elamon.data

import android.content.Context
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * A key of this phone's own for each computer whose token came with Import pairings, and so is
 * still the exporting phone's too ([Machine.tokenShared]): to the computer the two phones are one
 * entry in `elamond clients`, and revoking it cuts off both. At the first connection to it that
 * works, the app asks it for a new entry beside that one (`POST /api/v1/rekey`), keeps the new
 * token sealed with this phone's key and clears the mark; the old phone keeps working until it is
 * revoked on its own.
 *
 * A service that doesn't know the request (elamond before 0.5) answers 404 `not_found`: the token
 * stays shared, and the mark with it, and that computer isn't asked again until the app runs again.
 * Any other failure is asked again at a later connection, [Limits.OWN_KEY_RETRY_MS] after it at the
 * soonest. [now] is the clock for that, in milliseconds.
 */
class OwnKeys(private val store: MachineStore, private val now: () -> Long = System::currentTimeMillis) {
	/** Computers not asked again this run: their service is too old, or this phone couldn't keep the key it got. */
	private val notThisRun: MutableSet<String> = ConcurrentHashMap.newKeySet()

	/** When asking each computer last failed otherwise. */
	private val failedAt = ConcurrentHashMap<String, Long>()

	/** One request at a time to each computer, so two screens connecting at once ask once. */
	private val locks = ConcurrentHashMap<String, Mutex>()

	/** Whether a connection to [machine] that works now asks it for a key of this phone's own. */
	fun due(machine: Machine): Boolean =
		machine.tokenShared && machine.id !in notThisRun && failedAt[machine.id]?.let { now() - it < Limits.OWN_KEY_RETRY_MS } != true

	/**
	 * Called once a connection to the computer [id] works: when it is [due], asks it through [rekey]
	 * for a key of this phone's own, named [device] there, and keeps it. Returns the new token, to use
	 * from now on, or null when there is none: not due, or asking failed. Once asked, it is seen
	 * through even if the caller is cancelled meanwhile, so a key the computer made isn't left unkept.
	 */
	suspend fun renew(id: String, device: String, rekey: suspend (device: String) -> PairResponse): String? {
		if (store.get(id)?.let(::due) != true) return null
		return locks.computeIfAbsent(id) { Mutex() }.withLock {
			// Another screen may have asked while this one waited.
			val machine = store.get(id)?.takeIf(::due) ?: return@withLock null
			withContext(NonCancellable) { ask(machine, device, rekey) }
		}
	}

	private suspend fun ask(machine: Machine, device: String, rekey: suspend (String) -> PairResponse): String? {
		val resp = try {
			rekey(device)
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			if ((e as? ElamondException)?.unknownPath == true) notThisRun += machine.id else failedAt[machine.id] = now()
			return null
		}
		if (resp.token.isEmpty()) {
			failedAt[machine.id] = now()
			return null
		}
		return try {
			if (store.ownToken(machine, resp.token, resp.clientId.ifEmpty { null }, resp.role)) resp.token else null
		} catch (e: KeystoreException) {
			// The computer made the entry, but this phone can't keep its token: asking again would only add another.
			notThisRun += machine.id
			null
		}
	}

	companion object {
		@Volatile private var instance: OwnKeys? = null
		fun get(context: Context): OwnKeys =
			instance ?: synchronized(this) {
				instance ?: OwnKeys(MachineStore.get(context)).also { instance = it }
			}
	}
}
