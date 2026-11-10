package ua.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import ua.elamon.R
import ua.elamon.data.ElamondClient
import ua.elamon.data.DEFAULT_PORT
import ua.elamon.data.Found
import ua.elamon.data.PairTarget
import ua.elamon.data.Sample
import ua.elamon.data.Server
import ua.elamon.data.ServerStore
import ua.elamon.data.discoverServices

/** What the home screen knows about a paired computer. */
private sealed interface Live {
	data object Unknown : Live
	data object Offline : Live
	data class Online(val sample: Sample) : Live
}

@Composable
fun HomeScreen(onOpen: (Server) -> Unit, onPair: (PairTarget) -> Unit, onHelp: () -> Unit, onSettings: () -> Unit) {
	val ctx = LocalContext.current
	val servers by ServerStore.get(ctx).servers.collectAsStateWithLifecycle()
	val discovery = remember { discoverServices(ctx) }
	val found by discovery.collectAsStateWithLifecycle(initialValue = emptyList())
	val unpaired = found.filter { f -> servers.none { it.id == f.id } }
	var addDialog by rememberSaveable { mutableStateOf(false) }
	val live = remember { mutableStateMapOf<String, Live>() }
	val haze = remember { HazeState() }
	val list = rememberLazyListState()

	// A reading of each computer every few seconds while this screen is open.
	LaunchedEffect(servers) {
		while (true) {
			for (s in servers) {
				launch {
					live[s.id] = runCatching {
						val reached = ElamondClient.firstReachable(s.hostsToTry(), s.port, s.fingerprint, s.id) ?: return@runCatching Live.Offline
						val c = ElamondClient(reached.first.host, s.port, s.fingerprint, s.token)
						Live.Online(c.get<Sample>("/api/v1/snapshot"))
					}.getOrDefault(Live.Offline)
				}
			}
			delay(5_000)
		}
	}

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			LazyColumn(
				Modifier.fillMaxSize().hazeSource(haze),
				state = list,
				contentPadding = listPadding(bottomNav = true),
				verticalArrangement = Arrangement.spacedBy(12.dp),
			) {
				item(key = "title") {
					BigTitle(
						stringResource(R.string.app_name),
						subtitle = {
							Text(
								if (servers.isEmpty()) stringResource(R.string.home_subtitle_empty)
								else pluralStringResource(R.plurals.home_computers, servers.size, servers.size),
								style = MaterialTheme.typography.titleSmall,
								color = MaterialTheme.colorScheme.onSurfaceVariant,
							)
						},
						list = list,
					)
				}
				items(servers, key = { it.id }) { s -> ComputerCard(s, live[s.id] ?: Live.Unknown) { onOpen(s) } }
				if (unpaired.isNotEmpty()) {
					item(key = "found-header") { GroupLabel(stringResource(R.string.home_found)) }
					items(unpaired, key = { "found-" + it.id }) { f -> FoundCard(f) { onPair(f.toTarget()) } }
				}
				if (servers.isEmpty()) {
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

			ExtendedFloatingActionButton(
				onClick = { addDialog = true },
				icon = { Icon(Icons.Default.Add, null) },
				text = { Text(stringResource(R.string.home_add_computer)) },
				shape = RoundedCornerShape(28.dp),
				containerColor = MaterialTheme.colorScheme.primaryContainer,
				modifier = Modifier.align(Alignment.BottomEnd).navigationBarsPadding().padding(20.dp),
			)
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

private fun Found.toTarget() = PairTarget(hosts = listOf(host), port = port, name = name, id = id, fingerprint = null, fingerprintPrefix = fingerprintPrefix)

@Composable
private fun ComputerCard(s: Server, live: Live, onClick: () -> Unit) {
	val c = Metric.colors
	val p = LocalPrefs.current
	val online = live is Live.Online
	Surface(modifier = Modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier
				.background(Brush.linearGradient(listOf(MaterialTheme.colorScheme.primary.copy(alpha = if (online) 0.12f else 0.04f), Color.Transparent)))
				.clickable(onClick = onClick)
				.padding(18.dp),
			verticalArrangement = Arrangement.spacedBy(14.dp),
		) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				Box(Modifier.size(52.dp).background(MaterialTheme.colorScheme.primaryContainer, CircleShape), contentAlignment = Alignment.Center) {
					Icon(ElIcons.Laptop, null, tint = MaterialTheme.colorScheme.onPrimaryContainer)
				}
				Spacer(Modifier.width(14.dp))
				Column(Modifier.weight(1f)) {
					Text(s.name, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
					Text(s.os ?: s.hostsToTry().firstOrNull().orEmpty(), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
				}
				when (live) {
					is Live.Online -> Pill(stringResource(R.string.home_online), c.good, dot = true)
					Live.Offline -> Pill(stringResource(R.string.offline), MaterialTheme.colorScheme.outline, dot = true)
					Live.Unknown -> {}
				}
			}
			if (live is Live.Online) {
				val sample = live.sample
				val m = sample.memory
				Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
					MiniStat(stringResource(R.string.cpu), Fmt.percent(sample.cpu.usage), sample.cpu.usage / 100f, c.cpu, Modifier.weight(1f))
					val mf = if (m.total > 0) m.used.toFloat() / m.total else 0f
					MiniStat(stringResource(R.string.ram), Fmt.percent(mf * 100), mf, c.memory, Modifier.weight(1f))
					sample.cpu.temp?.let { MiniStat(stringResource(R.string.home_temp), Fmt.temp(it, p.fahrenheit), it / 100f, tempColor(it), Modifier.weight(1f)) }
				}
			}
		}
	}
}

@Composable
private fun MiniStat(label: String, value: String, fraction: Float, color: Color, modifier: Modifier) {
	Column(modifier.background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(16.dp)).padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
		Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		RollingText(value, MaterialTheme.typography.titleMedium)
		UsageBar(fraction, color, height = 5.dp)
	}
}

