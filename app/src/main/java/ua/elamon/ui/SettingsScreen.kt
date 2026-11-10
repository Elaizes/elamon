package ua.elamon.ui

import android.content.Intent
import android.net.Uri
import android.os.Build
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import ua.elamon.BuildConfig
import ua.elamon.Intervals
import ua.elamon.R
import ua.elamon.Settings
import ua.elamon.ThemeMode

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
	val serviceInterval by settings.serviceIntervalMs.collectAsStateWithLifecycle()

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			LazyColumn(
				Modifier.fillMaxSize().hazeSource(haze),
				state = list,
				contentPadding = listPadding(bottomNav = false),
				verticalArrangement = Arrangement.spacedBy(8.dp),
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

				item { GroupLabel(stringResource(R.string.settings_about)) }
				item {
					ListGroup {
						ListRow(stringResource(R.string.help), stringResource(R.string.settings_help_summary), icon = ElIcons.Help, onClick = onHelp)
						GroupDivider()
						ListRow(stringResource(R.string.settings_source), Project.URL.removePrefix("https://"), icon = ElIcons.Globe, onClick = {
							runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(Project.URL))) }
						})
						GroupDivider()
						ListRow(stringResource(R.string.settings_licences), stringResource(R.string.settings_licences_summary), icon = Icons.Default.Info, onClick = { license = true })
						GroupDivider()
						ListRow(stringResource(R.string.settings_version), BuildConfig.VERSION_NAME)
					}
				}
			}
			BottomGlass(Modifier.align(androidx.compose.ui.Alignment.BottomCenter), extra = 0.dp)
			GlassTopBar(title = stringResource(R.string.settings), collapsed = rememberCollapsed(list), onBack = onBack)
		}
	}

	if (customInterval) {
		CustomIntervalDialog(
			current = p.intervalMs,
			minMs = (serviceInterval ?: Intervals.MIN_MS).coerceAtMost(Intervals.MAX_MS),
			onDismiss = { customInterval = false },
			onSave = { v ->
				customInterval = false
				settings.update { it.copy(intervalMs = v) }
			},
		)
	}

	if (license) {
		val text = remember { runCatching { ctx.assets.open("licenses/inter-OFL.txt").bufferedReader().readText() }.getOrDefault("") }
		GlassDialog(
			onDismiss = { license = false },
			title = stringResource(R.string.settings_licences),
			text = {
				Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
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
	Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
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
				onValueChange = { text = it.take(8) },
				label = { Text(stringResource(R.string.settings_interval_custom_label)) },
				supportingText = { Text(stringResource(R.string.settings_interval_custom_range, Fmt.seconds(minMs), Fmt.seconds(Intervals.MAX_MS))) },
				isError = parsed == null,
				singleLine = true,
				keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
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
	Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
		Text(title, style = MaterialTheme.typography.bodyLarge)
		SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
			options.forEachIndexed { i, (v, label) ->
				SegmentedButton(selected = v == value, onClick = { onPick(v) }, shape = SegmentedButtonDefaults.itemShape(i, options.size)) { Text(label) }
			}
		}
	}
}
