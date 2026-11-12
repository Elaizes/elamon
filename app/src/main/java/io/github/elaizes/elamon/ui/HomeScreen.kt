package io.github.elaizes.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.Migration
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.deviceName
import io.github.elaizes.elamon.pollEach
import io.github.elaizes.elamon.data.APP_API
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.DEFAULT_PORT
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.Found
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.OwnKeys
import io.github.elaizes.elamon.data.discoverElamond
import io.github.elaizes.elamon.data.reachPaired
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
	/** It answers; [stats] are its first tab's figures, none where it said nothing to show. */
	data class Online(val stats: List<HomeStat>) : Live
}

/**
 * The card of a computer that answers: the figures of the first tab of its ui as kept ([ui] with
 * false), read with [snapshot]. A tab the computer refuses (gone from what it shows this phone:
 * renamed, or the role or profile changed) has the ui read from the computer again (true), and the
 * figures once more. What fails is thrown, for [cardError].
 */
internal suspend fun cardReading(ui: suspend (fresh: Boolean) -> UiDoc, snapshot: suspend (String) -> DataLine): Live {
	var doc = ui(false)
	var again = true
	while (true) {
		val tab = tabsShown(doc).firstOrNull() ?: return Live.Online(emptyList())
		try {
			return Live.Online(homeStats(doc, tab, snapshot(tab).widgets))
		} catch (e: ElamondException) {
			val stale = e.status in HTTP_CLIENT_ERRORS && e.kind != ElamondException.Kind.Unauthorized && e.kind != ElamondException.Kind.TooManyRequests
			if (!stale || !again) throw e
			again = false
			doc = ui(true)
		}
	}
}

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

private val HTTP_CLIENT_ERRORS = 400..499

/** A figure on a computer's card: one of its first tab's first gauges, meters and values. */
data class HomeStat(
	val label: String,
	val text: String,
	/** How full its bar is; null for one without. */
	val fraction: Float?,
	val color: String?,
	val level: String?,
	val blend: Double?,
)

/** How many figures a computer's card shows. */
private const val HOME_STATS = 3

/**
 * The figures a computer's card shows, from a reading of [tab]: the first gauges, meters and
 * values of it with a number, as the CPU, memory and temperature were in 0.5.
 */
fun homeStats(ui: UiDoc, tab: String, states: Map<String, WidgetState>): List<HomeStat> =
	ui.dashboard(tab)?.widgets.orEmpty()
		.filter { it.type == "gauge" || it.type == "meter" || it.type == "value" }
		.mapNotNull { w ->
			val s = states[w.id] ?: return@mapNotNull null
			val v = s.number ?: return@mapNotNull null
			val max = s.max ?: w.max ?: if (w.format == "percent") Thresholds.PERCENT.toDouble() else null
			val min = w.min ?: 0.0
			val fraction = max?.takeIf { it > min && (w.type != "value" || w.format == "percent") }?.let { ((v - min) / (it - min)).toFloat().coerceIn(0f, 1f) }
			HomeStat(w.label.orEmpty(), s.text ?: Fmt.number(v, w.format, w.unit), fraction, w.color, s.level, s.blend)
		}
		.take(HOME_STATS)

