package ua.elamon.ui

import android.os.Build
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import ua.elamon.R
import ua.elamon.data.ElamondClient
import ua.elamon.data.ElamondException
import ua.elamon.data.Hello
import ua.elamon.data.PairTarget
import ua.elamon.data.Server
import ua.elamon.data.ServerStore

private sealed interface Step {
	data object Connecting : Step
	data class Ready(val host: String, val hello: Hello, val fingerprint: String) : Step
	data class Failed(@StringRes val message: Int) : Step
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairScreen(target: PairTarget, onBack: () -> Unit, onPaired: (Server) -> Unit, onHelp: () -> Unit) {
	val ctx = LocalContext.current
	val scope = rememberCoroutineScope()
	var step by remember { mutableStateOf<Step>(Step.Connecting) }
	var code by rememberSaveable { mutableStateOf(target.code.orEmpty()) }
	var busy by remember { mutableStateOf(false) }
	var codeError by remember { mutableStateOf<Int?>(null) }
	var attempt by remember { mutableStateOf(0) }

	LaunchedEffect(target, attempt) {
		step = Step.Connecting
		step = try {
			val reached = ElamondClient.firstReachable(target.hosts, target.port, target.fingerprint, target.id)
			if (reached == null) {
				Step.Failed(R.string.pair_unreachable)
			} else {
				val (client, hello) = reached
				val fp = client.seenCertificate.get().orEmpty()
				val prefix = target.fingerprintPrefix
				if (prefix != null && !fp.startsWith(prefix.lowercase())) Step.Failed(R.string.pair_cert_mismatch)
				else Step.Ready(client.host, hello, fp)
			}
		} catch (e: ElamondException) {
			Step.Failed(if (e.kind == ElamondException.Kind.CertificateChanged) R.string.pair_cert_mismatch else R.string.pair_unreachable)
		}
	}

	fun pair(ready: Step.Ready) {
		if (busy) return
		busy = true
		codeError = null
		scope.launch {
			try {
				// Pinned to the certificate just seen, so it can't be swapped between the two requests.
				val client = ElamondClient(ready.host, target.port, ready.fingerprint)
				val resp = client.pair(code.filter { it.isDigit() }, deviceName(ctx))
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
				ServerStore.get(ctx).put(server)
				onPaired(server)
			} catch (e: ElamondException) {
				codeError = when (e.kind) {
					ElamondException.Kind.WrongCode -> R.string.pair_wrong_code
					ElamondException.Kind.NoPairing -> R.string.pair_no_pairing
					else -> R.string.pair_unreachable
				}
			} finally {
				busy = false
			}
		}
	}

	Scaffold(
		topBar = {
			TopAppBar(
				title = { Text(stringResource(R.string.pair_title)) },
				navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) } },
			)
		},
	) { padding ->
		Column(
			Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(24.dp),
			horizontalAlignment = Alignment.CenterHorizontally,
			verticalArrangement = Arrangement.spacedBy(16.dp),
		) {
			Icon(ElIcons.Laptop, null, Modifier.size(56.dp), tint = MaterialTheme.colorScheme.primary)
			when (val s = step) {
				Step.Connecting -> {
					Text(stringResource(R.string.pair_connecting, target.name ?: target.hosts.first()), textAlign = TextAlign.Center)
					CircularProgressIndicator()
				}
				is Step.Failed -> {
					Notice(Icons.Default.Warning, stringResource(R.string.pair_failed), stringResource(s.message)) {
						Button(onClick = { attempt++ }) { Text(stringResource(R.string.retry)) }
						androidx.compose.material3.TextButton(onClick = onHelp) { Text(stringResource(R.string.troubleshooting_button)) }
					}
				}
				is Step.Ready -> {
					Text(
						stringResource(R.string.pair_with, s.hello.name),
						style = MaterialTheme.typography.headlineSmall,
						fontWeight = FontWeight.SemiBold,
						textAlign = TextAlign.Center,
					)
					Text(
						"${s.host}:${target.port}",
						style = MaterialTheme.typography.bodyMedium,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
					)
					val fromQr = target.fingerprint != null && target.code != null
					if (!fromQr) {
						if (target.fingerprint == null) {
							// Pairing by code trusts whatever answered; the person checks it is their computer.
							Text(stringResource(R.string.pair_compare), textAlign = TextAlign.Center, style = MaterialTheme.typography.bodyMedium)
							Text(
								ElamondClient.prettyFingerprint(s.fingerprint),
								fontFamily = FontFamily.Monospace,
								style = MaterialTheme.typography.titleMedium,
								textAlign = TextAlign.Center,
								modifier = Modifier
									.fillMaxWidth()
									.background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(12.dp))
									.padding(12.dp),
							)
						}
						OutlinedTextField(
							value = code,
							onValueChange = { v -> code = v.filter { it.isDigit() }.take(6) },
							label = { Text(stringResource(R.string.pair_code)) },
							singleLine = true,
							isError = codeError != null,
							supportingText = { Text(stringResource(codeError ?: R.string.pair_code_hint)) },
							keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword, imeAction = ImeAction.Done),
							keyboardActions = KeyboardActions(onDone = { if (code.length == 6) pair(s) }),
							textStyle = MaterialTheme.typography.headlineSmall.copy(textAlign = TextAlign.Center, fontFamily = FontFamily.Monospace),
							modifier = Modifier.fillMaxWidth(),
						)
					} else {
						Text(stringResource(R.string.pair_from_qr), textAlign = TextAlign.Center, style = MaterialTheme.typography.bodyMedium)
						codeError?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center) }
					}
					Button(onClick = { pair(s) }, enabled = !busy && code.length == 6, modifier = Modifier.fillMaxWidth()) {
						if (busy) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
						else Text(stringResource(R.string.pair))
					}
				}
			}
		}
	}
}

/** What the computer's list of paired phones will call this one. */
private fun deviceName(ctx: android.content.Context): String =
	runCatching { android.provider.Settings.Global.getString(ctx.contentResolver, "device_name") }.getOrNull()
		?.takeIf { it.isNotBlank() }
		?: listOf(Build.MANUFACTURER.replaceFirstChar { it.uppercase() }, Build.MODEL).distinct().joinToString(" ")
