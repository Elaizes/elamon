package io.github.elaizes.elamon.ui

import android.app.Application
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import io.github.elaizes.elamon.ActionsState
import io.github.elaizes.elamon.Hist
import io.github.elaizes.elamon.Message
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Reach
import io.github.elaizes.elamon.afterRun
import io.github.elaizes.elamon.allWidgets
import io.github.elaizes.elamon.messageFor
import io.github.elaizes.elamon.pollEach
import io.github.elaizes.elamon.reach
import io.github.elaizes.elamon.readDashboard
import io.github.elaizes.elamon.runAction
import io.github.elaizes.elamon.uiOf
import io.github.elaizes.elamon.data.Action
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.DEFAULT_PORT
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.Found
import io.github.elaizes.elamon.data.HistoryEntry
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.discoverElamond
import io.github.elaizes.elamon.data.parseAddress
import io.github.elaizes.elamon.data.tabsShown
import io.github.elaizes.elamon.data.unpairedFound

/** What the home screen knows about a paired computer. */
internal sealed interface Live {
	data object Unknown : Live
	data object Offline : Live
	/** The computer turned this phone away, or the phone can't read its own token any more. */
	data object Unpaired : Live
	/** Its service speaks another API: it (or the app) needs updating. */
	data object Outdated : Live
	/**
	 * It answers; [stats] are its card's figures, none where it said nothing to show, and [quick]
	 * its quick actions, where the profile's card has any.
	 */
	data class Online(val stats: List<HomeStat>, val quick: QuickActions? = null) : Live
}

/** A card's quick actions: the buttons and toggles of the profile's `card`, and the ui whose actions they run. */
data class QuickActions(val ui: UiDoc, val items: List<QuickAction>)

/** A button or toggle of the profile's `card`, run from the computer's card as from its dashboard. */
data class QuickAction(
	val widget: Widget,
	val action: Action,
	/** A toggle's: on now ([toggledOn]); a button's false. */
	val on: Boolean,
	/** Control is on, and the computer doesn't say the widget is disabled. */
	val enabled: Boolean,
)

/**
 * The card of a computer that answers: the figures of the dashboard its profile's `card` names, or
 * without one those of its first tab, as before 0.10; read with [snapshot] by the computer's own ui
 * ([ui] with true), whose actions the card's quick actions run, asking before them as the profile
 * says now. With [fresh] false, by the ui as kept (false), the computer's own where that is out of
 * date ([readDashboard]). What fails is thrown, for [cardError].
 */
internal suspend fun cardReading(ui: suspend (fresh: Boolean) -> UiDoc, snapshot: suspend (String) -> DataLine, fresh: Boolean = true): Live {
	val (doc, line) = readDashboard(ui, ::cardDashboard, snapshot, fresh = fresh)
	if (line == null) return Live.Online(emptyList())
	val card = ownCard(doc) ?: return Live.Online(homeStats(doc, tabsShown(doc).first(), line.widgets))
	val quick = cardQuick(doc, card, line.widgets)
	return Live.Online(cardStats(doc, card, line), quick.takeIf { it.isNotEmpty() }?.let { QuickActions(doc, it) })
}

/** The dashboard a computer's card shows: the profile's `card` where the ui has it, else the first tab. */
internal fun cardDashboard(ui: UiDoc): String? = ownCard(ui) ?: tabsShown(ui).firstOrNull()

/** The profile's `card`, where the ui has it (the role may see none of it). */
internal fun ownCard(ui: UiDoc): String? = ui.profile.android.card.takeIf { it.isNotEmpty() && ui.dashboard(it) != null }

/**
 * What a reading that failed with [e] says on the card: not paired when the computer turned the
 * token away, offline when it couldn't be reached; anything else, it answered (online, with
 * nothing to show).
 */
internal fun cardError(e: Exception): Live = when ((e as? ElamondException)?.kind) {
	ElamondException.Kind.Unauthorized -> Live.Unpaired
	null, ElamondException.Kind.Unreachable, ElamondException.Kind.CertificateChanged -> Live.Offline
	else -> Live.Online(emptyList())
}

/** A figure on a computer's card: one of its first tab's first gauges, meters and values, or one of its card's. */
data class HomeStat(
	val label: String,
	val text: String,
	/** How full its bar is; null for one without. */
	val fraction: Float?,
	val color: String?,
	val level: String?,
	val blend: Double?,
	/** A `value` with `spark` on the profile's card: its points so far, drawn small under it. */
	val spark: HistoryEntry? = null,
)

private const val HOME_STATS = 3

/** The types of widget a card shows as figures. */
private val FIGURES = setOf("gauge", "meter", "value")

