package io.github.elaizes.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.KeyboardActionHandler
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.TextObfuscationMode
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.maxLength
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconToggleButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedSecureTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Transfer
import io.github.elaizes.elamon.TransferViewModel
import io.github.elaizes.elamon.data.PairingsFile

/** The longest passphrase taken: far beyond any typed, short of one pasted by mistake. */
private const val PASSPHRASE_MAX = 1024

/**
 * Asks for a passphrase, [twice] alike and at least [PairingsFile.PASSPHRASE_MIN] characters long
 * for a new file. The fields use the password keyboard, hide what is typed unless asked to show it,
 * and are never saved: they live in plain `remember`, are emptied as the passphrase is handed on,
 * and go with the dialog.
 */
@Composable
fun PassphraseDialog(
	title: String,
	text: String,
	twice: Boolean,
	confirmLabel: String,
	onDismiss: () -> Unit,
	onPassphrase: (CharArray) -> Unit,
	error: String? = null,
) {
	val first = remember { TextFieldState() }
	val again = remember { TextFieldState() }
	var shown by remember { mutableStateOf(false) }
	DisposableEffect(Unit) {
		onDispose {
			first.clearText()
			again.clearText()
		}
	}
	val tooShort = twice && first.text.length < PairingsFile.PASSPHRASE_MIN
	val differ = twice && !first.text.contentEquals(again.text)
	val ready = first.text.isNotEmpty() && !tooShort && !differ
	val submit = {
		if (ready) {
			val typed = first.text
			val chars = CharArray(typed.length) { typed[it] }
			first.clearText()
			again.clearText()
			onPassphrase(chars)
		}
	}
	val mode = if (shown) TextObfuscationMode.Visible else TextObfuscationMode.RevealLastTyped
	val keyboard = { last: Boolean -> KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = if (last) ImeAction.Done else ImeAction.Next) }
	val onDone = KeyboardActionHandler { submit() }
	GlassDialog(
		onDismiss = onDismiss,
		title = title,
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
				Text(text, style = MaterialTheme.typography.bodyMedium)
				OutlinedSecureTextField(
					state = first,
					label = { Text(stringResource(R.string.transfer_passphrase)) },
					textObfuscationMode = mode,
					inputTransformation = InputTransformation.maxLength(PASSPHRASE_MAX),
					trailingIcon = {
						IconToggleButton(checked = shown, onCheckedChange = { shown = it }) {
							Icon(if (shown) ElIcons.Conceal else ElIcons.Reveal, stringResource(R.string.transfer_show))
						}
					},
					isError = error != null || (tooShort && again.text.isNotEmpty()),
					supportingText = when {
						error != null -> {
							{ Text(error, Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
						}
						twice -> {
							{ Text(pluralStringResource(R.plurals.transfer_too_short, PairingsFile.PASSPHRASE_MIN, PairingsFile.PASSPHRASE_MIN)) }
						}
						else -> null
					},
					keyboardOptions = keyboard(!twice),
					onKeyboardAction = if (twice) null else onDone,
					modifier = Modifier.fillMaxWidth(),
				)
				if (twice) {
					val mismatch = differ && again.text.isNotEmpty()
					OutlinedSecureTextField(
						state = again,
						label = { Text(stringResource(R.string.transfer_passphrase_again)) },
						textObfuscationMode = mode,
						inputTransformation = InputTransformation.maxLength(PASSPHRASE_MAX),
						isError = mismatch,
						supportingText = if (mismatch) {
							{ Text(stringResource(R.string.transfer_differ)) }
						} else {
							null
						},
						keyboardOptions = keyboard(true),
						onKeyboardAction = onDone,
						modifier = Modifier.fillMaxWidth(),
					)
				}
			}
		},
		confirm = { TextButton(onClick = submit, enabled = ready) { Text(confirmLabel) } },
		dismiss = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
	)
}

/** What exporting or importing pairings is doing or came to, over Settings. */
@Composable
fun TransferDialogs(transfer: TransferViewModel) {
	val state by transfer.state.collectAsStateWithLifecycle()
	when (val s = state) {
		Transfer.Idle -> {}
		is Transfer.ExportNeedsPassphrase -> PassphraseDialog(
			title = stringResource(R.string.settings_export),
			text = stringResource(R.string.transfer_export_again),
			twice = true,
			confirmLabel = stringResource(R.string.transfer_export_button),
			onDismiss = { transfer.cancelExport(s.uri) },
			onPassphrase = { transfer.export(s.uri, it) },
		)
		is Transfer.ImportNeedsPassphrase -> PassphraseDialog(
			title = stringResource(R.string.settings_import),
			text = stringResource(R.string.transfer_import_text),
			twice = false,
			confirmLabel = stringResource(R.string.transfer_import_button),
			onDismiss = transfer::dismiss,
			onPassphrase = transfer::importWith,
			error = if (s.wrong) stringResource(R.string.transfer_wrong) else null,
		)
		is Transfer.Working -> Working(stringResource(if (s.export) R.string.transfer_encrypting else R.string.transfer_decrypting))
		is Transfer.Exported -> Outcome(
			stringResource(R.string.transfer_exported_title),
			listOfNotNull(
				pluralStringResource(R.plurals.transfer_exported, s.count, s.count),
				if (s.leftOut > 0) pluralStringResource(R.plurals.transfer_left_out, s.leftOut, s.leftOut) else null,
			),
			transfer::dismiss,
		)
		is Transfer.Imported -> Outcome(
			stringResource(R.string.transfer_imported_title),
			listOf(stringResource(R.string.transfer_imported, s.added, s.replaced), stringResource(R.string.transfer_imported_note)),
			transfer::dismiss,
		)
		is Transfer.Failed -> Outcome(
			stringResource(if (s.export) R.string.settings_export else R.string.settings_import),
			listOf(stringResource(s.message)),
			transfer::dismiss,
		)
	}
}

/** While the key is made from the passphrase: a second or two, not to be cut short. */
@Composable
private fun Working(title: String) {
	GlassDialog(
		onDismiss = {},
		title = title,
		text = {
			Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
		},
		confirm = {},
	)
}

@Composable
private fun Outcome(title: String, lines: List<String>, onDismiss: () -> Unit) {
	GlassDialog(
		onDismiss = onDismiss,
		title = title,
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
				for (line in lines) Text(line, style = MaterialTheme.typography.bodyMedium)
			}
		},
		confirm = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.settings_close)) } },
	)
}
