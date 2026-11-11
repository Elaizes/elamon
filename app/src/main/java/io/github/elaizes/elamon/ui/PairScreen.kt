package io.github.elaizes.elamon.ui

import android.app.Application
import android.os.SystemClock
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
import androidx.compose.material3.FilledTonalButton
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.elaizes.elamon.Arrival
import io.github.elaizes.elamon.PairState
import io.github.elaizes.elamon.PairStep
import io.github.elaizes.elamon.PairViewModel
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.tapCounts

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairScreen(target: PairTarget, owner: ViewModelStoreOwner, onBack: () -> Unit, onPaired: (Machine) -> Unit, onHelp: () -> Unit) {
	val ctx = LocalContext.current
	val vm: PairViewModel = viewModel(
		viewModelStoreOwner = owner,
		factory = PairViewModel.Factory(ctx.applicationContext as Application, target),
	)
	val state by vm.state.collectAsStateWithLifecycle()
	var code by rememberSaveable { mutableStateOf(target.code.orEmpty()) }

	val paired by rememberUpdatedState(onPaired)
	state.paired?.let { machine -> LaunchedEffect(machine) { paired(machine) } }

	// A tap that comes too soon after what it lands on appeared, or after the app came to the front
	// (a link opened by another app over what was tapped there), is ignored.
	val shownAt = remember(state.step, state.replacing) { SystemClock.elapsedRealtime() }
	val tap = { action: () -> Unit -> { if (tapCounts(SystemClock.elapsedRealtime(), maxOf(shownAt, Arrival.at))) action() } }

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
			when (val s = state.step) {
				PairStep.Connecting -> {
					Icon(ElIcons.Laptop, null, Modifier.size(Sizes.pairIcon), tint = MaterialTheme.colorScheme.primary)
					Text(stringResource(R.string.pair_connecting, target.name ?: target.hosts.first()), textAlign = TextAlign.Center)
					CircularProgressIndicator()
				}
				is PairStep.Failed -> {
					Icon(ElIcons.Laptop, null, Modifier.size(Sizes.pairIcon), tint = MaterialTheme.colorScheme.primary)
					Notice(Icons.Default.Warning, stringResource(R.string.pair_failed), stringResource(s.message)) {
						Button(onClick = { vm.connect() }) { Text(stringResource(R.string.retry)) }
						TextButton(onClick = onHelp) { Text(stringResource(R.string.troubleshooting_button)) }
					}
				}
				is PairStep.Ready -> {
					val replacing = state.replacing
					if (replacing != null) {
						ReplaceWarning(replacing, s.fingerprint, onReplace = tap { vm.pair(code, replaceConfirmed = true) }, onCancel = vm::keepExisting)
					} else {
						Icon(ElIcons.Laptop, null, Modifier.size(Sizes.pairIcon), tint = MaterialTheme.colorScheme.primary)
						Ready(target, s, state, code, onCode = { code = it }, tap = tap) { vm.pair(code) }
					}
				}
			}
		}
	}
}

@Composable
private fun Ready(target: PairTarget, s: PairStep.Ready, state: PairState, code: String, onCode: (String) -> Unit, tap: (() -> Unit) -> () -> Unit, onPair: () -> Unit) {
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
	// A link carries the certificate and the code; anything else is paired by code, and the
	// person first checks that the certificate is their computer's.
	val fromLink = target.fingerprint != null && target.code != null
	val byCode = target.fingerprint == null
	// Asked again for another certificate (the computer found anew after a retry).
	var digitsMatch by rememberSaveable(s.fingerprint) { mutableStateOf(false) }
	val codeOpen = !byCode || digitsMatch
	if (!fromLink) {
		if (byCode) {
			Text(stringResource(R.string.pair_compare), textAlign = TextAlign.Center, style = MaterialTheme.typography.bodyMedium)
			Fingerprint(s.fingerprint)
			if (!digitsMatch) {
				FilledTonalButton(onClick = tap { digitsMatch = true }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.pair_digits_match)) }
			}
		}
		OutlinedTextField(
			value = code,
			onValueChange = { v -> onCode(v.filter { it in '0'..'9' }.take(Limits.PAIRING_DIGITS)) },
			label = { Text(stringResource(R.string.pair_code)) },
			enabled = codeOpen,
			singleLine = true,
			isError = state.codeError != null,
			supportingText = { Text(stringResource(state.codeError ?: if (codeOpen) R.string.pair_code_hint else R.string.pair_code_after_digits)) },
			keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword, imeAction = ImeAction.Done),
			keyboardActions = KeyboardActions(onDone = { if (code.length == Limits.PAIRING_DIGITS) tap(onPair)() }),
			textStyle = MaterialTheme.typography.headlineSmall.copy(textAlign = TextAlign.Center, fontFamily = FontFamily.Monospace),
			modifier = Modifier.fillMaxWidth(),
		)
	} else {
		// Any web page can open such a link, not only the QR code elamond pair prints.
		Text(stringResource(R.string.pair_from_link, s.hello.name), textAlign = TextAlign.Center, style = MaterialTheme.typography.bodyMedium)
		state.codeError?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center) }
	}
	if (state.confirmOnComputer) {
		Column(
			Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(Radius.m)).padding(Space.m).semantics { liveRegion = LiveRegionMode.Polite },
			horizontalAlignment = Alignment.CenterHorizontally,
			verticalArrangement = Arrangement.spacedBy(Space.xs),
		) {
			Text(stringResource(R.string.pair_confirm_title), style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
			Text(stringResource(R.string.pair_confirm_body), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
		}
	}
	val busy = state.busy || state.paired != null
	Button(onClick = tap(onPair), enabled = !busy && codeOpen && code.length == Limits.PAIRING_DIGITS, modifier = Modifier.fillMaxWidth()) {
		if (busy) CircularProgressIndicator(Modifier.size(Sizes.iconM), strokeWidth = Sizes.spinnerStroke)
		else Text(stringResource(R.string.pair))
	}
}

/** The first [Limits.FINGERPRINT_SHOWN] hex digits of a certificate, grouped as `elamond pair` prints them. */
@Composable
private fun Fingerprint(fp: String) {
	Text(
		ElamondClient.prettyFingerprint(fp),
		fontFamily = FontFamily.Monospace,
		style = MaterialTheme.typography.titleMedium,
		textAlign = TextAlign.Center,
		modifier = Modifier
			.fillMaxWidth()
			.background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(Radius.m))
			.padding(Space.m),
	)
}

/**
 * The computer is already here under the same id with another certificate: pairing would forget
 * that pairing. Both certificates are shown, and pairing goes on only on a second word.
 */
@Composable
private fun ReplaceWarning(existing: Machine, fingerprint: String, onReplace: () -> Unit, onCancel: () -> Unit) {
	IconBadge(Icons.Default.Warning, MaterialTheme.colorScheme.error, Sizes.badgeNotice)
	Text(stringResource(R.string.pair_replace_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center)
	Text(stringResource(R.string.pair_replace_body, existing.name), style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
	Text(stringResource(R.string.pair_replace_old), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
	Fingerprint(existing.fingerprint)
	Text(stringResource(R.string.pair_replace_new), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
	Fingerprint(fingerprint)
	Button(onClick = onReplace, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.pair_replace_button)) }
	TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
}