/**
 * The figures a computer's card shows, from a reading of [tab]: the first gauges, meters and
 * values of it with a number, as the CPU, memory and temperature were in 0.5.
 */
fun homeStats(ui: UiDoc, tab: String, states: Map<String, WidgetState>): List<HomeStat> =
	ui.dashboard(tab)?.widgets.orEmpty()
		.filter { it.type in FIGURES }
		.mapNotNull { w -> states[w.id]?.takeIf { it.number != null }?.let { stat(w, it) } }
		.take(HOME_STATS)

/**
 * The figures of the profile's `card` [card], from its reading [line]: every gauge, meter and value
 * of it, those in its groups too, with a state; a value with `spark`, with its points.
 */
fun cardStats(ui: UiDoc, card: String, line: DataLine): List<HomeStat> =
	allWidgets(ui.dashboard(card)?.widgets.orEmpty())
		.filter { it.type in FIGURES }
		.mapNotNull { w ->
			val s = line.widgets[w.id] ?: return@mapNotNull null
			stat(w, s)?.copy(spark = line.history?.get(w.id)?.takeIf { w.type == "value" && w.spark == true })
		}

/** [w]'s figure in the state [s]: its number formatted (a share's bar with it), or the word the service gave; null for neither. */
private fun stat(w: Widget, s: WidgetState): HomeStat? {
	val v = s.number
	val text = s.text ?: v?.let { Fmt.number(it, w.format, w.unit) } ?: Fmt.format(s.v, w.format, w.unit) ?: return null
	val max = s.max ?: w.max ?: if (w.format == "percent") Thresholds.PERCENT.toDouble() else null
	val min = w.min ?: 0.0
	val fraction = if (v == null) null else max?.takeIf { it > min && (w.type != "value" || w.format == "percent") }?.let { ((v - min) / (it - min)).toFloat().coerceIn(0f, 1f) }
	return HomeStat(w.label.orEmpty(), text, fraction, w.color, s.level, s.blend)
}

/**
 * The quick actions of the profile's `card` [card]: its buttons and toggles, in order, those in
 * its groups too, whose action this phone may run there; [states] say which toggles are on.
 */
fun cardQuick(ui: UiDoc, card: String, states: Map<String, WidgetState>): List<QuickAction> =
	allWidgets(ui.dashboard(card)?.widgets.orEmpty())
		.filter { it.type == "button" || (it.type == "toggle" && it.param != null) }
		.mapNotNull { w ->
			val action = ui.action(w.action) ?: return@mapNotNull null
			val s = states[w.id]
			QuickAction(w, action, on = w.type == "toggle" && toggledOn(w, s), enabled = ui.control.enabled && s?.enabled != false)
		}

/**
 * The cards' quick actions: those running, the form one asks for, and what to say of them; kept
 * through a rotation, as a computer's screen keeps its own.
 */
class HomeActions(app: Application) : AndroidViewModel(app) {
	/** An action's form, open over the home screen, the computer it is for, and the ui its card was read by. */
	val form = mutableStateOf<FormRequest?>(null)
	var formFor: String? = null
	var formQuick: QuickActions? = null

	private val _running = MutableStateFlow(emptySet<String>())

	/** The quick actions running now, by computer and widget ([quickKey]). */
	val running: StateFlow<Set<String>> = _running.asStateFlow()

	/** How each ended, for the snackbar: what a done action says, or why it failed. */
	val messages = Channel<Message>(capacity = Limits.MESSAGES_KEPT, onBufferOverflow = BufferOverflow.DROP_OLDEST)

	private val _ended = MutableStateFlow(0)

	/** Counts the actions that ended: the cards are read again at once, a toggle showing where it is now. */
	val ended: StateFlow<Int> = _ended.asStateFlow()

	fun run(machine: Machine, action: Action, params: JsonObject, key: String, typed: String?) {
		if (key in _running.value) return
		_running.update { it + key }
		viewModelScope.launch {
			try {
				val r = runAction(machine, MachineStore.get(getApplication()), UiCache.get(getApplication()), action, params, typed)
				afterRun(action, params, r)?.let { messages.trySend(it) }
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				messages.trySend(messageFor(e))
			} finally {
				_running.update { it - key }
				_ended.update { it + 1 }
			}
		}
	}
}

/** What tells a quick action's run apart: its computer's and its widget's ids. */
fun quickKey(machine: String, widget: String): String = "$machine/$widget"

