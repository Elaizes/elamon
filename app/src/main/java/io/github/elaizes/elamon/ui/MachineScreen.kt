package io.github.elaizes.elamon.ui

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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
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
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import io.github.elaizes.elamon.BuildConfig
import io.github.elaizes.elamon.Compat
import io.github.elaizes.elamon.Conn
import io.github.elaizes.elamon.Intervals
import io.github.elaizes.elamon.MIN_ELAMOND
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.Message
import io.github.elaizes.elamon.Permissions
import io.github.elaizes.elamon.PhoneRole
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.compatibility
import io.github.elaizes.elamon.data.Info
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore

private enum class Tab(@StringRes val label: Int) {
	Overview(R.string.tab_overview),
	Processes(R.string.tab_processes),
	Sensors(R.string.tab_sensors),
	Control(R.string.tab_control),
}

private fun Tab.icon() = when (this) {
	Tab.Overview -> ElIcons.Overview
	Tab.Processes -> ElIcons.Processes
	Tab.Sensors -> ElIcons.Thermometer
	Tab.Control -> ElIcons.Control
}

/** What a tab gets from the screen around it. */
class TabScope(
	val state: MachineState,
	val vm: MachineViewModel,
	/** What this phone may do here: whatever else, the tab leaves out. */
	val perms: Permissions,
	val list: LazyListState,
	val header: @Composable () -> Unit,
	val openDetail: (String) -> Unit,
	val openHelp: (String?) -> Unit,
)

