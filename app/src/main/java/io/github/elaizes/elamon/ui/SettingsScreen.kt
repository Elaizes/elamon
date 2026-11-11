package io.github.elaizes.elamon.ui

import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Info
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import io.github.elaizes.elamon.BuildConfig
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.Intervals
import io.github.elaizes.elamon.Prefs
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.TransferViewModel
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.ThemeMode
import io.github.elaizes.elamon.UnlockGrace
import io.github.elaizes.elamon.asksLess

@Composable
fun SettingsScreen(onBack: () -> Unit, onHelp: () -> Unit) {
	val ctx = LocalContext.current
	val settings = Settings.get(ctx)
	val p = LocalPrefs.current
	val dark = LocalDark.current
	val haze = remember { HazeState() }
	val list = rememberLazyListState()
	var license by remember { mutableStateOf(false) }
	var customInterval by remember { mutableStateOf(false) }
	val machines by MachineStore.get(ctx).machines.collectAsStateWithLifecycle()
	val transfer: TransferViewModel = viewModel()
	var askExport by remember { mutableStateOf(false) }
	val guard = LocalGuard.current
	val res = resources()
	/**
	 * Asking less (a kind no longer asked for, a longer grace) outlasts any grace, so it asks to
	 * unlock every time, as exporting does; asking more is free.
	 */
	fun change(prompt: String, question: String, button: String, edit: (Prefs) -> Prefs) {
		if (!asksLess(settings.current, edit(settings.current))) return settings.update(edit)
		guard.run(Ask(Guarded.Transfer, prompt, question, button, icon = ElIcons.Lock)) { settings.update(edit) }
	}
	// Here, not in the list's rows: a row scrolled away would take the picker's answer with it.
	val saveFile = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(PAIRINGS_MIME)) { transfer.exportTo(it) }
	val openFile = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { transfer.importFrom(it) }
	val elamondInterval by settings.elamondIntervalMs.collectAsStateWithLifecycle()
	// Checked again on coming back, as from setting a screen lock.
	var lockScreen by remember { mutableStateOf(hasScreenLock(ctx)) }
	LifecycleResumeEffect(Unit) {
		lockScreen = hasScreenLock(ctx)
		onPauseOrDispose {}
	}

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			LazyColumn(
				Modifier.fillMaxSize().hazeSource(haze),
				state = list,
				contentPadding = listPadding(),
				verticalArrangement = Arrangement.spacedBy(Space.s),
			) {
				item { BigTitle(stringResource(R.string.settings), list = list) }

				item { GroupLabel(stringResource(R.string.settings_appearance)) }
				item {
					ListGroup {
						Choice(
							stringResource(R.string.settings_theme),
							listOf(
								ThemeMode.System to stringResource(R.string.settings_theme_system),
								ThemeMode.Light to stringResource(R.string.settings_theme_light),
								ThemeMode.Dark to stringResource(R.string.settings_theme_dark),
							),
							p.theme,
						) { v -> settings.update { it.copy(theme = v) } }
						GroupDivider()
						SwitchRow(stringResource(R.string.settings_pure_black), stringResource(R.string.settings_pure_black_summary), p.pureBlack, enabled = dark) { v -> settings.update { it.copy(pureBlack = v) } }
						GroupDivider()
						SwitchRow(
							stringResource(R.string.settings_wallpaper_colours),
							stringResource(if (Build.VERSION.SDK_INT >= 31) R.string.settings_wallpaper_colours_summary else R.string.settings_needs_android_12),
							p.dynamicColor && Build.VERSION.SDK_INT >= 31,
							enabled = Build.VERSION.SDK_INT >= 31,
						) { v -> settings.update { it.copy(dynamicColor = v) } }
						GroupDivider()
						SwitchRow(
							stringResource(R.string.settings_blur),
							stringResource(if (p.blurAvailable) R.string.settings_blur_summary else R.string.settings_needs_android_12),
							p.useBlur,
							enabled = p.blurAvailable,
						) { v -> settings.update { it.copy(blur = v) } }
						GroupDivider()
						SwitchRow(stringResource(R.string.settings_animations), stringResource(R.string.settings_animations_summary), p.animations) { v -> settings.update { it.copy(animations = v) } }
					}
				}

				item { GroupLabel(stringResource(R.string.settings_data)) }
				item {
					ListGroup {
						IntervalChoice(p.intervalMs, onCustom = { customInterval = true }) { v -> settings.update { it.copy(intervalMs = v) } }
						GroupDivider()
						Choice(stringResource(R.string.settings_temperature), listOf(false to Fmt.CELSIUS, true to Fmt.FAHRENHEIT), p.fahrenheit) { v -> settings.update { it.copy(fahrenheit = v) } }
						GroupDivider()
						Choice(stringResource(R.string.settings_network_speed), listOf(false to stringResource(R.string.settings_bytes), true to stringResource(R.string.settings_bits)), p.networkBits) { v -> settings.update { it.copy(networkBits = v) } }
						GroupDivider()
						SwitchRow(stringResource(R.string.settings_keep_screen_on), stringResource(R.string.settings_keep_screen_on_summary), p.keepScreenOn) { v -> settings.update { it.copy(keepScreenOn = v) } }
					}
				}

				item { GroupLabel(stringResource(R.string.settings_control)) }
				item {
					ListGroup {
						SwitchRow(stringResource(R.string.settings_confirm_actions), stringResource(R.string.settings_confirm_actions_summary), p.confirmActions) { v -> settings.update { it.copy(confirmActions = v) } }
						GroupDivider()
						SwitchRow(stringResource(R.string.settings_haptics), stringResource(R.string.settings_haptics_summary), p.haptics) { v -> settings.update { it.copy(haptics = v) } }
					}
				}

				item { GroupLabel(stringResource(R.string.settings_unlock)) }
				if (!lockScreen) {
					item {
						Text(
							stringResource(R.string.settings_unlock_no_lock),
							style = MaterialTheme.typography.bodySmall,
							color = MaterialTheme.colorScheme.onSurfaceVariant,
							modifier = Modifier.padding(horizontal = Space.xl, vertical = Space.xs),
						)
					}
				}
				item {
					ListGroup {
						Guarded.entries.forEachIndexed { i, kind ->
							if (i > 0) GroupDivider()
							// A fixed kind shows as on, and can't be switched off.
							SwitchRow(stringResource(kind.title()), stringResource(kind.summary()), kind.fixed || kind in p.unlockFor, enabled = !kind.fixed) { v ->
								val title = res.getString(kind.title())
								change(
									res.getString(R.string.unlock_stop_asking, title),
									res.getString(R.string.settings_stop_asking_title, title),
									res.getString(R.string.settings_stop_asking),
								) { it.copy(unlockFor = if (v) it.unlockFor + kind else it.unlockFor - kind) }
							}
						}
						GroupDivider()
						val graceLabel = { g: UnlockGrace -> if (g == UnlockGrace.EveryTime) res.getString(R.string.settings_unlock_every_time) else res.getString(R.string.duration_minutes, g.minutes) }
						Choice(
							stringResource(R.string.settings_unlock_grace),
							UnlockGrace.entries.map { it to graceLabel(it) },
							p.unlockGrace,
						) { v ->
							change(
								res.getString(R.string.unlock_grace, graceLabel(v)),
								res.getString(R.string.settings_grace_title, graceLabel(v)),
								res.getString(R.string.settings_grace_change),
							) { it.copy(unlockGrace = v) }
						}
					}
				}

				item { GroupLabel(stringResource(R.string.settings_pairings)) }
				item {
					ListGroup {
						ListRow(
							stringResource(R.string.settings_export),
							stringResource(if (machines.isEmpty()) R.string.settings_export_none else R.string.settings_export_summary),
							enabled = machines.isNotEmpty(),
							onClick = {
								// Every computer's key leaves the phone: unlocking it first, every time.
								val ask = Ask(Guarded.Transfer, res.getString(R.string.settings_export), res.getString(R.string.transfer_export_question), res.getString(R.string.control_continue))
								guard.run(ask) { askExport = true }
							},
						)
						GroupDivider()
						ListRow(stringResource(R.string.settings_import), stringResource(R.string.settings_import_summary), onClick = {
							// Computers from a file may take the place of those here: as for exporting.
							val ask = Ask(Guarded.Transfer, res.getString(R.string.settings_import), res.getString(R.string.transfer_import_question), res.getString(R.string.control_continue))
							guard.run(ask) {
								// Any type: a file copied over may have lost its own on the way.
								runCatching { openFile.launch(arrayOf("*/*")) }.onFailure { transfer.failed(R.string.transfer_no_picker, export = false) }
							}
						})
					}
				}

				item { GroupLabel(stringResource(R.string.settings_about)) }
				item {
					ListGroup {
						ListRow(stringResource(R.string.help), stringResource(R.string.settings_help_summary), icon = ElIcons.Help, onClick = onHelp)
						GroupDivider()
						ListRow(stringResource(R.string.settings_source), Project.URL.removePrefix("https://"), icon = ElIcons.Globe, onClick = {
							openWebLink(ctx, Project.URL)
						})
						GroupDivider()
						ListRow(stringResource(R.string.settings_licences), stringResource(R.string.settings_licences_summary), icon = Icons.Default.Info, onClick = { license = true })
						GroupDivider()
						ListRow(stringResource(R.string.settings_version), BuildConfig.VERSION_NAME)
					}
				}
			}
			BottomGlass(Modifier.align(androidx.compose.ui.Alignment.BottomCenter))
			GlassTopBar(title = stringResource(R.string.settings), collapsed = rememberCollapsed(list), onBack = onBack)
		}
	}

	if (customInterval) {
		CustomIntervalDialog(
			current = p.intervalMs,
			minMs = (elamondInterval ?: Intervals.MIN_MS).coerceAtMost(Intervals.MAX_MS),
			onDismiss = { customInterval = false },
			onSave = { v ->
				customInterval = false
				settings.update { it.copy(intervalMs = v) }
			},
		)
	}

	if (askExport) {
		PassphraseDialog(
			title = stringResource(R.string.settings_export),
			text = stringResource(R.string.transfer_export_text),
			twice = true,
			confirmLabel = stringResource(R.string.transfer_choose_file),
			onDismiss = { askExport = false },
			onPassphrase = { passphrase ->
				askExport = false
				transfer.exportPassphrase(passphrase)
				runCatching { saveFile.launch(TransferViewModel.fileName()) }.onFailure { transfer.failed(R.string.transfer_no_picker, export = true) }
			},
		)
	}
	TransferDialogs(transfer)

	if (license) {
		val text = remember { runCatching { ctx.assets.open("licenses/inter-OFL.txt").bufferedReader().readText() }.getOrDefault("") }
		GlassDialog(
			onDismiss = { license = false },
			title = stringResource(R.string.settings_licences),
			text = {
				Column(Modifier.heightIn(max = Sizes.licenceMax).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
					Text(stringResource(R.string.settings_licence_elamon))
					Text(stringResource(R.string.settings_licence_inter), style = MaterialTheme.typography.titleSmall)
					Text(text, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
				}
			},
			confirm = { TextButton(onClick = { license = false }) { Text(stringResource(R.string.settings_close)) } },
		)
	}
}