@Composable
fun HomeScreen(onOpen: (Machine) -> Unit, onPair: (PairTarget) -> Unit, onHelp: () -> Unit, onSettings: () -> Unit) {
	val ctx = LocalContext.current
	val res = resources()
	val store = MachineStore.get(ctx)
	val cache = UiCache.get(ctx)
	val machines by store.machines.collectAsStateWithLifecycle()
	val discovery = remember { discoverElamond(ctx) }
	val found by discovery.collectAsStateWithLifecycle(initialValue = emptyList())
	val unpaired = unpairedFound(found, machines.map { it.id })
	var addDialog by rememberSaveable { mutableStateOf(false) }
	val live = remember { mutableStateMapOf<String, Live>() }
	val haze = remember { HazeState() }
	val list = rememberLazyListState()
	val actions: HomeActions = viewModel()
	val running by actions.running.collectAsStateWithLifecycle()
	val ended by actions.ended.collectAsStateWithLifecycle()
	val guard = LocalGuard.current
	val prefs = LocalPrefs.current
	val haptics = LocalHapticFeedback.current
	val snackbar = remember { SnackbarHostState() }

	// A reading of each computer every few seconds while this screen can be seen; at once again
	// after a quick action, which may have changed what the card shows.
	val lifecycle = LocalLifecycleOwner.current.lifecycle
	LaunchedEffect(machines, lifecycle, ended) {
		lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
			pollEach(machines, Limits.HOME_POLL_MS, probe = { reading(it, store, cache) }, report = { s, l -> live[s.id] = l })
		}
	}
	LaunchedEffect(actions) { actions.messages.receiveAsFlow().collect { snackbar.showSnackbar(it.text(res)) } }
	LaunchedEffect(guard) { guard.messages.collect { snackbar.showSnackbar(it) } }

	/** What runs [s]'s quick actions: the same asking as on its screen, by the ui the card was read by. */
	fun launcher(s: Machine, quick: QuickActions) = ActionLauncher(
		guard,
		run = { a, p, key, typed -> actions.run(s, a, p, key, typed) },
		ui = quick.ui,
		computer = s.name,
		res = res,
		tap = { if (prefs.haptics) haptics.performHapticFeedback(HapticFeedbackType.ContextClick) },
		form = actions.form,
		running = ActionsState(running = running),
	)

	CompositionLocalProvider(LocalHaze provides haze) {
		// The add button is measured first, so the end of the list can scroll clear of it.
		BottomBarLayout(
			Modifier.fillMaxSize(),
			barAlignment = Alignment.End,
			bar = { AddButton(stringResource(R.string.home_add_computer)) { addDialog = true } },
		) {
			Box(Modifier.fillMaxSize()) {
				LazyColumn(
					Modifier.fillMaxSize().hazeSource(haze),
					state = list,
					contentPadding = listPadding(),
					verticalArrangement = Arrangement.spacedBy(Space.gap),
				) {
					item(key = "title") {
						BigTitle(
							stringResource(R.string.app_name),
							subtitle = {
								Text(
									if (machines.isEmpty()) stringResource(R.string.home_subtitle_empty)
									else pluralStringResource(R.plurals.home_computers, machines.size, machines.size),
									style = MaterialTheme.typography.titleSmall,
									color = MaterialTheme.colorScheme.onSurfaceVariant,
								)
							},
							list = list,
						)
					}
					items(machines, key = { it.id }) { s ->
						ComputerCard(s, live[s.id] ?: Live.Unknown, running, onClick = { onOpen(s) }) { quick, q ->
							actions.formFor = s.id
							actions.formQuick = quick
							val tap = tapOf(q.widget, q.action, q.on)
							launcher(s, quick).launch(q.action, tap.params, tap.phrase, key = quickKey(s.id, q.widget.id))
						}
					}
					if (unpaired.isNotEmpty()) {
						item(key = "found-header") { GroupLabel(stringResource(R.string.home_found)) }
						items(unpaired, key = { "found-" + it.id }) { f -> FoundCard(f) { onPair(f.toTarget()) } }
					}
					if (machines.isEmpty()) {
						item(key = "setup") { SetupGuide(onHelp) }
					}
				}

				GlassTopBar(
					title = stringResource(R.string.app_name),
					collapsed = rememberCollapsed(list),
					actions = {
						IconButton(onClick = onHelp) { Icon(ElIcons.Help, stringResource(R.string.help)) }
						IconButton(onClick = onSettings) { Icon(Icons.Default.Settings, stringResource(R.string.settings)) }
					},
				)
				Messages(snackbar, Modifier.align(Alignment.BottomCenter))
			}
		}
	}

	// A quick action's values, where its button leaves some open: asked as on the computer's screen.
	actions.form.value?.let { req ->
		val s = machines.find { it.id == actions.formFor }
		// By the ui the card was read by when tapped: kept through a rotation, and a reading that fails meanwhile.
		val quick = actions.formQuick
		// The computer forgotten since: nothing to run it on.
		if (s == null || quick == null) {
			SideEffect { actions.form.value = null }
		} else {
			ParamForm(req, onDismiss = { actions.form.value = null }) { params -> launcher(s, quick).submit(req, params) }
		}
	}

	if (addDialog) {
		AddDialog(
			onDismiss = { addDialog = false },
			onAddress = { host, port ->
				addDialog = false
				onPair(PairTarget(hosts = listOf(host), port = port, name = null, id = null, fingerprint = null))
			},
		)
	}
}

