package ua.elamon.ui

import android.app.Application
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import ua.elamon.Conn
import ua.elamon.MachineViewModel
import ua.elamon.R
import ua.elamon.Settings
import ua.elamon.data.PairTarget
import ua.elamon.data.Server
import ua.elamon.data.ServerStore

private enum class Tab(val label: Int, val icon: ImageVector) {
	Overview(R.string.tab_overview, ElIcons.Overview),
	Processes(R.string.tab_processes, ElIcons.Processes),
	Sensors(R.string.tab_sensors, ElIcons.Thermometer),
	Displays(R.string.tab_displays, ElIcons.Monitor),
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MachineScreen(server: Server, onBack: () -> Unit, onPairAgain: (PairTarget) -> Unit, onForgotten: () -> Unit) {
	val ctx = LocalContext.current
	val vm: MachineViewModel = viewModel(
		key = "machine-" + server.id,
		factory = MachineViewModel.Factory(ctx.applicationContext as Application, server.id),
	)
	val state by vm.state.collectAsStateWithLifecycle()
	val settings = Settings.get(ctx)
	val interval by settings.intervalFlow.collectAsStateWithLifecycle()
	val keepOn by settings.keepScreenOn.collectAsStateWithLifecycle()
	var tab by rememberSaveable { mutableStateOf(Tab.Overview) }
	var menu by rememberSaveable { mutableStateOf(false) }
	var confirmForget by rememberSaveable { mutableStateOf(false) }

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

	val view = LocalView.current
	DisposableEffect(keepOn) {
		view.keepScreenOn = keepOn
		onDispose { view.keepScreenOn = false }
	}

	Scaffold(
		topBar = {
			TopAppBar(
				title = {
					Column {
						Text(state.info?.name ?: server.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
						Text(
							connText(state.conn, interval),
							style = MaterialTheme.typography.labelMedium,
							color = if (state.conn == Conn.Live) Metric.colors.good else MaterialTheme.colorScheme.onSurfaceVariant,
						)
					}
				},
				navigationIcon = {
					IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
				},
				actions = {
					IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, stringResource(R.string.more)) }
					DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
						Text(
							stringResource(R.string.update_every),
							style = MaterialTheme.typography.labelMedium,
							color = MaterialTheme.colorScheme.onSurfaceVariant,
							modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
						)
						for (ms in Settings.INTERVALS) {
							DropdownMenuItem(
								text = { Text(stringResource(R.string.seconds, ms / 1000)) },
								onClick = {
									settings.setInterval(ms)
									menu = false
								},
								trailingIcon = { if (ms == interval) Icon(Icons.Default.Check, null) },
							)
						}
						HorizontalDivider()
						DropdownMenuItem(
							text = { Text(stringResource(R.string.keep_screen_on)) },
							onClick = { settings.setKeepScreenOn(!keepOn) },
							trailingIcon = { if (keepOn) Icon(Icons.Default.Check, null) },
						)
						DropdownMenuItem(
							text = { Text(stringResource(R.string.forget_computer)) },
							onClick = {
								menu = false
								confirmForget = true
							},
						)
					}
				},
				colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
			)
		},
		bottomBar = {
			NavigationBar {
				for (t in Tab.entries) {
					NavigationBarItem(
						selected = tab == t,
						onClick = { tab = t },
						icon = { Icon(t.icon, null) },
						label = { Text(stringResource(t.label)) },
					)
				}
			}
		},
	) { padding ->
		Box(Modifier.fillMaxSize()) {
			when {
				state.conn == Conn.Unauthorized -> Centered(padding) {
					Notice(Icons.Default.Warning, stringResource(R.string.unpaired_title), stringResource(R.string.unpaired_body)) {
						Button(onClick = { onPairAgain(server.toTarget()) }) { Text(stringResource(R.string.pair_again)) }
					}
				}
				state.conn == Conn.CertificateChanged -> Centered(padding) {
					Notice(Icons.Default.Warning, stringResource(R.string.cert_changed_title), stringResource(R.string.cert_changed_body)) {
						Button(onClick = { onPairAgain(server.toTarget().copy(fingerprint = null)) }) { Text(stringResource(R.string.pair_again)) }
					}
				}
				state.sample == null && state.conn == Conn.Offline -> Centered(padding) {
					Notice(ElIcons.Laptop, stringResource(R.string.offline_title, server.name), stringResource(R.string.offline_body)) {
						Button(onClick = { vm.retry() }) { Text(stringResource(R.string.retry)) }
					}
				}
				state.sample == null -> Centered(padding) { CircularProgressIndicator() }
				else -> when (tab) {
					Tab.Overview -> OverviewTab(state, padding)
					Tab.Processes -> ProcessesTab(state, padding, vm::setProcesses)
					Tab.Sensors -> SensorsTab(state, padding)
					Tab.Displays -> DisplaysTab(state, padding)
				}
			}
			// Old numbers stay visible while the connection comes back, marked as such.
			AnimatedVisibility(
				visible = state.sample != null && state.conn != Conn.Live,
				modifier = Modifier.padding(top = padding.calculateTopPadding()).align(Alignment.TopCenter),
			) {
				Text(
					stringResource(R.string.reconnecting),
					style = MaterialTheme.typography.labelMedium,
					color = MaterialTheme.colorScheme.onErrorContainer,
					modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.errorContainer).padding(6.dp),
					textAlign = androidx.compose.ui.text.style.TextAlign.Center,
				)
			}
		}
	}

	if (confirmForget) {
		AlertDialog(
			onDismissRequest = { confirmForget = false },
			title = { Text(stringResource(R.string.forget_title, server.name)) },
			text = { Text(stringResource(R.string.forget_body)) },
			confirmButton = {
				TextButton(onClick = {
					ServerStore.get(ctx).remove(server.id)
					confirmForget = false
					onForgotten()
				}) { Text(stringResource(R.string.forget)) }
			},
			dismissButton = { TextButton(onClick = { confirmForget = false }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

@Composable
private fun Centered(padding: PaddingValues, content: @Composable () -> Unit) {
	Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) { content() }
}

@Composable
private fun connText(conn: Conn, interval: Int): String = when (conn) {
	Conn.Live -> stringResource(R.string.conn_live, interval / 1000)
	Conn.Connecting -> stringResource(R.string.conn_connecting)
	Conn.Reconnecting -> stringResource(R.string.reconnecting)
	Conn.Offline -> stringResource(R.string.conn_offline)
	Conn.Unauthorized -> stringResource(R.string.conn_unpaired)
	Conn.CertificateChanged -> stringResource(R.string.conn_cert_changed)
}

fun Server.toTarget() = PairTarget(hosts = hostsToTry(), port = port, name = name, id = id, fingerprint = fingerprint)
