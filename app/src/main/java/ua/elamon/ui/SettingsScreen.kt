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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Info
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import ua.elamon.BuildConfig
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

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			LazyColumn(
				Modifier.fillMaxSize().hazeSource(haze),
				state = list,
				contentPadding = listPadding(bottomNav = false),
				verticalArrangement = Arrangement.spacedBy(8.dp),
			) {
				item { BigTitle("Settings", list = list) }

				item { GroupLabel("Appearance") }
				item {
					ListGroup {
						Choice("Theme", listOf(ThemeMode.System to "System", ThemeMode.Light to "Light", ThemeMode.Dark to "Dark"), p.theme) { v -> settings.update { it.copy(theme = v) } }
						GroupDivider()
						SwitchRow("Pure black", "Black instead of dark grey in dark mode; saves power on OLED screens", p.pureBlack, enabled = dark) { v -> settings.update { it.copy(pureBlack = v) } }
						GroupDivider()
						SwitchRow(
							"Wallpaper colours",
							if (Build.VERSION.SDK_INT >= 31) "Take the app's colours from the wallpaper" else "Needs Android 12",
							p.dynamicColor && Build.VERSION.SDK_INT >= 31,
							enabled = Build.VERSION.SDK_INT >= 31,
						) { v -> settings.update { it.copy(dynamicColor = v) } }
						GroupDivider()
						SwitchRow(
							"Blur",
							if (p.blurAvailable) "Frosted glass behind bars and dialogs" else "Needs Android 12",
							p.useBlur,
							enabled = p.blurAvailable,
						) { v -> settings.update { it.copy(blur = v) } }
						GroupDivider()
						SwitchRow("Animations", "Rolling numbers and moving gauges", p.animations) { v -> settings.update { it.copy(animations = v) } }
					}
				}

				item { GroupLabel("Data") }
				item {
					ListGroup {
						Choice("Update every", Settings.INTERVALS.map { it to "${it / 1000} s" }, p.intervalMs) { v -> settings.update { it.copy(intervalMs = v) } }
						GroupDivider()
						Choice("Temperature", listOf(false to "°C", true to "°F"), p.fahrenheit) { v -> settings.update { it.copy(fahrenheit = v) } }
						GroupDivider()
						Choice("Network speed", listOf(false to "Bytes", true to "Bits"), p.networkBits) { v -> settings.update { it.copy(networkBits = v) } }
						GroupDivider()
						SwitchRow("Keep screen on", "While a computer is open, as a dashboard", p.keepScreenOn) { v -> settings.update { it.copy(keepScreenOn = v) } }
					}
				}

				item { GroupLabel("Control") }
				item {
					ListGroup {
						SwitchRow("Confirm actions", "Ask before power, kill, restart and commands", p.confirmActions) { v -> settings.update { it.copy(confirmActions = v) } }
						GroupDivider()
						SwitchRow("Haptic feedback", "A tick on long presses", p.haptics) { v -> settings.update { it.copy(haptics = v) } }
					}
				}

				item { GroupLabel("About") }
				item {
					ListGroup {
						ListRow("Help", "Installing the service, the firewall, pairing, troubleshooting", icon = ElIcons.Help, onClick = onHelp)
						GroupDivider()
						ListRow("Source code", Project.URL.removePrefix("https://"), icon = ElIcons.Globe, onClick = {
							runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(Project.URL))) }
						})
						GroupDivider()
						ListRow("Licences", "Elamon: GPL-3.0-or-later · Inter: SIL Open Font License", icon = Icons.Default.Info, onClick = { license = true })
						GroupDivider()
						ListRow("Version", BuildConfig.VERSION_NAME)
					}
				}
			}
			BottomGlass(Modifier.align(androidx.compose.ui.Alignment.BottomCenter), extra = 0.dp)
			GlassTopBar(title = "Settings", collapsed = rememberCollapsed(list), onBack = onBack)
		}
	}

	if (license) {
		val text = remember { runCatching { ctx.assets.open("licenses/inter-OFL.txt").bufferedReader().readText() }.getOrDefault("") }
		GlassDialog(
			onDismiss = { license = false },
			title = "Licences",
			text = {
				Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
					Text("Elamon is free software under the GNU General Public License, version 3 or later.")
					Text("The Inter typeface:", style = MaterialTheme.typography.titleSmall)
					Text(text, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
				}
			},
			confirm = { TextButton(onClick = { license = false }) { Text("Close") } },
		)
	}
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