@Composable
fun HomeScreen(onOpen: (Machine) -> Unit, onPair: (PairTarget) -> Unit, onHelp: () -> Unit, onSettings: () -> Unit) {
	val ctx = LocalContext.current
	val store = MachineStore.get(ctx)
	val ownKeys = OwnKeys.get(ctx)
	val cache = UiCache.get(ctx)
	val settings = Settings.get(ctx)
	val migration by settings.migration.collectAsStateWithLifecycle()
	val device = remember { deviceName(ctx) }
	val machines by store.machines.collectAsStateWithLifecycle()
	val discovery = remember { discoverElamond(ctx) }
	val found by discovery.collectAsStateWithLifecycle(initialValue = emptyList())
	val unpaired = unpairedFound(found, machines.map { it.id })
	var addDialog by rememberSaveable { mutableStateOf(false) }
	val live = remember { mutableStateMapOf<String, Live>() }
	val haze = remember { HazeState() }
	val list = rememberLazyListState()

	// A reading of each computer every few seconds while this screen can be seen.
	val lifecycle = LocalLifecycleOwner.current.lifecycle
	LaunchedEffect(machines, lifecycle) {
		lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
			pollEach(machines, Limits.HOME_POLL_MS, probe = { reading(it, store, ownKeys, cache, device) }, report = { s, l -> live[s.id] = l })
		}
	}

	CompositionLocalProvider(LocalHaze provides haze) {
		// The add button is measured first, so the end of the list can scroll clear of it.
		BottomBarLayout(
			Modifier.fillMaxSize(),
			barAlignment = Alignment.End,
			bar = {
				val add = stringResource(R.string.home_add_computer)
				ExtendedFloatingActionButton(
					onClick = { addDialog = true },
					icon = { Icon(Icons.Default.Add, null) },
					text = { Text(add) },
					shape = RoundedCornerShape(Radius.field),
					containerColor = MaterialTheme.colorScheme.primaryContainer,
					// Material's extended button hides its text from screen readers: the button says it instead.
					modifier = Modifier.navigationBarsPadding().padding(Space.xl).semantics { contentDescription = add },
				)
			},
		) {
			Box(Modifier.fillMaxSize()) {
				LazyColumn(
					Modifier.fillMaxSize().hazeSource(haze),
					state = list,
					contentPadding = listPadding(),
					verticalArrangement = Arrangement.spacedBy(Space.m),
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
					migration?.let { toml -> item(key = "migration") { MigrationCard(toml, machines) { settings.dismissMigration() } } }
					items(machines, key = { it.id }) { s -> ComputerCard(s, live[s.id] ?: Live.Unknown) { onOpen(s) } }
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
			}
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
 * One reading of [s], on whichever of its addresses answers; a computer whose token came with
 * Import pairings is asked for a key of this phone's own, named [device], first. Its `ui` is the
 * one kept on the phone, asked for where there is none or its first tab is refused ([cardReading]).
 */
private suspend fun reading(s: Machine, store: MachineStore, ownKeys: OwnKeys, cache: UiCache, device: String): Live {
	// Opened by the key store off the main thread, the first time.
	val token = withContext(Dispatchers.Default) { store.token(s) } ?: return Live.Unpaired
	// Taken before asking: forgotten meanwhile, the computer's answer isn't kept.
	val ticket = cache.ticket(s.id)
	return try {
		// A certificate the computer renewed is followed and kept here too.
		var pin = s.fingerprint
		val reached = reachPaired(s) { from, to ->
			pin = to
			// Kept only while still pinned to the one followed: a pairing made meanwhile stays as it is.
			store.repin(s.id, from, to)
		} ?: return Live.Offline
		if (reached.second.api != APP_API) return Live.Outdated
		val host = reached.first.host
		val own = ownKeys.renew(s.id, device) { ElamondClient(host, s.port, pin, token).rekey(it) }
		val client = ElamondClient(host, s.port, pin, own ?: token)
		cardReading(
			ui = { fresh -> (if (fresh) null else cache.ui(s.id)) ?: client.ui().also { cache.putUi(s.id, it, ticket) } },
			snapshot = { client.snapshot(it) },
		)
	} catch (e: CancellationException) {
		throw e
	} catch (e: Exception) {
		cardError(e)
	}
}

private fun Found.toTarget() = PairTarget(hosts = listOf(host), port = port, name = name, id = id, fingerprint = null, fingerprintPrefix = fingerprintPrefix)

@Composable
private fun ComputerCard(s: Machine, live: Live, onClick: () -> Unit) {
	val c = Metric.colors
	val online = live is Live.Online
	Surface(modifier = Modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier
				.background(Brush.linearGradient(listOf(MaterialTheme.colorScheme.primary.copy(alpha = if (online) Alpha.CARD_ONLINE else Alpha.CARD_OFFLINE), Color.Transparent)))
				.clickable(onClick = onClick)
				.padding(Space.lPlus),
			verticalArrangement = Arrangement.spacedBy(Space.mPlus),
		) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				Box(Modifier.size(Sizes.badgeComputer).background(MaterialTheme.colorScheme.primaryContainer, CircleShape), contentAlignment = Alignment.Center) {
					Icon(ElIcons.Laptop, null, tint = MaterialTheme.colorScheme.onPrimaryContainer)
				}
				Spacer(Modifier.width(Space.mPlus))
				Column(Modifier.weight(1f)) {
					Text(s.name, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
					Text(s.os ?: s.hostsToTry().firstOrNull().orEmpty(), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
				}
				when (live) {
					is Live.Online -> Pill(stringResource(R.string.home_online), c.good, dot = true)
					Live.Offline -> Pill(stringResource(R.string.offline), MaterialTheme.colorScheme.outline, dot = true)
					Live.Unpaired -> Pill(stringResource(R.string.conn_unauthorized), c.warn, dot = true)
					Live.Outdated -> Pill(stringResource(R.string.home_outdated), c.warn, dot = true)
					Live.Unknown -> {}
				}
			}
			if (live is Live.Online && live.stats.isNotEmpty()) {
				Row(Modifier.height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(Space.sPlus)) {
					for (stat in live.stats) {
						val accent = accent(stat.color)
						MiniStat(stat.label, stat.text, stat.fraction, levelColor(stat.level, stat.blend, accent) ?: accent, Modifier.weight(1f).fillMaxHeight())
					}
				}
			}
		}
	}
}

@Composable
private fun MiniStat(label: String, value: String, fraction: Float?, color: Color, modifier: Modifier) {
	Column(modifier.background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(Radius.l)).padding(Space.m), verticalArrangement = Arrangement.spacedBy(Space.xsPlus)) {
		Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
		RollingText(value, MaterialTheme.typography.titleMedium, color = if (fraction == null) color else Color.Unspecified)
		if (fraction != null) UsageBar(fraction, color, height = Sizes.barMini)
	}
}

/**
 * Once after updating from 0.5: the settings it kept on the phone, as the profile that gives the
 * same on a computer, and how to give it to this phone there.
 */
@Composable
private fun MigrationCard(toml: String, machines: List<Machine>, onDismiss: () -> Unit) {
	val ctx = LocalContext.current
	Surface(shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(Space.xl), verticalArrangement = Arrangement.spacedBy(Space.m)) {
			Text(stringResource(R.string.migration_title), style = MaterialTheme.typography.titleLarge)
			Text(stringResource(R.string.migration_body), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			CodeBlock(toml)
			Text(stringResource(R.string.migration_apply), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			val commands = machines.ifEmpty { null }?.joinToString("\n") { "elamond clients profile ${it.clientId ?: "ID"} ${Migration.PROFILE_ID}" }
				?: "elamond clients profile ID ${Migration.PROFILE_ID}"
			CodeBlock(commands)
			if (machines.size > 1) Text(stringResource(R.string.migration_each), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			// Unlocks this phone asked for that the computers' default profile doesn't: gone until the profile is given.
			val more = Migration.unlocksMore(toml).mapNotNull { k -> Guarded.entries.find { it.key == k } }
			if (more.isNotEmpty()) {
				val kinds = more.map { stringResource(it.title()) }.joinToString(", ")
				Text(stringResource(R.string.migration_unlock, kinds), style = MaterialTheme.typography.bodyMedium, color = Metric.colors.warn)
			}
			Row(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
				FilledTonalButton(onClick = { copy(ctx, toml) }, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.copy)) }
				TextButton(onClick = onDismiss, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.migration_done)) }
			}
		}
	}
}

