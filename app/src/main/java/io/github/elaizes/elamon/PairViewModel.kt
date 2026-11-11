package io.github.elaizes.elamon

import android.app.Application
import android.content.Context
import android.os.Build
import android.os.SystemClock
import androidx.annotation.StringRes
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.Hello
import io.github.elaizes.elamon.data.KeystoreException
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.PairCheck
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.cleanName
import io.github.elaizes.elamon.data.isServiceId
import io.github.elaizes.elamon.data.pairCheck
import io.github.elaizes.elamon.data.pairedId

/** Where the pairing screen is: finding the computer, then asking it for a token. */
sealed interface PairStep {
	data object Connecting : PairStep
	data class Ready(val host: String, val hello: Hello, val fingerprint: String) : PairStep
	data class Failed(@StringRes val message: Int) : PairStep
}

data class PairState(
	val step: PairStep = PairStep.Connecting,
	/** The request for a token is out. */
	val busy: Boolean = false,
	/** It has been out a while: the computer is asking whether to allow this phone. */
	val confirmOnComputer: Boolean = false,
	@StringRes val codeError: Int? = null,
	/** Pairing would forget this computer, paired here with another certificate: asked first, with both. */
	val replacing: Machine? = null,
	/** Paired and saved: the screen moves on to the computer. */
	val paired: Machine? = null,
)

/**
 * When the app was last brought to the front or handed a link (elapsed realtime): a tap meant for
 * whatever was in front before may land on the pairing screen just after, so it waits a moment.
 */
object Arrival {
	@Volatile var at: Long = 0L

	fun now() {
		at = SystemClock.elapsedRealtime()
	}
}

/**
 * The computer adds the phone as it answers the pairing request, so that request and saving its
 * answer run here, past the screen: rotating the phone or leaving can't lose the token.
 */
private val pairingScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

class PairViewModel(app: Application, private val target: PairTarget) : AndroidViewModel(app) {
	private val _state = MutableStateFlow(PairState())
	val state: StateFlow<PairState> = _state.asStateFlow()

	private var probe: Job? = null

	init {
		connect()
	}

	/** Finds the computer on one of its addresses and reads the certificate it presents. */
	fun connect() {
		probe?.cancel()
		_state.update { it.copy(step = PairStep.Connecting, codeError = null, replacing = null) }
		probe = viewModelScope.launch {
			val step = try {
				val reached = ElamondClient.firstReachable(target.hosts, target.port, target.fingerprint, target.id)
				if (reached == null) {
					PairStep.Failed(R.string.pair_unreachable)
				} else {
					val (client, hello) = reached
					val fp = client.seenCertificate.get().orEmpty()
					// An empty announcement is none: the digits are compared by eye instead.
					val prefix = target.fingerprintPrefix?.takeIf { it.isNotEmpty() }
					when {
						// Kept under it, and compared with what the computer answers to pairing.
						!isServiceId(hello.id) -> PairStep.Failed(R.string.pair_bad_id)
						fp.length != Limits.FINGERPRINT_HEX -> PairStep.Failed(R.string.pair_unreachable)
						prefix != null && !fp.startsWith(prefix.lowercase()) -> PairStep.Failed(R.string.pair_cert_mismatch)
						else -> PairStep.Ready(client.host, hello, fp)
					}
				}
			} catch (e: CancellationException) {
				throw e
			} catch (e: ElamondException) {
				PairStep.Failed(if (e.kind == ElamondException.Kind.CertificateChanged) R.string.pair_cert_mismatch else R.string.pair_unreachable)
			}
			_state.update { it.copy(step = step) }
		}
	}

