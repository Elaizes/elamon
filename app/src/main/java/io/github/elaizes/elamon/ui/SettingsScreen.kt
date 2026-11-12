package io.github.elaizes.elamon.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Info
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import io.github.elaizes.elamon.BuildConfig
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.ThemeMode
import io.github.elaizes.elamon.TransferViewModel
import io.github.elaizes.elamon.UnlockGrace
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache

/**
 * What applies here, and where it is set: how the app looks and behaves comes from the profile
 * the computer gives this phone (the one [machine], or the first paired), each row with the key of
 * elamond's settings it comes from. What stays the app's own: the pairings, help and licences.
 */
@Composable
fun SettingsScreen(machine: String?, onBack: () -> Unit, onHelp: () -> Unit) {
	val ctx = LocalContext.current
	val p = LocalPrefs.current
	val haze = remember { HazeState() }
	val list = rememberLazyListState()
	var license by remember { mutableStateOf(false) }
	val machines by MachineStore.get(ctx).machines.collectAsStateWithLifecycle()
	val profiles by UiCache.get(ctx).profiles.collectAsStateWithLifecycle()
	val computer = machines.find { it.id == machine } ?: machines.firstOrNull()
	val profile = computer?.let { profiles[it.id] }
	val transfer: TransferViewModel = viewModel()
	var askExport by remember { mutableStateOf(false) }
	val guard = LocalGuard.current
	val res = resources()
	// Here, not in the list's rows: a row scrolled away would take the picker's answer with it.
	val saveFile = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(PAIRINGS_MIME)) { transfer.exportTo(it) }
	val openFile = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { transfer.importFrom(it) }
	val key = { k: String -> "profiles.${profile?.id ?: DEFAULT_PROFILE}.$k" }

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			LazyColumn(
				Modifier.fillMaxSize().hazeSource(haze),
				state = list,
				contentPadding = listPadding(),
				verticalArrangement = Arrangement.spacedBy(Space.s),
			) {
				item {
					BigTitle(
						stringResource(R.string.settings),
						list = list,
						subtitle = {
							Text(
								when {
									computer == null -> stringResource(R.string.settings_from_defaults)
									profile == null -> stringResource(R.string.settings_from_unknown, computer.name)
									else -> stringResource(R.string.settings_from_profile, profile.name.ifEmpty { profile.id }, computer.name)
								},
								style = MaterialTheme.typography.titleSmall,
								color = MaterialTheme.colorScheme.onSurfaceVariant,
							)
						},
					)
				}
				item {
					Text(
						stringResource(R.string.settings_where),
						style = MaterialTheme.typography.bodyMedium,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
						modifier = Modifier.padding(horizontal = Space.xl, vertical = Space.xs),
					)
				}
				item { CodeBlock("elamond clients profile ${computer?.clientId ?: "ID"} ${profile?.id ?: DEFAULT_PROFILE}") }

				item { GroupLabel(stringResource(R.string.settings_appearance)) }
				item {
					ListGroup {
						Ref(R.string.settings_theme, themeName(p.theme), key("theme"))
						GroupDivider()
						Ref(R.string.settings_pure_black, onOff(p.pureBlack), key("pure_black"))
						GroupDivider()
						Ref(R.string.settings_wallpaper_colours, onOff(p.dynamicColor), key("android.dynamic_color"), note = if (android.os.Build.VERSION.SDK_INT < 31) stringResource(R.string.settings_needs_android_12) else null)
						GroupDivider()
						Ref(R.string.settings_blur, onOff(p.blur), key("android.blur"), note = if (!p.blurAvailable) stringResource(R.string.settings_needs_android_12) else null)
						GroupDivider()
						Ref(R.string.settings_animations, onOff(p.animations), key("animations"))
					}
				}

				item { GroupLabel(stringResource(R.string.settings_data)) }
				item {
					ListGroup {
						Ref(R.string.settings_interval, stringResource(R.string.settings_interval_seconds, Fmt.seconds(p.intervalMs)), key("interval_ms"))
						GroupDivider()
						Ref(R.string.settings_temperature, if (p.fahrenheit) Fmt.FAHRENHEIT else Fmt.CELSIUS, key("fahrenheit"))
						GroupDivider()
						Ref(R.string.settings_network_speed, stringResource(if (p.networkBits) R.string.settings_bits else R.string.settings_bytes), key("network_bits"))
						GroupDivider()
						Ref(R.string.settings_keep_screen_on, onOff(p.keepScreenOn), key("android.keep_screen_on"))
					}
				}

				item { GroupLabel(stringResource(R.string.settings_control)) }
				item {
					ListGroup {
						Ref(R.string.settings_confirm_actions, onOff(p.confirmActions), key("confirm_actions"))
						GroupDivider()
						Ref(R.string.settings_haptics, onOff(p.haptics), key("android.haptics"))
						GroupDivider()
						Ref(
							R.string.settings_unlock,
							Guarded.entries.filter { !it.fixed && it in p.unlockFor }.joinToString(", ") { res.getString(it.title()) }.ifEmpty { res.getString(R.string.settings_unlock_none) },
							key("unlock.for"),
						)
						GroupDivider()
						Ref(R.string.settings_unlock_grace, graceName(p.unlockGrace), key("unlock.grace"))
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
								val ask = Ask(Guarded.Transfer, res.getString(R.string.settings_export), res.getString(R.string.transfer_export_question), res.getString(R.string.control_continue), unlock = true)
								guard.run(ask) { askExport = true }
							},
						)
						GroupDivider()
						ListRow(stringResource(R.string.settings_import), stringResource(R.string.settings_import_summary), onClick = {
							// Computers from a file may take the place of those here: as for exporting.
							val ask = Ask(Guarded.Transfer, res.getString(R.string.settings_import), res.getString(R.string.transfer_import_question), res.getString(R.string.control_continue), unlock = true)
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
			BottomGlass(Modifier.align(Alignment.BottomCenter))
			GlassTopBar(title = stringResource(R.string.settings), collapsed = rememberCollapsed(list), onBack = onBack)
		}
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

/** A setting as it applies, and the key of elamond's settings it comes from. */
@Composable
private fun Ref(@StringRes title: Int, value: String, key: String, note: String? = null) {
	Column(Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.m), verticalArrangement = Arrangement.spacedBy(Space.xxs)) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			Text(stringResource(title), style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
			Spacer(Modifier.width(Space.m))
			Text(value, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.primary, textAlign = TextAlign.End)
		}
		Text(key, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace, color = MaterialTheme.colorScheme.onSurfaceVariant)
		note?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
	}
}