/**
 * One reading of [s], on whichever of its addresses answers. Its `ui` is the one kept on the
 * phone, asked for where there is none or it is out of date ([cardReading]).
 */
private suspend fun reading(s: Machine, store: MachineStore, cache: UiCache): Live {
	// Taken before asking: forgotten meanwhile, the computer's answer isn't kept.
	val ticket = cache.ticket(s.id)
	return try {
		val client = when (val r = reach(s, store)) {
			is Reach.Reached -> r.client
			Reach.Offline -> return Live.Offline
			Reach.Unpaired -> return Live.Unpaired
			Reach.Outdated -> return Live.Outdated
		}
		cardReading(ui = { fresh -> uiOf(s, client, cache, store, fresh, ticket) }, snapshot = { client.snapshot(it) })
	} catch (e: CancellationException) {
		throw e
	} catch (e: Exception) {
		cardError(e)
	}
}

private fun Found.toTarget() = PairTarget(hosts = listOf(host), port = port, name = name, id = id, fingerprint = null, fingerprintPrefix = fingerprintPrefix)

/** The most figures to a row on a card. */
private const val STATS_IN_A_ROW = 3

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ComputerCard(s: Machine, live: Live, running: Set<String>, onClick: () -> Unit, onQuick: (QuickActions, QuickAction) -> Unit) {
	val c = Metric.colors
	val online = live is Live.Online
	CardSurface {
		Column(
			Modifier
				.background(Brush.verticalGradient(listOf(MaterialTheme.colorScheme.primary.copy(alpha = if (online) Alpha.CARD_ONLINE else Alpha.CARD_OFFLINE), Color.Transparent)))
				.clickable(onClick = onClick)
				.padding(Space.card),
			verticalArrangement = Arrangement.spacedBy(Space.m),
		) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				Box(Modifier.size(Sizes.badgeL).background(MaterialTheme.colorScheme.primaryContainer, CircleShape), contentAlignment = Alignment.Center) {
					Icon(ElIcons.Laptop, null, tint = MaterialTheme.colorScheme.onPrimaryContainer)
				}
				Spacer(Modifier.width(Space.m))
				Column(Modifier.weight(1f)) {
					Text(s.name, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.shrinkToFit(Shrink.LABEL))
					Text(s.hostsToTry().firstOrNull().orEmpty(), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
				}
				Spacer(Modifier.width(Space.s))
				when (live) {
					is Live.Online -> Pill(stringResource(R.string.home_online), c.good, dot = true)
					Live.Offline -> Pill(stringResource(R.string.offline), MaterialTheme.colorScheme.outline, dot = true)
					Live.Unpaired -> Pill(stringResource(R.string.conn_unauthorized), c.warn, dot = true)
					Live.Outdated -> Pill(stringResource(R.string.home_outdated), c.warn, dot = true)
					Live.Unknown -> {}
				}
			}
			if (live !is Live.Online) return@Column
			// The first tab's three figures in a row, as before 0.10; a card's, three to a row.
			if (live.stats.isNotEmpty()) {
				EvenRows(live.stats, STATS_IN_A_ROW) { stat ->
					val accent = accent(stat.color)
					MiniStat(stat, levelColor(stat.level, stat.blend, accent) ?: accent)
				}
			}
			val quick = live.quick ?: return@Column
			FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
				for (q in quick.items) {
					val busy = quickKey(s.id, q.widget.id) in running
					val label = q.widget.label ?: q.action.name
					FilterChip(
						selected = q.on,
						onClick = { onQuick(quick, q) },
						enabled = q.enabled && !busy,
						label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
						leadingIcon = {
							if (busy) Spinner(Sizes.iconS)
							else Icon(icon(q.widget.icon ?: q.action.icon), null, Modifier.size(Sizes.iconS))
						},
						shape = PillShape,
					)
				}
			}
		}
	}
}

