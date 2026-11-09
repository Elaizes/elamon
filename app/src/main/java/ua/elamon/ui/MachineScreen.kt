package ua.elamon.ui

import android.app.Application
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
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
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import ua.elamon.Conn
import ua.elamon.MachineState
import ua.elamon.MachineViewModel
import ua.elamon.data.PairTarget
import ua.elamon.data.Server
import ua.elamon.data.ServerStore

private enum class Tab(val label: String) {
	Overview("Overview"),
	Processes("Processes"),
	Sensors("Sensors"),
	Displays("Displays"),
	Control("Control"),
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
	LaunchedEffect(Unit) { vm.messages.collect { snackbar.showSnackbar(it) } }
	LaunchedEffect(Unit) { vm.refreshControl() }

	val view = LocalView.current
	DisposableEffect(prefs.keepScreenOn) {
		view.keepScreenOn = prefs.keepScreenOn
		onDispose { view.keepScreenOn = false }
	}

	BackHandler(enabled = detail != null) { detail = null }

	val name = state.info?.name ?: server.name
	val status = connText(state.conn, prefs.intervalMs)
	val currentList = if (detail != null) detailList else lists.getValue(tab)
	val collapsed = rememberCollapsed(currentList)

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			Box(Modifier.fillMaxSize().hazeSource(haze)) {
				when {
					state.conn == Conn.Unauthorized -> Centered {
						Notice(Icons.Default.Warning, "This phone isn't paired anymore", "The computer no longer accepts it, maybe after elamond revoke. Run elamond pair on the computer and pair again.") {
							Button(onClick = { onPairAgain(server.toTarget()) }) { Text("Pair again") }
						}
					}
					state.conn == Conn.CertificateChanged -> Centered {
						Notice(Icons.Default.Warning, "The computer's certificate changed", "This happens when the service's data was reset or reinstalled, but also when something else pretends to be the computer. If you didn't reset it, check your network before pairing again.") {
							Button(onClick = { onPairAgain(server.toTarget().copy(fingerprint = null)) }) { Text("Pair again") }
						}
					}
					state.sample == null && state.conn == Conn.Offline -> Centered {
						Notice(ElIcons.Laptop, "Can't reach $name", "The computer may be off or asleep, the service not running, or the phone on another network. Elamon keeps trying.") {
							Button(onClick = { vm.retry() }) { Text("Try again") }
							TextButton(onClick = { onHelp(HelpTopic.TROUBLESHOOTING) }) { Text("Troubleshooting") }
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
								val s = TabScope(state, vm, lists.getValue(t), { MachineHeader(name, state, status, lists.getValue(t)) }, { detail = it }, onHelp)
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
					IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, "More") }
					DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
						DropdownMenuItem(text = { Text("Settings") }, onClick = { menu = false; onSettings() })
						DropdownMenuItem(text = { Text("Help") }, onClick = { menu = false; onHelp(null) })
						DropdownMenuItem(text = { Text("Forget computer") }, onClick = { menu = false; confirmForget = true })
					}
				},
			)

			// Old numbers stay visible while the connection comes back, marked as such.
			AnimatedVisibility(
				visible = state.sample != null && state.conn != Conn.Live,
				modifier = Modifier.align(Alignment.TopCenter).padding(top = 110.dp),
			) {
				Pill(if (state.conn == Conn.Offline) "Offline · retrying" else "Reconnecting…", Metric.colors.warn, dot = true)
			}

			if (state.sample != null) BottomGlass(Modifier.align(Alignment.BottomCenter), extra = if (detail == null) 92.dp else 0.dp)
			if (state.sample != null && detail == null) {
				FloatingNavBar(
					items = Tab.entries.map { NavItem(it.label, it.icon()) },
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
			title = "Forget ${server.name}?",
			text = { Text("This phone will stop showing it. To see it again, pair again. To also remove the phone on the computer, run elamond revoke there.") },
			confirm = {
				TextButton(onClick = {
					ServerStore.get(ctx).remove(server.id)
					confirmForget = false
					onForgotten()
				}) { Text("Forget") }
			},
			dismiss = { TextButton(onClick = { confirmForget = false }) { Text("Cancel") } },
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
					Pill("Up ${Fmt.duration(s.uptime)}", MaterialTheme.colorScheme.secondary)
					when (s.power.onAc) {
						true -> Pill("On AC power", Metric.colors.good, dot = true)
						false -> Pill("On battery", Metric.colors.warn, dot = true)
						null -> {}
					}
					(s.power.ppdProfile ?: s.power.profile)?.let { p -> Pill(profileName(p), MaterialTheme.colorScheme.tertiary) }
				}
			}
		},
	)
}

fun profileName(p: String): String = when (p) {
	"power-saver", "low-power" -> "Power saver"
	"cool" -> "Cool"
	"quiet" -> "Quiet"
	"balanced" -> "Balanced"
	"balanced-performance" -> "Balanced performance"
	"performance" -> "Performance"
	else -> p.replaceFirstChar { it.uppercase() }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
	Box(Modifier.fillMaxSize().padding(top = 80.dp), contentAlignment = Alignment.Center) { content() }
}

private fun connText(conn: Conn, interval: Int): String = when (conn) {
	Conn.Live -> "Live · every ${interval / 1000} s"
	Conn.Connecting -> "Connecting…"
	Conn.Reconnecting -> "Reconnecting…"
	Conn.Offline -> "Offline"
	Conn.Unauthorized -> "Not paired"
	Conn.CertificateChanged -> "Certificate changed"
}

fun Server.toTarget() = PairTarget(hosts = hostsToTry(), port = port, name = name, id = id, fingerprint = fingerprint)
