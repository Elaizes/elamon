package io.github.elaizes.elamon

import android.app.Application
import android.content.Context
import android.os.Build
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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.Hello
import io.github.elaizes.elamon.data.KeystoreException
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore

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
	@StringRes val codeError: Int? = null,
	/** Paired and saved: the screen moves on to the computer. */
	val paired: Machine? = null,
)

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
		_state.update { it.copy(step = PairStep.Connecting, codeError = null) }
		probe = viewModelScope.launch {
			val step = try {
				val reached = ElamondClient.firstReachable(target.hosts, target.port, target.fingerprint, target.id)
				if (reached == null) {
					PairStep.Failed(R.string.pair_unreachable)
				} else {
					val (client, hello) = reached
					val fp = client.seenCertificate.get().orEmpty()
					val prefix = target.fingerprintPrefix
					if (prefix != null && !fp.startsWith(prefix.lowercase())) PairStep.Failed(R.string.pair_cert_mismatch)
					else PairStep.Ready(client.host, hello, fp)
				}
			} catch (e: CancellationException) {
				throw e
			} catch (e: ElamondException) {
				PairStep.Failed(if (e.kind == ElamondException.Kind.CertificateChanged) R.string.pair_cert_mismatch else R.string.pair_unreachable)
			}
			_state.update { it.copy(step = step) }
		}
	}

	/** Asks for a token with [code]; only ever on the user's word, never by itself. */
	fun pair(code: String) {
		val s = _state.value
		val ready = s.step as? PairStep.Ready ?: return
		if (s.busy || s.paired != null) return
		val digits = code.filter { it in '0'..'9' }
		if (digits.length != Limits.PAIRING_DIGITS) return
		_state.update { it.copy(busy = true, codeError = null) }
		val app = getApplication<Application>()
		val device = deviceName(app)
		pairingScope.launch {
			try {
				// Pinned to the certificate just seen, so it can't be swapped between the two requests.
				val client = ElamondClient(ready.host, target.port, ready.fingerprint)
				val resp = client.pair(digits, device)
				val machine = Machine(
					id = resp.id.ifEmpty { ready.hello.id },
					name = resp.name.ifEmpty { ready.hello.name },
					hosts = target.hostsToKeep(ready.host),
					port = target.port,
					fingerprint = ready.fingerprint,
					lastHost = ready.host,
					addedAt = System.currentTimeMillis(),
					clientId = resp.clientId.ifEmpty { null },
					role = resp.role,
				)
				MachineStore.get(app).put(machine, resp.token, replacing = target.replaces)
				_state.update { it.copy(busy = false, paired = machine) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				val error = when ((e as? ElamondException)?.kind) {
					ElamondException.Kind.WrongCode -> R.string.pair_wrong_code
					ElamondException.Kind.NoPairing -> R.string.pair_no_pairing
					ElamondException.Kind.TooManyRequests -> R.string.pair_too_many
					// Paired on the computer, but the token couldn't be kept: nothing was saved.
					else -> if (e is KeystoreException) R.string.pair_keystore else R.string.pair_unreachable
				}
				_state.update { it.copy(busy = false, codeError = error) }
			}
		}
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
