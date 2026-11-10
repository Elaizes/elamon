package ua.elamon.ui

import android.app.Application
import android.content.res.Resources
import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
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
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import ua.elamon.BuildConfig
import ua.elamon.Compat
import ua.elamon.Conn
import ua.elamon.Intervals
import ua.elamon.MIN_ELAMOND
import ua.elamon.MachineState
import ua.elamon.MachineViewModel
import ua.elamon.Message
import ua.elamon.R
import ua.elamon.compatibility
import ua.elamon.data.Info
import ua.elamon.data.PairTarget
import ua.elamon.data.Server
import ua.elamon.data.ServerStore

private enum class Tab(@StringRes val label: Int) {
	Overview(R.string.tab_overview),
	Processes(R.string.tab_processes),
	Sensors(R.string.tab_sensors),
	Displays(R.string.tab_displays),
	Control(R.string.tab_control),
}

private fun Tab.icon() = when (this) {
	Tab.Overview -> ElIcons.Overview
	Tab.Processes -> ElIcons.Processes
	Tab.Sensors -> ElIcons.Thermometer
	Tab.Displays -> ElIcons.Monitor
	Tab.Control -> ElIcons.Control
}

/** What a tab gets from the screen around it. */
class TabScope(
	val state: MachineState,
	val vm: MachineViewModel,
	val list: LazyListState,
	val header: @Composable () -> Unit,
	val openDetail: (String) -> Unit,
	val openHelp: (String?) -> Unit,
)

