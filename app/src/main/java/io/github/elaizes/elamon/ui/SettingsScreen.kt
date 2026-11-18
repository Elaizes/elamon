package io.github.elaizes.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
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
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import kotlin.math.roundToInt
import android.os.Build
import io.github.elaizes.elamon.BuildConfig
import io.github.elaizes.elamon.OwnTheme
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Scale
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.ThemeMode
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.Profile
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.withoutControls

/**
 * Which profile each computer gives this phone, and where it is changed (how the app looks and
 * behaves comes from there). What stays the app's own: its scale and, if it is on, its own theme
 * (Display), the same from home and from any computer; help, source and licences.
 */
@Composable
fun SettingsScreen(machine: String?, onBack: () -> Unit, onHelp: () -> Unit) {
	val ctx = LocalContext.current
	val p = LocalPrefs.current
	val haze = remember { HazeState() }
	val list = rememberLazyListState()
	var license by rememberSaveable { mutableStateOf(false) }
	val settings = remember { Settings.get(ctx) }
	val look by settings.look.collectAsStateWithLifecycle()
	val machines by MachineStore.get(ctx).machines.collectAsStateWithLifecycle()
	val profiles by UiCache.get(ctx).profiles.collectAsStateWithLifecycle()
	// The computer Settings was opened from, or all of them from the home screen.
	val shown = machines.filter { machine == null || it.id == machine }.ifEmpty { machines.take(1) }

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			LazyColumn(
				Modifier.fillMaxSize().hazeSource(haze),
				state = list,
				contentPadding = listPadding(),
				verticalArrangement = Arrangement.spacedBy(Space.s),
			) {
				item { BigTitle(stringResource(R.string.settings), list = list) }
				// Which profile each computer gives this phone, and where it is changed.
				if (shown.isEmpty()) {
					item {
						Text(
							stringResource(R.string.settings_from_defaults),
							style = MaterialTheme.typography.bodyMedium,
							color = MaterialTheme.colorScheme.onSurfaceVariant,
							modifier = Modifier.padding(horizontal = Space.xl, vertical = Space.xs),
						)
					}
				} else {
					item {
						ListGroup {
							shown.forEachIndexed { i, m ->
								if (i > 0) GroupDivider()
								val profile = profiles[m.id]
								if (profile == null) {
									ListRow(stringResource(R.string.settings_from_unknown, m.name))
								} else {
									ListRow(stringResource(R.string.settings_from_profile, profile.name.ifEmpty { profile.id }, m.name), profilePlaceText(profilePlace(profile)))
								}
							}
						}
					}
				}

				// With the phone's own theme on, the profile gives the rest but not the theme.
				if (look.ownOn) {
					item {
						Text(
							stringResource(R.string.settings_theme_from_phone),
							style = MaterialTheme.typography.bodySmall,
							color = MaterialTheme.colorScheme.onSurfaceVariant,
							modifier = Modifier.padding(horizontal = Space.xl),
						)
					}
				}

				// The app's own: the same from home and from any computer.
				item { GroupLabel(stringResource(R.string.settings_display)) }
				item {
					// Off, what turning it on would give: as last set, or the theme in force now.
					val own = look.own ?: OwnTheme.of(p)
					ListGroup {
						ScaleRow(look.scale, settings::setScale)
						GroupDivider()
						SwitchRow(
							stringResource(R.string.settings_own_theme),
							stringResource(if (look.ownOn) R.string.settings_own_theme_on else R.string.settings_own_theme_off),
							checked = look.ownOn,
						) { settings.setOwnTheme(it, p) }
						GroupDivider()
						ThemeModeRow(own.theme, enabled = look.ownOn) { settings.setOwn(own.copy(theme = it)) }
						GroupDivider()
						SwitchRow(
							stringResource(R.string.settings_pure_black),
							stringResource(R.string.settings_pure_black_summary),
							checked = own.pureBlack,
							enabled = look.ownOn,
						) { settings.setOwn(own.copy(pureBlack = it)) }
						// Colours from the wallpaper are Android 12's.
						if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
							GroupDivider()
							SwitchRow(
								stringResource(R.string.settings_dynamic_color),
								stringResource(R.string.settings_dynamic_color_summary),
								checked = own.dynamicColor,
								enabled = look.ownOn,
							) { settings.setOwn(own.copy(dynamicColor = it)) }
						}
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

	if (license) {
		val text = remember { runCatching { ctx.assets.open("licenses/inter-OFL.txt").bufferedReader().readText() }.getOrDefault("") }
		GlassDialog(
			onDismiss = { license = false },
			title = stringResource(R.string.settings_licences),
			text = {
				Column(Modifier.heightIn(max = Sizes.licenceMax).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
					Text(stringResource(R.string.settings_licence_elamon))
					Text(stringResource(R.string.settings_licence_icons))
					Text(stringResource(R.string.settings_licence_inter), style = MaterialTheme.typography.titleSmall)
					Text(text, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
				}
			},
			confirm = { TextButton(onClick = { license = false }) { Text(stringResource(R.string.settings_close)) } },
		)
	}
}

/**
 * The app's scale ([Scale]): a slider in steps, its value beside the title, and back to 100%. It
 * changes once the finger lifts, so the slider doesn't grow under it.
 */
@Composable
private fun ScaleRow(scale: Int, onChange: (Int) -> Unit) {
	var dragged by remember { mutableStateOf<Float?>(null) }
	val shown = dragged?.let { Scale.of(it.roundToInt()) } ?: scale
	val describe = stringResource(R.string.settings_scale_slider)
	Column(Modifier.fillMaxWidth().padding(start = Space.xl, end = Space.s, top = Space.mPlus)) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			Column(Modifier.weight(1f)) {
				Text(stringResource(R.string.settings_scale), style = MaterialTheme.typography.bodyLarge)
				Text(
					stringResource(R.string.settings_scale_summary, Fmt.percent(shown.toDouble())),
					style = MaterialTheme.typography.bodySmall,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
				)
			}
			TextButton(onClick = { onChange(Scale.DEFAULT) }, enabled = scale != Scale.DEFAULT) {
				Text(stringResource(R.string.settings_scale_reset, Fmt.percent(Scale.DEFAULT.toDouble())))
			}
		}
		Slider(
			value = dragged ?: scale.toFloat(),
			onValueChange = { dragged = it },
			onValueChangeFinished = {
				dragged?.let { onChange(Scale.of(it.roundToInt())) }
				dragged = null
			},
			valueRange = Scale.MIN.toFloat()..Scale.MAX.toFloat(),
			steps = Scale.STOPS,
			modifier = Modifier.padding(end = Space.m).semantics { contentDescription = describe },
		)
	}
}

/** The phone's own theme's mode: the phone's day and night, light, or dark; a chip each. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ThemeModeRow(mode: ThemeMode, enabled: Boolean, onChange: (ThemeMode) -> Unit) {
	val alpha = if (enabled) 1f else Alpha.DISABLED
	Column(Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.s)) {
		Text(stringResource(R.string.settings_theme), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha))
		FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
			for (m in ThemeMode.entries) {
				FilterChip(
					selected = m == mode,
					onClick = { if (m != mode) onChange(m) },
					label = { Text(stringResource(themeLabel(m))) },
					enabled = enabled,
					shape = PillShape,
				)
			}
		}
	}
}

private fun themeLabel(mode: ThemeMode): Int = when (mode) {
	ThemeMode.System -> R.string.settings_theme_system
	ThemeMode.Light -> R.string.settings_theme_light
	ThemeMode.Dark -> R.string.settings_theme_dark
}

/** Where a profile is set on the computer, as a person finds it. */
internal sealed interface ProfilePlace {
	/** A file of the settings folder (~/.config/elamon), and the line of the profile's table there. */
	data class InFolder(val file: String, val line: Int) : ProfilePlace

	/** A file elsewhere, by its path. */
	data class AtPath(val file: String, val line: Int) : ProfilePlace

	/** Settings the NixOS or Home Manager module wrote into the Nix store: set in Nix, under [id]. */
	data class Nix(val id: String) : ProfilePlace

	/** Only its table: the computer tells admins alone where it is. */
	data class Table(val id: String) : ProfilePlace
}

/** Where [profile] is set, from what the computer says of it (`source`, for an admin). */
internal fun profilePlace(profile: Profile): ProfilePlace {
	val s = profile.source ?: return ProfilePlace.Table(profile.id)
	// Shown as it is: nothing in it may read as something else.
	val file = s.file.withoutControls()
	return when {
		file.startsWith(NIX_STORE) -> ProfilePlace.Nix(profile.id)
		file.startsWith("/") -> ProfilePlace.AtPath(file, s.line)
		else -> ProfilePlace.InFolder(file, s.line)
	}
}

@Composable
private fun profilePlaceText(place: ProfilePlace): String = when (place) {
	is ProfilePlace.InFolder -> stringResource(R.string.settings_profile_at, place.file, place.line)
	is ProfilePlace.AtPath -> stringResource(R.string.settings_profile_at_path, place.file, place.line)
	is ProfilePlace.Nix -> stringResource(R.string.settings_profile_nix, place.id)
	is ProfilePlace.Table -> stringResource(R.string.settings_profile_table, place.id)
}

/** Where Nix keeps what it builds: settings there are set in Nix, not in a file to edit. */
private const val NIX_STORE = "/nix/store/"