@Composable
private fun FoundCard(f: Found, onClick: () -> Unit) {
	Surface(modifier = Modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Row(Modifier.clickable(onClick = onClick).padding(18.dp), verticalAlignment = Alignment.CenterVertically) {
			IconBadge(ElIcons.Monitor, MaterialTheme.colorScheme.tertiary, 44.dp)
			Spacer(Modifier.width(14.dp))
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
		Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
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
		Box(Modifier.size(28.dp).background(MaterialTheme.colorScheme.primaryContainer, CircleShape), contentAlignment = Alignment.Center) {
			Text(n.toString(), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onPrimaryContainer)
		}
		Spacer(Modifier.width(12.dp))
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
			Text(text, style = MaterialTheme.typography.bodyMedium)
			if (command != null) {
				Text(
					command,
					fontFamily = FontFamily.Monospace,
					style = MaterialTheme.typography.bodySmall,
					modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainerHighest, RoundedCornerShape(10.dp)).padding(horizontal = 10.dp, vertical = 8.dp),
				)
			}
		}
	}
}

@Composable
private fun AddDialog(onDismiss: () -> Unit, onAddress: (String, Int) -> Unit) {
	var address by rememberSaveable { mutableStateOf("") }
	val parsed = ElamondClient.parseAddress(address, DEFAULT_PORT)
	GlassDialog(
		onDismiss = onDismiss,
		title = stringResource(R.string.home_add_computer),
		icon = ElIcons.Qr,
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
				Text(stringResource(R.string.home_add_qr))
				Text(stringResource(R.string.home_add_or_address), color = MaterialTheme.colorScheme.onSurfaceVariant)
				OutlinedTextField(
					value = address,
					onValueChange = { address = it },
					singleLine = true,
					label = { Text(stringResource(R.string.home_add_address)) },
					placeholder = { Text(stringResource(R.string.home_add_address_hint)) },
					keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
				)
			}
		},
		confirm = { TextButton(enabled = parsed != null, onClick = { parsed?.let { onAddress(it.first, it.second) } }) { Text(stringResource(R.string.home_add_next)) } },
		dismiss = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
	)
}