@Composable
fun MachineScreen(
	server: Server,
	onBack: () -> Unit,
	onPairAgain: (PairTarget) -> Unit,
	onForgotten: () -> Unit,
	onHelp: (String?) -> Unit,
	onSettings: () -> Unit,
) {
	val ctx = LocalContext.current
	val vm: MachineViewModel = viewModel(
		key = "machine-" + server.id,
		factory = MachineViewModel.Factory(ctx.applicationContext as Application, server.id),
	)
	val state by vm.state.collectAsStateWithLifecycle()
	val prefs = LocalPrefs.current
	var tab by rememberSaveable { mutableStateOf(Tab.Overview) }
	var detail by rememberSaveable { mutableStateOf<String?>(null) }
	var menu by remember { mutableStateOf(false) }
	var confirmForget by remember { mutableStateOf(false) }
	val haze = remember { HazeState() }
	val snackbar = remember { SnackbarHostState() }
	val lists = Tab.entries.associateWith { rememberLazyListState() }
	val detailList = rememberLazyListState()

	// Stream only while the screen can be seen: no data, battery or network use in the background.
	val lifecycle = LocalLifecycleOwner.current.lifecycle
	DisposableEffect(lifecycle) {
		val observer = LifecycleEventObserver { _, e ->
			when (e) {
				Lifecycle.Event.ON_START -> vm.setVisible(true)
				Lifecycle.Event.ON_STOP -> vm.setVisible(false)
				else -> {}
			}
		}
		lifecycle.addObserver(observer)
		if (lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) vm.setVisible(true)
		onDispose {
			lifecycle.removeObserver(observer)
			vm.setVisible(false)
		}
	}
	val res by rememberUpdatedState(resources())
	LaunchedEffect(Unit) {
		vm.messages.collect { m -> snackbar.showSnackbar(m.text(res)) }
	}

	val view = LocalView.current
	DisposableEffect(prefs.keepScreenOn) {
		view.keepScreenOn = prefs.keepScreenOn
		onDispose { view.keepScreenOn = false }
	}

	BackHandler(enabled = detail != null) { detail = null }

	val name = state.info?.name ?: server.name
	val status = connText(state.conn, Intervals.effective(prefs.intervalMs, state.info?.intervalMs))
	val compatDismissed by vm.compatDismissed.collectAsStateWithLifecycle()
	val compat = state.info?.let { compatibility(BuildConfig.VERSION_NAME, it.api, it.serviceVersion, it.minApp) } ?: Compat.Ok
	val currentList = if (detail != null) detailList else lists.getValue(tab)
	val collapsed = rememberCollapsed(currentList)

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			Box(Modifier.fillMaxSize().hazeSource(haze)) {
				when {
					state.conn == Conn.Unauthorized -> Centered {
						Notice(Icons.Default.Warning, stringResource(R.string.machine_unpaired_title), stringResource(R.string.machine_unpaired_body)) {
							Button(onClick = { onPairAgain(server.toTarget()) }) { Text(stringResource(R.string.machine_pair_again)) }
						}
					}
					state.conn == Conn.CertificateChanged -> Centered {
						Notice(Icons.Default.Warning, stringResource(R.string.machine_cert_title), stringResource(R.string.machine_cert_body)) {
							Button(onClick = { onPairAgain(server.toTarget().copy(fingerprint = null)) }) { Text(stringResource(R.string.machine_pair_again)) }
						}
					}
					state.conn == Conn.Refused -> Centered {
						Notice(Icons.Default.Warning, stringResource(R.string.machine_refused_title), state.refused?.text(resources())) {
							Button(onClick = { vm.retry() }) { Text(stringResource(R.string.retry)) }
						}
					}
					state.sample == null && state.conn == Conn.Offline -> Centered {
						Notice(ElIcons.Laptop, stringResource(R.string.machine_unreachable_title, name), stringResource(R.string.machine_unreachable_body)) {
							Button(onClick = { vm.retry() }) { Text(stringResource(R.string.retry)) }
							TextButton(onClick = { onHelp(HelpTopic.TROUBLESHOOTING) }) { Text(stringResource(R.string.troubleshooting_button)) }
						}
					}
					state.sample == null -> Centered { CircularProgressIndicator() }
					else -> AnimatedContent(
						targetState = detail,
						transitionSpec = {
							if (prefs.animations) (fadeIn(tween(220)) + scaleIn(tween(220), initialScale = 0.97f)) togetherWith fadeOut(tween(150))
							else fadeIn(tween(0)) togetherWith fadeOut(tween(0))
						},
						label = "detail",
					) { d ->
						if (d != null) {
							DetailPage(d, state, vm, detailList)
						} else {
							AnimatedContent(
								targetState = tab,
								transitionSpec = { fadeIn(tween(if (prefs.animations) 200 else 0)) togetherWith fadeOut(tween(if (prefs.animations) 120 else 0)) },
								label = "tab",
							) { t ->
								val header = @Composable {
									Column {
										MachineHeader(name, state, status, lists.getValue(t))
										val info = state.info
										if (compat != Compat.Ok && !compatDismissed && info != null) CompatBanner(compat, info) { vm.dismissCompat() }
									}
								}
								val s = TabScope(state, vm, lists.getValue(t), header, { detail = it }, onHelp)
								when (t) {
									Tab.Overview -> OverviewTab(s)
									Tab.Processes -> ProcessesTab(s)
									Tab.Sensors -> SensorsTab(s)
									Tab.Displays -> DisplaysTab(s)
									Tab.Control -> ControlTab(s)
								}
							}
						}
					}
				}
			}

			GlassTopBar(
				title = detail?.let { detailTitle(it, state) } ?: name,
				collapsed = collapsed || state.sample == null,
				onBack = { if (detail != null) detail = null else onBack() },
				subtitle = if (detail != null) name else status,
				actions = {
					IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, stringResource(R.string.machine_more)) }
					DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
						DropdownMenuItem(text = { Text(stringResource(R.string.settings)) }, onClick = { menu = false; onSettings() })
						DropdownMenuItem(text = { Text(stringResource(R.string.help)) }, onClick = { menu = false; onHelp(null) })
						DropdownMenuItem(text = { Text(stringResource(R.string.machine_forget)) }, onClick = { menu = false; confirmForget = true })
					}
				},
			)

			// Old numbers stay visible while the connection comes back, marked as such.
			AnimatedVisibility(
				visible = state.sample != null && state.conn != Conn.Live,
				modifier = Modifier.align(Alignment.TopCenter).padding(top = 110.dp),
			) {
				Pill(stringResource(if (state.conn == Conn.Offline) R.string.machine_offline_retrying else R.string.conn_reconnecting), Metric.colors.warn, dot = true)
			}

			if (state.sample != null) BottomGlass(Modifier.align(Alignment.BottomCenter), extra = if (detail == null) 92.dp else 0.dp)
			if (state.sample != null && detail == null) {
				FloatingNavBar(
					items = Tab.entries.map { NavItem(stringResource(it.label), it.icon()) },
					selected = tab.ordinal,
					onSelect = { tab = Tab.entries[it] },
					modifier = Modifier.align(Alignment.BottomCenter),
				)
			}

			SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = if (detail == null) 96.dp else 16.dp))
		}
	}

	if (confirmForget) {
		GlassDialog(
			onDismiss = { confirmForget = false },
			title = stringResource(R.string.machine_forget_title, server.name),
			text = { Text(stringResource(R.string.machine_forget_body)) },
			confirm = {
				TextButton(onClick = {
					ServerStore.get(ctx).remove(server.id)
					confirmForget = false
					onForgotten()
				}) { Text(stringResource(R.string.machine_forget_confirm)) }
			},
			dismiss = { TextButton(onClick = { confirmForget = false }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

/** The big title: the computer's name, how it's doing, and what it runs. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MachineHeader(name: String, state: MachineState, status: String, list: LazyListState) {
	val s = state.sample
	BigTitle(
		name,
		list = list,
		subtitle = {
			Text(
				status,
				style = MaterialTheme.typography.titleSmall,
				color = if (state.conn == Conn.Live) Metric.colors.good else MaterialTheme.colorScheme.onSurfaceVariant,
			)
		},
		extra = s?.let {
			{
				FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
					state.info?.os?.takeIf { o -> o.isNotBlank() }?.let { os -> Pill(os, MaterialTheme.colorScheme.secondary) }
					Pill(stringResource(R.string.machine_uptime, Fmt.duration(resources(), s.uptime)), MaterialTheme.colorScheme.secondary)
					when (s.power.onAc) {
						true -> Pill(stringResource(R.string.machine_on_ac), Metric.colors.good, dot = true)
						false -> Pill(stringResource(R.string.machine_on_battery), Metric.colors.warn, dot = true)
						null -> {}
					}
					(s.power.ppdProfile ?: s.power.profile)?.let { p -> Pill(profileName(p), MaterialTheme.colorScheme.tertiary) }
				}
			}
		},
	)
}

@Composable
fun profileName(p: String): String = when (p) {
	"power-saver", "low-power" -> stringResource(R.string.profile_power_saver)
	"cool" -> stringResource(R.string.profile_cool)
	"quiet" -> stringResource(R.string.profile_quiet)
	"balanced" -> stringResource(R.string.profile_balanced)
	"balanced-performance" -> stringResource(R.string.profile_balanced_performance)
	"performance" -> stringResource(R.string.profile_performance)
	else -> p.replaceFirstChar { it.uppercase() }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
	Box(Modifier.fillMaxSize().padding(top = 80.dp), contentAlignment = Alignment.Center) { content() }
}

@Composable
private fun connText(conn: Conn, intervalMs: Int): String = when (conn) {
	Conn.Live -> stringResource(R.string.conn_live, Fmt.seconds(intervalMs))
	Conn.Connecting -> stringResource(R.string.conn_connecting)
	Conn.Reconnecting -> stringResource(R.string.conn_reconnecting)
	Conn.Offline -> stringResource(R.string.offline)
	Conn.Unauthorized -> stringResource(R.string.conn_unauthorized)
	Conn.CertificateChanged -> stringResource(R.string.conn_certificate_changed)
	Conn.Refused -> stringResource(R.string.conn_refused)
}

fun Message.text(res: Resources): String = when (this) {
	is Message.Res -> res.getString(id, *args.toTypedArray())
	is Message.Text -> text
}

/** A note under the title when the app or the service is out of date; it works either way. */
@Composable
private fun CompatBanner(compat: Compat, info: Info, onDismiss: () -> Unit) {
	val warn = Metric.colors.warn
	Surface(
		shape = RoundedCornerShape(18.dp),
		color = warn.copy(alpha = 0.14f),
		modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
	) {
		Row(Modifier.padding(start = 14.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
			Icon(Icons.Default.Warning, null, tint = warn, modifier = Modifier.size(18.dp))
			Spacer(Modifier.width(10.dp))
			Text(
				if (compat == Compat.UpdateApp) stringResource(R.string.compat_update_app)
				else stringResource(R.string.compat_update_elamond, info.serviceVersion, MIN_ELAMOND),
				style = MaterialTheme.typography.bodySmall,
				modifier = Modifier.weight(1f),
			)
			IconButton(onClick = onDismiss) { Icon(Icons.Default.Close, stringResource(R.string.compat_dismiss), Modifier.size(18.dp)) }
		}
	}
}

fun Server.toTarget() = PairTarget(hosts = hostsToTry(), port = port, name = name, id = id, fingerprint = fingerprint)
