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
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
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
import io.github.elaizes.elamon.MIN_ELAMOND
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.Message
import io.github.elaizes.elamon.PhoneRole
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.allWidgets
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState

/**
 * A computer's screen: its tabs and the pages their widgets open, as the computer describes them
 * (`ui`), filled in from one stream that is told what is on screen.
 */
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
	val want by vm.dashboard.collectAsStateWithLifecycle()
	val actionsState by vm.actions.collectAsStateWithLifecycle()
	val prefs = LocalPrefs.current
	val ui = state.ui
	// The last tab shown here, as kept for this computer, until another is chosen.
	var tab by rememberSaveable { mutableStateOf(vm.dashboard.value.dashboard) }
	var pages by rememberSaveable { mutableStateOf(listOf<String>()) }
	val tabs = ui?.tabs.orEmpty().filter { ui?.dashboard(it) != null }
	val shownTab = tab?.takeIf { it in tabs } ?: tabs.firstOrNull()
	// A page whose dashboard is gone from the ui (the role changed) closes.
	val shownPages = pages.filter { ui?.dashboard(it) != null }
	val current = shownPages.lastOrNull() ?: shownTab
	LaunchedEffect(current) { current?.let(vm::show) }
	var menu by remember { mutableStateOf(false) }
	var confirmForget by remember { mutableStateOf(false) }
	val haze = remember { HazeState() }
	val snackbar = remember { SnackbarHostState() }
	val lists = remember { mutableMapOf<String, LazyListState>() }
	val folds = remember(machine.id) { Folds(Settings.get(ctx), machine.id) }
	val tables = remember { TableStates() }
	val form = remember { mutableStateOf<FormRequest?>(null) }

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
	// Why unlocking the phone for an action failed, when it wasn't just cancelled.
	val guard = LocalGuard.current
	LaunchedEffect(guard) {
		guard.messages.collect { snackbar.showSnackbar(it) }
	}

	val view = LocalView.current
	DisposableEffect(prefs.keepScreenOn) {
		view.keepScreenOn = prefs.keepScreenOn
		onDispose { view.keepScreenOn = false }
	}

	BackHandler(enabled = shownPages.isNotEmpty()) { pages = shownPages.dropLast(1) }

	val name = ui?.service?.name?.ifEmpty { null } ?: machine.name
	val status = connText(state.conn, want.intervalMs)
	// Unpaired, another certificate, refused or not understood: the screen is a notice, with no
	// tabs to switch between and no old numbers coming back.
	val turnedAway = state.conn in setOf(Conn.Unauthorized, Conn.CertificateChanged, Conn.Refused, Conn.Incompatible)
	val currentList = lists.getOrPut(current ?: "") { LazyListState() }
	val collapsed = rememberCollapsed(currentList)
	val haptics = LocalHapticFeedback.current
	val launcher = ui?.let {
		ActionLauncher(
			guard, vm, it, name, res,
			tap = { if (prefs.haptics) haptics.performHapticFeedback(HapticFeedbackType.ContextClick) },
			form = form,
			running = actionsState,
		)
	}

	CompositionLocalProvider(LocalHaze provides haze) {
		// The tab bar is measured first: the lists, the glass and the snackbar keep clear of what it covers.
		BottomBarLayout(
			Modifier.fillMaxSize(),
			bar = {
				if (ui != null && shownPages.isEmpty() && !turnedAway && tabs.size > 1) {
					FloatingNavBar(
						items = tabs.map { id -> ui.dashboard(id).let { NavItem(it?.title ?: id, icon(it?.icon)) } },
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
							Notice(Icons.Default.Warning, stringResource(R.string.machine_unpaired_title), stringResource(if (state.keyLost) R.string.machine_key_lost_body else R.string.machine_unpaired_body)) {
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
						state.conn == Conn.Incompatible -> Centered {
							val app = state.compat == Compat.UpdateApp
							Notice(
								Icons.Default.Warning,
								stringResource(if (app) R.string.compat_app_title else R.string.compat_service_title, MIN_ELAMOND),
								stringResource(if (app) R.string.compat_app_body else R.string.compat_service_body, state.hello?.version.orEmpty(), BuildConfig.VERSION_NAME),
							) {
								Button(onClick = { vm.retry() }) { Text(stringResource(R.string.retry)) }
								TextButton(onClick = { onHelp(null) }) { Text(stringResource(R.string.help)) }
							}
						}
						ui == null && state.conn == Conn.Offline -> Centered {
							Notice(ElIcons.Laptop, stringResource(R.string.machine_unreachable_title, name), stringResource(R.string.machine_unreachable_body)) {
								Button(onClick = { vm.retry() }) { Text(stringResource(R.string.retry)) }
								TextButton(onClick = { onHelp(HelpTopic.TROUBLESHOOTING) }) { Text(stringResource(R.string.troubleshooting_button)) }
							}
						}
						ui == null || current == null || launcher == null -> Centered { CircularProgressIndicator() }
						else -> AnimatedContent(
							targetState = current,
							transitionSpec = {
								val page = initialState in shownPages || targetState in shownPages
								when {
									!prefs.animations -> fadeIn(tween(0)) togetherWith fadeOut(tween(0))
									page -> (fadeIn(tween(Motion.DETAIL_IN)) + scaleIn(tween(Motion.DETAIL_IN), initialScale = Motion.DETAIL_SCALE)) togetherWith fadeOut(tween(Motion.DETAIL_OUT))
									else -> fadeIn(tween(Motion.TAB_IN)) togetherWith fadeOut(tween(Motion.TAB_OUT))
								}
							},
							label = "dashboard",
						) { d ->
							val list = lists.getOrPut(d) { LazyListState() }
							val isPage = d in shownPages
							val scope = DashScope(
								machine.id, ui, d, state.data[d]?.widgets.orEmpty(), vm, state, name,
								open = { target -> if (target != d && ui.dashboard(target) != null) pages = shownPages + target },
								actions = launcher,
								folds = folds,
								tables = tables,
							)
							DashboardList(scope, list) {
								Column {
									MachineHeader(
										if (isPage) ui.dashboard(d)?.title ?: d else name,
										state,
										if (isPage) name else status,
										list,
										headerPills(ui, state),
										ui.role,
										live = !isPage,
									)
									if (state.compat != Compat.Ok) CompatBanner(state.compat, state.hello?.version.orEmpty())
								}
							}
						}
					}
				}

				GlassTopBar(
					title = if (current != null && current in shownPages) ui?.dashboard(current)?.title ?: name else name,
					// No big title on a notice: the bar says whose it is.
					collapsed = collapsed || ui == null || turnedAway,
					onBack = { if (shownPages.isNotEmpty()) pages = shownPages.dropLast(1) else onBack() },
					subtitle = if (current != null && current in shownPages) name else status,
					actions = {
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
					AnimatedVisibility(visible = ui != null && state.conn != Conn.Live && !turnedAway) {
						Pill(stringResource(if (state.conn == Conn.Offline) R.string.machine_offline_retrying else R.string.conn_reconnecting), Metric.colors.warn, dot = true)
					}
				}

				if (ui != null && !turnedAway) BottomGlass(Modifier.align(Alignment.BottomCenter))

				SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).padding(bottom = bottomReserve() + Space.l))
			}
		}
	}

	// The sheets and the form over the screen: a table's row, what a command printed, an action's values.
	if (ui != null && current != null && launcher != null) {
		val scope = DashScope(machine.id, ui, current, state.data[current]?.widgets.orEmpty(), vm, state, name, {}, launcher, folds, tables)
		TableSheet(scope)
	}
	ActionOutput(vm, ui, actionsState)
	form.value?.let { req ->
		ParamForm(req, onDismiss = { form.value = null }) { params -> launcher?.submit(req, params) }
	}

	if (confirmForget) {
		GlassDialog(
			onDismiss = { confirmForget = false },
			title = stringResource(R.string.machine_forget_title, machine.name),
			text = { Text(stringResource(R.string.machine_forget_body)) },
			confirm = {
				TextButton(onClick = {
					MachineStore.get(ctx).remove(machine.id)
					UiCache.get(ctx).remove(machine.id)
					Settings.get(ctx).forget(machine.id)
					confirmForget = false
					onForgotten()
				}) { Text(stringResource(R.string.machine_forget_confirm)) }
			},
			dismiss = { TextButton(onClick = { confirmForget = false }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

/** The pills under the computer's name: the header dashboard's widgets with their states. */
private fun headerPills(ui: UiDoc, state: MachineState): List<Pair<Widget, WidgetState?>> {
	val id = ui.header ?: return emptyList()
	val dash = ui.dashboard(id) ?: return emptyList()
	val states = state.data[id]?.widgets.orEmpty()
	return allWidgets(dash.widgets).filter { it.type != "group" }.map { it to states[it.id] }
}

/** The big title: the computer's name (or a page's), how it's doing, the header's pills, and what this phone may do there. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MachineHeader(
	title: String,
	state: MachineState,
	status: String,
	list: LazyListState,
	pills: List<Pair<Widget, WidgetState?>>,
	role: String?,
	live: Boolean,
) {
	BigTitle(
		title,
		list = list,
		subtitle = {
			Text(
				status,
				style = MaterialTheme.typography.titleSmall,
				color = if (live && state.conn == Conn.Live) Metric.colors.good else MaterialTheme.colorScheme.onSurfaceVariant,
			)
		},
		extra = if (pills.isEmpty() && role == null) null else ({
			FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s), verticalArrangement = Arrangement.spacedBy(Space.s)) {
				for ((w, s) in pills) HeaderPill(w, s)
				role?.let { RolePill(it) }
			}
		}),
	)
}

/** One of the header's widgets as a pill: its text, coloured by its level (with a dot) or its colour. */
@Composable
private fun HeaderPill(w: Widget, s: WidgetState?) {
	val text = s?.text ?: Fmt.format(s?.v, w.format, w.unit) ?: w.text ?: return
	if (text.isBlank()) return
	val level = levelColor(s?.level ?: w.level, s?.blend, Metric.colors.good)
	Pill(text, level ?: if (w.color != null) accent(w.color) else MaterialTheme.colorScheme.secondary, dot = level != null)
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
fun roleName(role: String): String = when (PhoneRole.of(role)) {
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
	Conn.Incompatible -> stringResource(R.string.conn_incompatible)
}

fun Message.text(res: Resources): String = when (this) {
	is Message.Res -> res.getString(id, *args.toTypedArray())
	is Message.Text -> text
}

/** A note under the title when the app or the service is older than the other likes; it works either way. */
@Composable
private fun CompatBanner(compat: Compat, version: String) {
	var dismissed by rememberSaveable { mutableStateOf(false) }
	if (dismissed) return
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
				else stringResource(R.string.compat_update_elamond, version, MIN_ELAMOND),
				style = MaterialTheme.typography.bodySmall,
				modifier = Modifier.weight(1f),
			)
			IconButton(onClick = { dismissed = true }) { Icon(Icons.Default.Close, stringResource(R.string.compat_dismiss), Modifier.size(Sizes.iconM)) }
		}
	}
}

/** Pairing this computer again, in its entry's place. */
fun Machine.toTarget() = PairTarget(hosts = hostsToTry(), port = port, name = name, id = id, fingerprint = fingerprint, replaces = id)