@Composable
private fun FoundCard(f: Found, onClick: () -> Unit) {
	Surface(modifier = Modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Row(Modifier.clickable(onClick = onClick).padding(Space.lPlus), verticalAlignment = Alignment.CenterVertically) {
			IconBadge(ElIcons.Monitor, MaterialTheme.colorScheme.tertiary, Sizes.badgeFound)
			Spacer(Modifier.width(Space.mPlus))
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
	Surface(shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(Space.xl), verticalArrangement = Arrangement.spacedBy(Space.mPlus)) {
			Text(stringResource(R.string.home_setup_title), style = MaterialTheme.typography.titleLarge)
			Text(
				stringResource(R.string.home_setup_intro),
				style = MaterialTheme.typography.bodyMedium,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
			)
			Step(1, stringResource(R.string.home_setup_install), null)
			Step(2, stringResource(R.string.home_setup_start), "systemctl --user enable --now elamond")
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
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xsPlus)) {
			Text(text, style = MaterialTheme.typography.bodyMedium)
			if (command != null) {
				Text(
					command,
					fontFamily = FontFamily.Monospace,
					style = MaterialTheme.typography.bodySmall,
					modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainerHighest, RoundedCornerShape(Radius.sPlus)).padding(horizontal = Space.sPlus, vertical = Space.s),
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
				)
			}
		},
		confirm = { TextButton(enabled = parsed != null, onClick = { parsed?.let { onAddress(it.first, it.second) } }) { Text(stringResource(R.string.home_add_next)) } },
		dismiss = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
	)
}