/** The usual intervals side by side, and a last one for any other value, which shows it once set. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun IntervalChoice(value: Int, onCustom: () -> Unit, onPick: (Int) -> Unit) {
	val custom = value !in Intervals.CHOICES
	val count = Intervals.CHOICES.size + 1
	Column(Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.m), verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
		Text(stringResource(R.string.settings_interval), style = MaterialTheme.typography.bodyLarge)
		SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
			Intervals.CHOICES.forEachIndexed { i, ms ->
				SegmentedButton(selected = ms == value, onClick = { onPick(ms) }, shape = SegmentedButtonDefaults.itemShape(i, count)) {
					Text(stringResource(R.string.settings_interval_seconds, Fmt.seconds(ms)), maxLines = 1)
				}
			}
			SegmentedButton(selected = custom, onClick = onCustom, shape = SegmentedButtonDefaults.itemShape(count - 1, count)) {
				Text(if (custom) stringResource(R.string.settings_interval_seconds, Fmt.seconds(value)) else stringResource(R.string.settings_interval_custom), maxLines = 1)
			}
		}
	}
}

/** Any interval in seconds, down to the service's own tick when it's known. */
@Composable
private fun CustomIntervalDialog(current: Int, minMs: Int, onDismiss: () -> Unit, onSave: (Int) -> Unit) {
	var text by remember { mutableStateOf(Fmt.seconds(current)) }
	val parsed = Intervals.parse(text, minMs)
	GlassDialog(
		onDismiss = onDismiss,
		title = stringResource(R.string.settings_interval),
		text = {
			OutlinedTextField(
				value = text,
				onValueChange = { text = it.take(Limits.INTERVAL_TEXT_MAX) },
				label = { Text(stringResource(R.string.settings_interval_custom_label)) },
				supportingText = { Text(stringResource(R.string.settings_interval_custom_range, Fmt.seconds(minMs), Fmt.seconds(Intervals.MAX_MS))) },
				isError = parsed == null,
				singleLine = true,
				keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Done),
				keyboardActions = KeyboardActions(onDone = { parsed?.let(onSave) }),
				modifier = Modifier.fillMaxWidth(),
			)
		},
		confirm = { TextButton(onClick = { parsed?.let(onSave) }, enabled = parsed != null) { Text(stringResource(R.string.settings_save)) } },
		dismiss = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
	)
}

