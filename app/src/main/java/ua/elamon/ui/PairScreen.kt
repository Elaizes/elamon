package ua.elamon.ui

import android.app.Application
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
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
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberUpdatedState
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
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import ua.elamon.PairStep
import ua.elamon.PairViewModel
import ua.elamon.R
import ua.elamon.data.ElamondClient
import ua.elamon.data.Limits
import ua.elamon.data.PairTarget
import ua.elamon.data.Server

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairScreen(target: PairTarget, owner: ViewModelStoreOwner, onBack: () -> Unit, onPaired: (Server) -> Unit, onHelp: () -> Unit) {
	val ctx = LocalContext.current
	val vm: PairViewModel = viewModel(
		viewModelStoreOwner = owner,
		factory = PairViewModel.Factory(ctx.applicationContext as Application, target),
	)
	val state by vm.state.collectAsStateWithLifecycle()
	var code by rememberSaveable { mutableStateOf(target.code.orEmpty()) }

	val paired by rememberUpdatedState(onPaired)
	state.paired?.let { server -> LaunchedEffect(server) { paired(server) } }

	Scaffold(
		topBar = {
			TopAppBar(
				title = { Text(stringResource(R.string.pair_title)) },
				navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) } },
			)
		},
	) { padding ->
		Column(
			// Above the keyboard too, so the code and the button can be scrolled into sight while typing.
			Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding).imePadding().verticalScroll(rememberScrollState()).padding(Space.xxl),
			horizontalAlignment = Alignment.CenterHorizontally,
			verticalArrangement = Arrangement.spacedBy(Space.l),
		) {
			Icon(ElIcons.Laptop, null, Modifier.size(Sizes.pairIcon), tint = MaterialTheme.colorScheme.primary)
			when (val s = state.step) {
				PairStep.Connecting -> {
					Text(stringResource(R.string.pair_connecting, target.name ?: target.hosts.first()), textAlign = TextAlign.Center)
					CircularProgressIndicator()
				}
				is PairStep.Failed -> {
					Notice(Icons.Default.Warning, stringResource(R.string.pair_failed), stringResource(s.message)) {
						Button(onClick = { vm.connect() }) { Text(stringResource(R.string.retry)) }
						TextButton(onClick = onHelp) { Text(stringResource(R.string.troubleshooting_button)) }
					}
				}
				is PairStep.Ready -> {
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
									.background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(Radius.m))
									.padding(Space.m),
							)
						}
						OutlinedTextField(
							value = code,
							onValueChange = { v -> code = v.filter { it in '0'..'9' }.take(Limits.PAIRING_DIGITS) },
							label = { Text(stringResource(R.string.pair_code)) },
							singleLine = true,
							isError = state.codeError != null,
							supportingText = { Text(stringResource(state.codeError ?: R.string.pair_code_hint)) },
							keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword, imeAction = ImeAction.Done),
							keyboardActions = KeyboardActions(onDone = { if (code.length == Limits.PAIRING_DIGITS) vm.pair(code) }),
							textStyle = MaterialTheme.typography.headlineSmall.copy(textAlign = TextAlign.Center, fontFamily = FontFamily.Monospace),
							modifier = Modifier.fillMaxWidth(),
						)
					} else {
						Text(stringResource(R.string.pair_from_qr), textAlign = TextAlign.Center, style = MaterialTheme.typography.bodyMedium)
						state.codeError?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center) }
					}
					val busy = state.busy || state.paired != null
					Button(onClick = { vm.pair(code) }, enabled = !busy && code.length == Limits.PAIRING_DIGITS, modifier = Modifier.fillMaxWidth()) {
						if (busy) CircularProgressIndicator(Modifier.size(Sizes.iconM), strokeWidth = Sizes.spinnerStroke)
						else Text(stringResource(R.string.pair))
					}
				}
			}
		}
	}
}
