package ua.elamon

import android.app.Application
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
import ua.elamon.data.ElamondClient
import ua.elamon.data.ElamondException
import ua.elamon.data.Hello
import ua.elamon.data.Limits
import ua.elamon.data.PairTarget
import ua.elamon.data.Server
import ua.elamon.data.ServerStore

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
	val paired: Server? = null,
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
				val server = Server(
					id = resp.id.ifEmpty { ready.hello.id },
					name = resp.name.ifEmpty { ready.hello.name },
					hosts = (listOf(ready.host) + target.hosts).distinct(),
					port = target.port,
					fingerprint = ready.fingerprint,
					token = resp.token,
					lastHost = ready.host,
					addedAt = System.currentTimeMillis(),
				)
				ServerStore.get(app).put(server, replacing = target.replaces)
				_state.update { it.copy(busy = false, paired = server) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				val error = when ((e as? ElamondException)?.kind) {
					ElamondException.Kind.WrongCode -> R.string.pair_wrong_code
					ElamondException.Kind.NoPairing -> R.string.pair_no_pairing
					else -> R.string.pair_unreachable
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

/** What the computer's list of paired phones will call this one. */
private fun deviceName(app: Application): String =
	runCatching { android.provider.Settings.Global.getString(app.contentResolver, "device_name") }.getOrNull()
		?.takeIf { it.isNotBlank() }
		?: listOf(Build.MANUFACTURER.replaceFirstChar { it.uppercase() }, Build.MODEL).distinct().joinToString(" ")