/** A setting with a few values side by side. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun <T> Choice(title: String, options: List<Pair<T, String>>, value: T, onPick: (T) -> Unit) {
	Column(Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.m), verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
		Text(title, style = MaterialTheme.typography.bodyLarge)
		SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
			options.forEachIndexed { i, (v, label) ->
				SegmentedButton(selected = v == value, onClick = { onPick(v) }, shape = SegmentedButtonDefaults.itemShape(i, options.size)) { Text(label) }
			}
		}
	}
}

@StringRes
private fun Guarded.title(): Int = when (this) {
	Guarded.Power -> R.string.settings_unlock_power
	Guarded.Processes -> R.string.settings_unlock_processes
	Guarded.Actions -> R.string.settings_unlock_actions
	Guarded.Units -> R.string.settings_unlock_units
	Guarded.Profile -> R.string.settings_unlock_profile
	Guarded.Media -> R.string.settings_unlock_media
	Guarded.Transfer -> R.string.settings_unlock_transfer
}

@StringRes
private fun Guarded.summary(): Int = when (this) {
	Guarded.Transfer -> R.string.settings_unlock_transfer_summary
	Guarded.Power -> R.string.settings_unlock_power_summary
	Guarded.Processes -> R.string.settings_unlock_processes_summary
	Guarded.Actions -> R.string.settings_unlock_actions_summary
	Guarded.Units -> R.string.settings_unlock_units_summary
	Guarded.Profile -> R.string.settings_unlock_profile_summary
	Guarded.Media -> R.string.settings_unlock_media_summary
}

/** What exported pairings are saved as: no type of their own, so the name's ending stays as given. */
private const val PAIRINGS_MIME = "application/octet-stream"
