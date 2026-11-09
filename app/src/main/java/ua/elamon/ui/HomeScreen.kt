package ua.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import ua.elamon.R
import ua.elamon.data.ElamondClient
import ua.elamon.data.DEFAULT_PORT
import ua.elamon.data.Found
import ua.elamon.data.PairTarget
import ua.elamon.data.Server
import ua.elamon.data.ServerStore
import ua.elamon.data.discoverServices

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(onOpen: (Server) -> Unit, onPair: (PairTarget) -> Unit) {
	val ctx = LocalContext.current
	val servers by ServerStore.get(ctx).servers.collectAsStateWithLifecycle()
	val discovery = remember { discoverServices(ctx) }
	val found by discovery.collectAsStateWithLifecycle(initialValue = emptyList())
	val unpaired = found.filter { f -> servers.none { it.id == f.id } }
	var addDialog by rememberSaveable { mutableStateOf(false) }
	val online = remember { mutableStateMapOf<String, Boolean>() }

	// Which computers answer, refreshed while this screen is open.
	LaunchedEffect(servers) {
		while (true) {
			for (s in servers) {
				launch {
					online[s.id] = runCatching {
						ElamondClient.firstReachable(s.hostsToTry(), s.port, s.fingerprint, s.id) != null
					}.getOrDefault(false)
				}
			}
			delay(15_000)
		}
	}

	val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
	Scaffold(
		modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
		topBar = { LargeTopAppBar(title = { Text(stringResource(R.string.app_name)) }, scrollBehavior = scroll) },
		floatingActionButton = {
			ExtendedFloatingActionButton(
				onClick = { addDialog = true },
				icon = { Icon(Icons.Default.Add, null) },
				text = { Text(stringResource(R.string.add_computer)) },
			)
		},
	) { padding ->
		LazyColumn(
			Modifier.fillMaxSize(),
			contentPadding = PaddingValues(16.dp, padding.calculateTopPadding() + 8.dp, 16.dp, padding.calculateBottomPadding() + 96.dp),
			verticalArrangement = Arrangement.spacedBy(12.dp),
		) {
			items(servers, key = { it.id }) { s ->
				ComputerCard(
					icon = ElIcons.Laptop,
					title = s.name,
					subtitle = s.os ?: s.hostsToTry().firstOrNull().orEmpty(),
					online = online[s.id],
					onClick = { onOpen(s) },
				)
			}
			if (unpaired.isNotEmpty()) {
				item(key = "found-header") { Header(stringResource(R.string.found_on_network)) }
				items(unpaired, key = { "found-" + it.id }) { f ->
					ComputerCard(
						icon = ElIcons.Monitor,
						title = f.name,
						subtitle = stringResource(R.string.tap_to_pair, f.host),
						online = null,
						onClick = { onPair(f.toTarget()) },
					)
				}
			}
			if (servers.isEmpty()) {
				item(key = "setup") { SetupGuide() }
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

private fun Found.toTarget() = PairTarget(
	hosts = listOf(host),
	port = port,
	name = name,
	id = id,
	fingerprint = null,
	fingerprintPrefix = fingerprintPrefix,
)

@Composable
private fun Header(text: String) {
	Text(
		text,
		style = MaterialTheme.typography.titleSmall,
		color = MaterialTheme.colorScheme.primary,
		modifier = Modifier.padding(start = 8.dp, top = 12.dp),
	)
}

@Composable
private fun ComputerCard(icon: ImageVector, title: String, subtitle: String, online: Boolean?, onClick: () -> Unit) {
	Surface(
		modifier = Modifier.fillMaxWidth(),
		shape = RoundedCornerShape(24.dp),
		color = MaterialTheme.colorScheme.surfaceContainerLow,
	) {
		Row(
			Modifier.clickable(onClick = onClick).padding(18.dp),
			verticalAlignment = Alignment.CenterVertically,
		) {
			Box(
				Modifier.size(48.dp).background(MaterialTheme.colorScheme.primaryContainer, CircleShape),
				contentAlignment = Alignment.Center,
			) {
				Icon(icon, null, tint = MaterialTheme.colorScheme.onPrimaryContainer)
			}
			Spacer(Modifier.width(16.dp))
			Column(Modifier.weight(1f)) {
				Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text(subtitle, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
				if (online != null) {
					Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 4.dp)) {
						val color = if (online) Metric.colors.good else MaterialTheme.colorScheme.outline
						Box(Modifier.size(8.dp).background(color, CircleShape))
						Spacer(Modifier.width(6.dp))
						Text(
							stringResource(if (online) R.string.online else R.string.offline),
							style = MaterialTheme.typography.labelMedium,
							color = color,
						)
					}
				}
			}
			Icon(Icons.Default.KeyboardArrowRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
		}
	}
}

/** How to get from nothing to a paired computer. */
@Composable
private fun SetupGuide() {
	Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
			Text(stringResource(R.string.setup_title), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
			Text(stringResource(R.string.setup_intro), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			Step(1, stringResource(R.string.setup_step_install), null)
			Step(2, stringResource(R.string.setup_step_start), "systemctl --user enable --now elamond")
			Step(3, stringResource(R.string.setup_step_pair), "elamond pair")
			Step(4, stringResource(R.string.setup_step_scan), null)
		}
	}
}

@Composable
private fun Step(n: Int, text: String, command: String?) {
	Row {
		Box(
			Modifier.size(28.dp).background(MaterialTheme.colorScheme.primaryContainer, CircleShape),
			contentAlignment = Alignment.Center,
		) {
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
					modifier = Modifier
						.fillMaxWidth()
						.background(MaterialTheme.colorScheme.surfaceContainerHighest, RoundedCornerShape(8.dp))
						.padding(horizontal = 10.dp, vertical = 8.dp),
				)
			}
		}
	}
}

@Composable
private fun AddDialog(onDismiss: () -> Unit, onAddress: (String, Int) -> Unit) {
	var address by rememberSaveable { mutableStateOf("") }
	val parsed = ElamondClient.parseAddress(address, DEFAULT_PORT)
	AlertDialog(
		onDismissRequest = onDismiss,
		icon = { Icon(ElIcons.Qr, null) },
		title = { Text(stringResource(R.string.add_computer)) },
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
				Text(stringResource(R.string.add_scan_hint))
				Text(stringResource(R.string.add_address_hint), color = MaterialTheme.colorScheme.onSurfaceVariant)
				OutlinedTextField(
					value = address,
					onValueChange = { address = it },
					singleLine = true,
					label = { Text(stringResource(R.string.address)) },
					placeholder = { Text(stringResource(R.string.address_placeholder)) },
					keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
				)
			}
		},
		confirmButton = {
			TextButton(enabled = parsed != null, onClick = { parsed?.let { onAddress(it.first, it.second) } }) {
				Text(stringResource(R.string.next))
			}
		},
		dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
	)
}