@Composable
private fun onOff(on: Boolean): String = stringResource(if (on) R.string.settings_on else R.string.settings_off)

@Composable
private fun themeName(t: ThemeMode): String = stringResource(
	when (t) {
		ThemeMode.System -> R.string.settings_theme_system
		ThemeMode.Light -> R.string.settings_theme_light
		ThemeMode.Dark -> R.string.settings_theme_dark
	}
)

@Composable
private fun graceName(g: UnlockGrace): String =
	if (g == UnlockGrace.EveryTime) stringResource(R.string.settings_unlock_every_time) else stringResource(R.string.settings_unlock_minutes, g.minutes)

@StringRes
internal fun Guarded.title(): Int = when (this) {
	Guarded.Power -> R.string.settings_unlock_power
	Guarded.Processes -> R.string.settings_unlock_processes
	Guarded.Actions -> R.string.settings_unlock_actions
	Guarded.Units -> R.string.settings_unlock_units
	Guarded.Profile -> R.string.settings_unlock_profile
	Guarded.Media -> R.string.settings_unlock_media
	Guarded.Transfer -> R.string.settings_unlock_transfer
}

/** The profile a phone has when the computer names none. */
private const val DEFAULT_PROFILE = "default"

/** What exported pairings are saved as: no type of their own, so the name's ending stays as given. */
private const val PAIRINGS_MIME = "application/octet-stream"