@Composable
fun MachineScreen(
	machine: Machine,
	owner: ViewModelStoreOwner,
	onBack: () -> Unit,
	onPairAgain: (PairTarget) -> Unit,
	onForgotten: () -> Unit,
	onHelp: (String?) -> Unit,
	onSettings: () -> Unit,
) {
	val ctx = LocalContext.current
	// Its own store, cleared when the computer is left for good, forgotten or paired again.
	val vm: MachineViewModel = viewModel(
		viewModelStoreOwner = owner,
		factory = MachineViewModel.Factory(ctx.applicationContext as Application, machine.id),
	)
	val state by vm.state.collectAsStateWithLifecycle()
	val perms by vm.permissions.collectAsStateWithLifecycle()
	val prefs = LocalPrefs.current
	var tab by rememberSaveable { mutableStateOf(Tab.Overview) }
	// A phone that may not see processes has no tab for them; one that loses it goes back to the overview.
	val tabs = Tab.entries.filter { it != Tab.Processes || perms.processes }
	val shownTab = if (tab in tabs) tab else Tab.Overview
	LaunchedEffect(shownTab) { tab = shownTab }
	var detail by rememberSaveable { mutableStateOf<String?>(null) }
	// The activity page closes for a phone no longer allowed to read it.
	LaunchedEffect(detail, perms.log) { if (detail == ACTIVITY_PAGE && !perms.log) detail = null }
	val activity by vm.activity.collectAsStateWithLifecycle()
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

	val name = state.info?.name ?: machine.name
	val status = connText(state.conn, Intervals.effective(prefs.intervalMs, state.info?.intervalMs))
	val compatDismissed by vm.compatDismissed.collectAsStateWithLifecycle()
	val compat = state.info?.let { compatibility(BuildConfig.VERSION_NAME, it.api, it.elamondVersion, it.minApp) } ?: Compat.Ok
	// Unpaired, another certificate or refused: the screen is a notice, with no tabs to switch
	// between and no old numbers coming back.
	val turnedAway = state.conn == Conn.Unauthorized || state.conn == Conn.CertificateChanged || state.conn == Conn.Refused
	val currentList = if (detail != null) detailList else lists.getValue(shownTab)
	val collapsed = rememberCollapsed(currentList)

	CompositionLocalProvider(LocalHaze provides haze) {
		// The tab bar is measured first: the lists, the glass and the snackbar keep clear of what it covers.
		BottomBarLayout(
			Modifier.fillMaxSize(),
			bar = {
				if (state.sample != null && detail == null && !turnedAway) {
					FloatingNavBar(
						items = tabs.map { NavItem(stringResource(it.label), it.icon()) },
						selected = tabs.indexOf(shownTab),
						onSelect = { tab = tabs[it] },
					)
				}
			},
		) {
			Box(Modifier.fillMaxSize()) {
				Box(Modifier.fillMaxSize().hazeSource(haze)) {
					when {
						state.conn == Conn.Unauthorized -> Centered {
							Notice(Icons.Default.Warning, stringResource(R.string.machine_unpaired_title), stringResource(R.string.machine_unpaired_body)) {
								Button(onClick = { onPairAgain(machine.toTarget()) }) { Text(stringResource(R.string.machine_pair_again)) }
							}
						}
						state.conn == Conn.CertificateChanged -> Centered {
							Notice(Icons.Default.Warning, stringResource(R.string.machine_cert_title), stringResource(R.string.machine_cert_body)) {
								// A reset service has a new certificate and a new id: pair with whatever answers, checked by eye, in this entry's place.
								Button(onClick = { onPairAgain(machine.toTarget().copy(fingerprint = null, id = null)) }) { Text(stringResource(R.string.machine_pair_again)) }
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
								if (prefs.animations) (fadeIn(tween(Motion.DETAIL_IN)) + scaleIn(tween(Motion.DETAIL_IN), initialScale = Motion.DETAIL_SCALE)) togetherWith fadeOut(tween(Motion.DETAIL_OUT))
								else fadeIn(tween(0)) togetherWith fadeOut(tween(0))
							},
							label = "detail",
						) { d ->
							if (d != null) {
								if (d == ACTIVITY_PAGE) ActivityPage(vm, state, detailList)
								else DetailPage(d, state, vm, detailList, processes = perms.processes)
							} else {
								AnimatedContent(
									targetState = shownTab,
									transitionSpec = { fadeIn(tween(if (prefs.animations) Motion.TAB_IN else 0)) togetherWith fadeOut(tween(if (prefs.animations) Motion.TAB_OUT else 0)) },
									label = "tab",
								) { t ->
									val header = @Composable {
										Column {
											MachineHeader(name, state, status, lists.getValue(t), perms.role)
											val info = state.info
											if (compat != Compat.Ok && !compatDismissed && info != null) CompatBanner(compat, info) { vm.dismissCompat() }
										}
									}
									val s = TabScope(state, vm, perms, lists.getValue(t), header, { detail = it }, onHelp)
									when (t) {
										Tab.Overview -> OverviewTab(s)
										Tab.Processes -> ProcessesTab(s)
										Tab.Sensors -> SensorsTab(s)
										Tab.Control -> ControlTab(s)
									}
								}
							}
						}
					}
				}

				GlassTopBar(
					title = detail?.let { detailTitle(it, state) } ?: name,
					// No big title on a notice: the bar says whose it is.
					collapsed = collapsed || state.sample == null || turnedAway,
					onBack = { if (detail != null) detail = null else onBack() },
					subtitle = if (detail != null) name else status,
					actions = {
						if (detail == ACTIVITY_PAGE) {
							IconButton(onClick = { vm.refreshActivity() }, enabled = !activity.loading) {
								Icon(Icons.Default.Refresh, stringResource(R.string.activity_refresh))
							}
						}
						IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, stringResource(R.string.machine_more)) }
						DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
							DropdownMenuItem(text = { Text(stringResource(R.string.settings)) }, onClick = { menu = false; onSettings() })
							DropdownMenuItem(text = { Text(stringResource(R.string.help)) }, onClick = { menu = false; onHelp(null) })
							DropdownMenuItem(text = { Text(stringResource(R.string.machine_forget)) }, onClick = { menu = false; confirmForget = true })
						}
					},
				)

				// Old numbers stay visible while the connection comes back, marked as such: in the bar's
				// empty middle while the big title shows, under the bar once it has the small one.
				val barHeight = topBarHeight()
				Box(
					Modifier
						.align(Alignment.TopCenter)
						.statusBarsPadding()
						.padding(horizontal = BarButtonsClearance)
						.then(if (collapsed) Modifier.padding(top = barHeight + Space.s) else Modifier.height(barHeight)),
					contentAlignment = Alignment.Center,
				) {
					AnimatedVisibility(visible = state.sample != null && state.conn != Conn.Live && !turnedAway) {
						Pill(stringResource(if (state.conn == Conn.Offline) R.string.machine_offline_retrying else R.string.conn_reconnecting), Metric.colors.warn, dot = true)
					}
				}

				if (state.sample != null && !turnedAway) BottomGlass(Modifier.align(Alignment.BottomCenter))

				SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).padding(bottom = bottomReserve() + Space.l))
			}
		}
	}

	if (confirmForget) {
		GlassDialog(
			onDismiss = { confirmForget = false },
			title = stringResource(R.string.machine_forget_title, machine.name),
			text = { Text(stringResource(R.string.machine_forget_body)) },
			confirm = {
				TextButton(onClick = {
					MachineStore.get(ctx).remove(machine.id)
					confirmForget = false
					onForgotten()
				}) { Text(stringResource(R.string.machine_forget_confirm)) }
			},
			dismiss = { TextButton(onClick = { confirmForget = false }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

/** The big title: the computer's name, how it's doing, what it runs, and what this phone may do there. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MachineHeader(name: String, state: MachineState, status: String, list: LazyListState, role: String?) {
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
				FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s), verticalArrangement = Arrangement.spacedBy(Space.s)) {
					state.info?.os?.takeIf { o -> o.isNotBlank() }?.let { os -> Pill(os, MaterialTheme.colorScheme.secondary) }
					Pill(stringResource(R.string.machine_uptime, Fmt.duration(resources(), s.uptime)), MaterialTheme.colorScheme.secondary)
					when (s.power.onAc) {
						true -> Pill(stringResource(R.string.machine_on_ac), Metric.colors.good, dot = true)
						false -> Pill(stringResource(R.string.machine_on_battery), Metric.colors.warn, dot = true)
						null -> {}
					}
					(s.power.ppdProfile ?: s.power.profile)?.let { p -> Pill(profileName(p), MaterialTheme.colorScheme.tertiary) }
					role?.let { RolePill(it) }
				}
			}
		},
	)
}

/** This phone's role on the computer, small among the other pills; a screen reader also hears what it allows. */
@Composable
private fun RolePill(role: String) {
	val name = roleName(role)
	val known = PhoneRole.of(role)
	val description = if (known != null) stringResource(R.string.a11y_role, name, stringResource(known.about())) else stringResource(R.string.a11y_role_unknown, name)
	Pill(name, MaterialTheme.colorScheme.primary, Modifier.clearAndSetSemantics { contentDescription = description })
}

/** A role by the name `elamond clients role` takes, which a newer service's may be. */
@Composable
private fun roleName(role: String): String = when (PhoneRole.of(role)) {
	PhoneRole.Glance -> stringResource(R.string.role_glance)
	PhoneRole.Monitor -> stringResource(R.string.role_monitor)
	PhoneRole.Operate -> stringResource(R.string.role_operate)
	PhoneRole.Admin -> stringResource(R.string.role_admin)
	null -> role.replaceFirstChar { it.uppercase() }
}

@StringRes
private fun PhoneRole.about(): Int = when (this) {
	PhoneRole.Glance -> R.string.role_glance_about
	PhoneRole.Monitor -> R.string.role_monitor_about
	PhoneRole.Operate -> R.string.role_operate_about
	PhoneRole.Admin -> R.string.role_admin_about
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

/** In the middle of what the bars leave: under the top bar, above whatever covers the bottom. */
@Composable
private fun Centered(content: @Composable () -> Unit) {
	Box(
		Modifier.fillMaxSize().statusBarsPadding().padding(top = topBarHeight(), bottom = bottomReserve()),
		contentAlignment = Alignment.Center,
	) { content() }
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
		shape = RoundedCornerShape(Radius.lPlus),
		color = warn.copy(alpha = Alpha.BANNER),
		modifier = Modifier.fillMaxWidth().padding(bottom = Space.m),
	) {
		Row(Modifier.padding(start = Space.mPlus, top = Space.xs, bottom = Space.xs), verticalAlignment = Alignment.CenterVertically) {
			Icon(Icons.Default.Warning, null, tint = warn, modifier = Modifier.size(Sizes.iconM))
			Spacer(Modifier.width(Space.sPlus))
			Text(
				if (compat == Compat.UpdateApp) stringResource(R.string.compat_update_app)
				else stringResource(R.string.compat_update_elamond, info.elamondVersion, MIN_ELAMOND),
				style = MaterialTheme.typography.bodySmall,
				modifier = Modifier.weight(1f),
			)
			IconButton(onClick = onDismiss) { Icon(Icons.Default.Close, stringResource(R.string.compat_dismiss), Modifier.size(Sizes.iconM)) }
		}
	}
}

/** Pairing this computer again, in its entry's place. */
fun Machine.toTarget() = PairTarget(hosts = hostsToTry(), port = port, name = name, id = id, fingerprint = fingerprint, replaces = id)