	/**
	 * Asks for a token with [code]; only ever on the user's word, never by itself. A computer here
	 * under the same id with another certificate is never replaced unasked: the first call shows
	 * the warning ([PairState.replacing]), and only a call with [replaceConfirmed] goes on.
	 */
	fun pair(code: String, replaceConfirmed: Boolean = false) {
		val s = _state.value
		val ready = s.step as? PairStep.Ready ?: return
		if (s.busy || s.paired != null || !isServiceId(ready.hello.id)) return
		val digits = code.filter { it in '0'..'9' }
		if (digits.length != Limits.PAIRING_DIGITS) return
		val app = getApplication<Application>()
		val store = MachineStore.get(app)
		val check = pairCheck(ready.hello.id, ready.fingerprint, store.machines.value, target.replaces)
		if (check is PairCheck.Replaces && !replaceConfirmed) {
			_state.update { it.copy(replacing = check.existing, codeError = null) }
			return
		}
		_state.update { it.copy(busy = true, confirmOnComputer = false, codeError = null, replacing = null) }
		val device = deviceName(app)
		pairingScope.launch {
			// elamond holds the answer until the computer allows the phone: said after a moment.
			val slow = launch {
				delay(Limits.PAIR_APPROVAL_HINT_MS)
				_state.update { if (it.busy) it.copy(confirmOnComputer = true) else it }
			}
			try {
				// Pinned to the certificate just seen, so it can't be swapped between the two requests.
				val client = ElamondClient(ready.host, target.port, ready.fingerprint)
				val resp = client.pair(digits, device)
				// The answer is the computer's that said hello, or nothing of it is kept.
				val id = pairedId(ready.hello.id, resp.id)
				if (id == null) {
					_state.update { it.copy(busy = false, confirmOnComputer = false, codeError = R.string.pair_other_id) }
					return@launch
				}
				val machine = Machine(
					id = id,
					name = cleanName(resp.name).ifEmpty { ready.hello.name },
					hosts = target.hostsToKeep(ready.host),
					port = target.port,
					fingerprint = ready.fingerprint,
					lastHost = ready.host,
					addedAt = System.currentTimeMillis(),
					clientId = resp.clientId.ifEmpty { null },
					role = resp.role,
				)
				// Written to disk before the screen moves on.
				withContext(Dispatchers.IO) { store.put(machine, resp.token, replacing = target.replaces) }
				_state.update { it.copy(busy = false, confirmOnComputer = false, paired = machine) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				val error = if ((e as? ElamondException)?.code == "too_many_phones") R.string.pair_too_many_phones else when ((e as? ElamondException)?.kind) {
					ElamondException.Kind.WrongCode -> R.string.pair_wrong_code
					ElamondException.Kind.NoPairing -> R.string.pair_no_pairing
					ElamondException.Kind.TooManyRequests -> R.string.pair_too_many
					ElamondException.Kind.PairDeclined -> R.string.pair_declined
					// Paired on the computer, but the token couldn't be kept (the key store, or the disk): nothing was saved.
					else -> when (e) {
						is KeystoreException -> R.string.pair_keystore
						is ElamondException -> R.string.pair_unreachable
						else -> R.string.pair_not_saved
					}
				}
				_state.update { it.copy(busy = false, confirmOnComputer = false, codeError = error) }
			} finally {
				slow.cancel()
			}
		}
	}

	/** The warning about replacing a computer answered with Cancel: nothing is sent. */
	fun keepExisting() {
		_state.update { it.copy(replacing = null) }
	}

	class Factory(private val app: Application, private val target: PairTarget) : ViewModelProvider.Factory {
		@Suppress("UNCHECKED_CAST")
		override fun <T : ViewModel> create(modelClass: Class<T>): T = PairViewModel(app, target) as T
	}
}

/** What the computer's list of paired phones will call this one, when it pairs or asks for a key of its own. */
fun deviceName(context: Context): String =
	runCatching { android.provider.Settings.Global.getString(context.contentResolver, "device_name") }.getOrNull()
		?.takeIf { it.isNotBlank() }
		?: listOf(Build.MANUFACTURER.replaceFirstChar { it.uppercase() }, Build.MODEL).distinct().joinToString(" ")