@Composable
private fun MiniStat(stat: HomeStat, color: Color) {
	Column(Modifier.background(cardColor(), InnerShape).padding(Space.s), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
		Text(stat.label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.shrinkToFit())
		RollingText(stat.text, MaterialTheme.typography.titleMedium, Modifier.shrinkToFit(), color = if (stat.fraction == null) color else Color.Unspecified)
		// A spark's points, as its tile draws them, small, in its bar's place.
		if (stat.fraction != null && stat.spark == null) UsageBar(stat.fraction, color, height = Sizes.barThin)
		stat.spark?.let { entry ->
			val hist = remember(entry) { Hist(1, sparkWindowMs(entry)).apply { replace(entry) } }
			Sparkline(hist.points(0), color, height = ChartStyle.sparkMini, maxValue = if (stat.fraction != null) Thresholds.PERCENT else null, name = stat.label)
		}
	}
}

/** What a card's spark spans: all the points the computer sent. */
private fun sparkWindowMs(entry: HistoryEntry): Long = (entry.t.lastOrNull() ?: 0L) - (entry.t.firstOrNull() ?: 0L)

@Composable
private fun FoundCard(f: Found, onClick: () -> Unit) {
	CardSurface {
		Row(Modifier.clickable(onClick = onClick).padding(Space.card), verticalAlignment = Alignment.CenterVertically) {
			IconBadge(ElIcons.Monitor, MaterialTheme.colorScheme.tertiary, Sizes.badgeL)
			Spacer(Modifier.width(Space.m))
			Column(Modifier.weight(1f)) {
				Text(f.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text(stringResource(R.string.home_found_tap, f.host), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
}

/** How to get from nothing to a paired computer. */
@Composable
private fun SetupGuide(onHelp: () -> Unit) {
	CardSurface {
		Column(Modifier.padding(Space.card), verticalArrangement = Arrangement.spacedBy(Space.l)) {
			Text(stringResource(R.string.home_setup_title), style = MaterialTheme.typography.titleLarge)
			Text(
				stringResource(R.string.home_setup_intro),
				style = MaterialTheme.typography.bodyMedium,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
			)
			Step(1, stringResource(R.string.home_setup_install), Project.INSTALL)
			Step(2, stringResource(R.string.home_setup_start), null)
			Step(3, stringResource(R.string.home_setup_pair), "elamond pair")
			Step(4, stringResource(R.string.home_setup_scan), null)
			FilledTonalButton(onClick = onHelp, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.home_setup_guide)) }
		}
	}
}

@Composable
private fun Step(n: Int, text: String, command: String?) {
	Row {
		Box(Modifier.size(Sizes.stepNumber).background(MaterialTheme.colorScheme.primaryContainer, CircleShape), contentAlignment = Alignment.Center) {
			Text(n.toString(), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onPrimaryContainer)
		}
		Spacer(Modifier.width(Space.m))
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.s)) {
			// Its first line in the middle of the number's disc.
			val style = MaterialTheme.typography.bodyMedium
			val line = with(LocalDensity.current) { style.lineHeight.toDp() }
			Text(text, style = style, modifier = Modifier.padding(top = ((Sizes.stepNumber - line) / 2).coerceAtLeast(Space.none)))
			if (command != null) {
				Text(
					command,
					fontFamily = FontFamily.Monospace,
					style = MaterialTheme.typography.bodySmall,
					modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainerHighest, InnerShape).padding(Space.m),
				)
			}
		}
	}
}

@Composable
private fun AddDialog(onDismiss: () -> Unit, onAddress: (String, Int) -> Unit) {
	var address by rememberSaveable { mutableStateOf("") }
	val parsed = parseAddress(address, DEFAULT_PORT)
	GlassDialog(
		onDismiss = onDismiss,
		title = stringResource(R.string.home_add_computer),
		icon = ElIcons.Qr,
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
				Text(stringResource(R.string.home_add_qr))
				Text(stringResource(R.string.home_add_or_address), color = MaterialTheme.colorScheme.onSurfaceVariant)
				OutlinedTextField(
					value = address,
					onValueChange = { address = it },
					singleLine = true,
					label = { Text(stringResource(R.string.home_add_address)) },
					placeholder = { Text(stringResource(R.string.home_add_address_hint)) },
					// The keyboard's own key goes on as Next does, once the address reads.
					keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
					keyboardActions = KeyboardActions(onGo = { parsed?.let { onAddress(it.first, it.second) } }),
					modifier = Modifier.fillMaxWidth(),
				)
			}
		},
		confirm = DialogButton(stringResource(R.string.home_add_next), enabled = parsed != null) { parsed?.let { onAddress(it.first, it.second) } },
		dismiss = DialogButton(stringResource(R.string.cancel), onClick = onDismiss),
	)
}
