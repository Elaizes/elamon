package ua.elamon.ui

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import androidx.annotation.StringRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import ua.elamon.Conn
import ua.elamon.R
import ua.elamon.data.ActionInfo
import ua.elamon.data.ActionResult
import ua.elamon.data.ControlInfo
import ua.elamon.data.Listener
import ua.elamon.data.Player
import ua.elamon.data.ServiceDef
import ua.elamon.data.ServiceStatus
import ua.elamon.data.UnitInfo
import ua.elamon.data.serviceUrl

@Composable
fun ControlTab(t: TabScope) {
	val vm = t.vm
	val control by vm.control.collectAsStateWithLifecycle()
	val controlError by vm.controlError.collectAsStateWithLifecycle()
	val s = t.state.sample ?: return
	var open by remember { mutableStateOf<ServiceStatus?>(null) }
	var edit by remember { mutableStateOf<ServiceStatus?>(null) }
	var adding by remember { mutableStateOf(false) }
	var localOnly by remember { mutableStateOf<ServiceStatus?>(null) }
	val ctx = LocalContext.current

	DisposableEffect(Unit) {
		vm.refreshControl()
		vm.setMedia(true)
		onDispose { vm.setMedia(false) }
	}

	fun launch(svc: ServiceStatus) {
		val host = t.state.host ?: return
		runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(serviceUrl(svc.url, host)))) }
	}

	val enabled = control?.enabled == true
	LazyColumn(
		Modifier.fillMaxSize(),
		state = t.list,
		contentPadding = listPadding(),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		item(key = "header") { t.header() }
		controlError?.let { e ->
			item(key = "error") { ControlError(e.text(resources())) { vm.refreshControl() } }
		}
		if (control != null && !enabled) {
			item(key = "off") { ControlOff { t.openHelp(null) } }
		}
		item(key = "services") {
			ServicesSection(
				services = s.services,
				onOpen = { if (it.localOnly) localOnly = it else launch(it) },
				onMenu = { open = it },
				onAdd = { adding = true },
			)
		}
		item(key = "displays") { DisplaysSection(t.state) }
		val media = s.media
		if (enabled && media != null && (media.players.isNotEmpty() || media.volume != null)) {
			item(key = "media") { MediaSection(t, media.players, media.volume) }
		}
		control?.profiles?.takeIf { enabled && it.available.isNotEmpty() }?.let { p ->
			item(key = "profile") { ProfileSection(s.power.ppdProfile ?: p.active, p.available) { vm.setProfile(it) } }
		}
		control?.actions?.takeIf { enabled && it.isNotEmpty() }?.let { actions ->
			item(key = "actions") { ActionsSection(t, actions) }
		}
		if (enabled) {
			item(key = "power") { PowerSection(t, control!!) }
		}
	}

	open?.let { svc ->
		ServiceSheet(
			t, svc, enabled,
			onDismiss = { open = null },
			onOpen = { open = null; if (svc.localOnly) localOnly = svc else launch(svc) },
			onEdit = { open = null; edit = svc },
		)
	}
	if (adding || edit != null) {
		ServiceEditor(t, edit, onDismiss = { adding = false; edit = null })
	}
	localOnly?.let { svc ->
		GlassDialog(
			onDismiss = { localOnly = null },
			title = stringResource(R.string.control_local_only_title),
			icon = Icons.Default.Warning,
			text = {
				Text(stringResource(R.string.control_local_only_body, svc.name))
			},
			confirm = { TextButton(onClick = { localOnly = null; launch(svc) }) { Text(stringResource(R.string.control_open_anyway)) } },
			dismiss = { TextButton(onClick = { localOnly = null }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

/** A new service's address before its port: the computer's own address, over plain HTTP. */
private const val SERVICE_URL_START = "http://{host}:"

@Composable
private fun ControlOff(onHelp: () -> Unit) {
	Section(stringResource(R.string.control_off), ElIcons.Lock, accent = MaterialTheme.colorScheme.secondary) {
		Text(
			stringResource(R.string.control_off_body),
			style = MaterialTheme.typography.bodyMedium,
		)
		CodeBlock("control = true")
		Text(stringResource(R.string.control_off_where), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
		TextButton(onClick = onHelp) { Text(stringResource(R.string.help)) }
	}
}

/** The control panel couldn't be read; what's shown below may be old or missing. */
@Composable
private fun ControlError(message: String, onRetry: () -> Unit) {
	Section(stringResource(R.string.control_error_title), Icons.Default.Warning, accent = Metric.colors.warn) {
		Text(message, style = MaterialTheme.typography.bodyMedium)
		TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
	}
}

// ---- services --------------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ServicesSection(services: List<ServiceStatus>, onOpen: (ServiceStatus) -> Unit, onMenu: (ServiceStatus) -> Unit, onAdd: () -> Unit) {
	Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
		Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(start = 4.dp)) {
			Text(stringResource(R.string.control_services), style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
			FilledIconButton(onClick = onAdd, colors = IconButtonDefaults.filledIconButtonColors(containerColor = MaterialTheme.colorScheme.primaryContainer, contentColor = MaterialTheme.colorScheme.onPrimaryContainer)) {
				Icon(Icons.Default.Add, stringResource(R.string.control_add_service))
			}
		}
		if (services.isEmpty()) {
			Surface(shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
				Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
					Text(stringResource(R.string.control_services_empty), style = MaterialTheme.typography.titleMedium)
					Text(
						stringResource(R.string.control_services_empty_body),
						style = MaterialTheme.typography.bodyMedium,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
					)
				}
			}
		} else {
			FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(10.dp), maxItemsInEachRow = 2) {
				for (svc in services) ServiceTile(svc, Modifier.weight(1f), onOpen = { onOpen(svc) }, onMenu = { onMenu(svc) })
				if (services.size % 2 == 1) Spacer(Modifier.weight(1f))
			}
		}
	}
}

@Composable
private fun serviceState(svc: ServiceStatus): Pair<String, Color> {
	val c = Metric.colors
	val (label, color) = when {
		svc.unitState == "failed" -> R.string.service_failed to c.hot
		svc.unitState == "missing" -> R.string.service_no_unit to c.warn
		svc.unitState == "activating" || svc.unitState == "reloading" -> R.string.service_starting to c.warn
		svc.unitState == "deactivating" -> R.string.service_stopping to c.warn
		svc.up == true && svc.localOnly -> R.string.service_local_only to c.warn
		svc.up == true -> R.string.service_running to c.good
		svc.unitState == "active" -> R.string.service_running to c.good
		svc.up == false -> R.string.service_stopped to MaterialTheme.colorScheme.outline
		else -> R.string.service_unknown to MaterialTheme.colorScheme.outline
	}
	return stringResource(label) to color
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
private fun ServiceTile(svc: ServiceStatus, modifier: Modifier, onOpen: () -> Unit, onMenu: () -> Unit) {
	val haptics = LocalHapticFeedback.current
	val prefs = LocalPrefs.current
	val (label, color) = serviceState(svc)
	Surface(modifier = modifier, shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier
				.combinedClickable(onClick = onOpen, onLongClick = {
					if (prefs.haptics) haptics.performHapticFeedback(HapticFeedbackType.LongPress)
					onMenu()
				})
				.padding(14.dp),
			verticalArrangement = Arrangement.spacedBy(10.dp),
		) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				LetterAvatar(svc.name, 40.dp)
				Spacer(Modifier.weight(1f))
				IconButton(onClick = onMenu, modifier = Modifier.size(32.dp)) {
					Icon(Icons.Default.MoreVert, stringResource(R.string.control_service_menu), modifier = Modifier.size(18.dp))
				}
			}
			Column {
				Text(svc.name, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text(svc.port?.let { ":$it" } ?: svc.url, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
			}
			Pill(label, color, dot = true)
		}
	}
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ServiceSheet(t: TabScope, svc: ServiceStatus, control: Boolean, onDismiss: () -> Unit, onOpen: () -> Unit, onEdit: () -> Unit) {
	val prefs = LocalPrefs.current
	var confirm by remember { mutableStateOf<String?>(null) }
	ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(start = 24.dp, end = 24.dp, bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				LetterAvatar(svc.name, 48.dp)
				Spacer(Modifier.width(14.dp))
				Column(Modifier.weight(1f)) {
					Text(svc.name, style = MaterialTheme.typography.headlineSmall)
					Text(svc.url, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
				}
				val (label, color) = serviceState(svc)
				Pill(label, color, dot = true)
			}
			svc.unit?.let { StatRow(stringResource(R.string.control_unit), "$it · ${svc.scope}") }
			svc.unitState?.let { StatRow(stringResource(R.string.control_unit_state), it) }
			svc.port?.let { StatRow(stringResource(R.string.control_port), if (svc.localOnly) stringResource(R.string.control_port_computer_only, it) else it.toString()) }
			Button(onClick = onOpen, modifier = Modifier.fillMaxWidth()) {
				Icon(ElIcons.Globe, null, Modifier.size(18.dp))
				Spacer(Modifier.width(8.dp))
				Text(stringResource(R.string.control_open))
			}
			if (svc.unit != null) {
				if (control) {
					Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
						val running = svc.unitState == "active" || svc.unitState == "activating"
						FilledTonalButton(onClick = { t.vm.unit(svc.id, if (running) "stop" else "start") }, modifier = Modifier.weight(1f)) {
							Icon(if (running) ElIcons.Pause else ElIcons.Play, null, Modifier.size(16.dp))
							Spacer(Modifier.width(6.dp))
							Text(stringResource(if (running) R.string.control_stop else R.string.control_start))
						}
						FilledTonalButton(onClick = { if (prefs.confirmActions) confirm = "restart" else t.vm.unit(svc.id, "restart") }, modifier = Modifier.weight(1f)) {
							Icon(ElIcons.Restart, null, Modifier.size(16.dp))
							Spacer(Modifier.width(6.dp))
							Text(stringResource(R.string.control_restart))
						}
					}
				} else {
					Text(stringResource(R.string.control_needs_control), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			if (svc.editable) {
				Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
					TextButton(onClick = onEdit) {
						Icon(Icons.Default.Edit, null, Modifier.size(16.dp))
						Spacer(Modifier.width(6.dp))
						Text(stringResource(R.string.control_edit))
					}
					TextButton(onClick = { confirm = "remove" }) {
						Icon(Icons.Default.Delete, null, Modifier.size(16.dp), tint = Metric.colors.hot)
						Spacer(Modifier.width(6.dp))
						Text(stringResource(R.string.control_remove), color = Metric.colors.hot)
					}
				}
			} else {
				Text(stringResource(R.string.control_set_on_computer), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
	confirm?.let { what ->
		GlassDialog(
			onDismiss = { confirm = null },
			title = stringResource(if (what == "remove") R.string.control_remove_title else R.string.control_restart_title, svc.name),
			text = { Text(stringResource(if (what == "remove") R.string.control_remove_body else R.string.control_restart_body)) },
			confirm = {
				TextButton(onClick = {
					if (what == "remove") t.vm.deleteService(svc.id) else t.vm.unit(svc.id, "restart")
					confirm = null
					onDismiss()
				}) { Text(stringResource(if (what == "remove") R.string.control_remove else R.string.control_restart)) }
			},
			dismiss = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
private fun ServiceEditor(t: TabScope, existing: ServiceStatus?, onDismiss: () -> Unit) {
	var name by remember { mutableStateOf(existing?.name.orEmpty()) }
	var url by remember { mutableStateOf(existing?.url ?: SERVICE_URL_START) }
	var unit by remember { mutableStateOf(existing?.unit.orEmpty()) }
	var scope by remember { mutableStateOf(existing?.scope ?: "user") }
	var detected by remember { mutableStateOf<List<Listener>>(emptyList()) }
	var units by remember { mutableStateOf<List<UnitInfo>>(emptyList()) }
	val taken = t.state.sample?.services.orEmpty().mapNotNull { it.port }.toSet()

	LaunchedEffect(Unit) { detected = t.vm.detected() }
	LaunchedEffect(scope) { units = t.vm.units(scope) }

	ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier.padding(start = 24.dp, end = 24.dp, bottom = 32.dp).verticalScroll(rememberScrollState()),
			verticalArrangement = Arrangement.spacedBy(12.dp),
		) {
			Text(if (existing == null) stringResource(R.string.control_add_title) else stringResource(R.string.control_edit_title, existing.name), style = MaterialTheme.typography.headlineSmall)
			val suggestions = detected.filter { it.port !in taken && it.port < 32768 && it.process != "elamond" }
			if (existing == null && suggestions.isNotEmpty()) {
				Text(stringResource(R.string.control_listening), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
				Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
					for (l in suggestions) {
						FilterChip(
							selected = url.endsWith(":${l.port}") || url.contains(":${l.port}/"),
							onClick = {
								url = "$SERVICE_URL_START${l.port}"
								if (name.isBlank()) l.process?.let { p -> name = p.replaceFirstChar { it.uppercase() } }
							},
							label = {
								val port = ":${l.port}" + (l.process?.let { " $it" } ?: "")
								Text(if (l.localOnly) stringResource(R.string.control_listening_local, port) else port)
							},
						)
					}
				}
			}
			OutlinedTextField(value = name, onValueChange = { name = it.take(64) }, label = { Text(stringResource(R.string.control_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
			OutlinedTextField(
				value = url,
				onValueChange = { url = it.take(512) },
				label = { Text(stringResource(R.string.control_address)) },
				supportingText = { Text(stringResource(R.string.control_address_hint)) },
				singleLine = true,
				modifier = Modifier.fillMaxWidth(),
			)
			Text(stringResource(R.string.control_unit_optional), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
			SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
				listOf("user" to R.string.control_scope_user, "system" to R.string.control_scope_system).forEachIndexed { i, (key, label) ->
					SegmentedButton(selected = scope == key, onClick = { scope = key }, shape = SegmentedButtonDefaults.itemShape(i, 2)) { Text(stringResource(label)) }
				}
			}
			OutlinedTextField(value = unit, onValueChange = { unit = it.trim().take(256) }, label = { Text(stringResource(R.string.control_unit)) }, placeholder = { Text("name.service") }, singleLine = true, modifier = Modifier.fillMaxWidth())
			val matches = units.filter { unit.isBlank() || it.name.contains(unit, true) || it.description.contains(unit, true) }.take(8)
			if (matches.isNotEmpty() && matches.none { it.name == unit }) {
				FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
					for (u in matches) FilterChip(selected = false, onClick = { unit = u.name }, label = { Text(u.name.removeSuffix(".service")) })
				}
			}
			Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 4.dp)) {
				TextButton(onClick = onDismiss, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.cancel)) }
				Button(
					enabled = name.isNotBlank() && url.length > 3,
					onClick = {
						t.vm.saveService(ServiceDef(existing?.id, name.trim(), url.trim(), unit.ifBlank { null }, scope))
						onDismiss()
					},
					modifier = Modifier.weight(1f),
				) { Text(stringResource(R.string.control_save)) }
			}
		}
	}
}

// ---- media ------------------------------------------------------------------------------

@Composable
private fun MediaSection(t: TabScope, players: List<Player>, volume: ua.elamon.data.Volume?) {
	var chosen by remember { mutableStateOf<String?>(null) }
	val player = players.find { it.id == chosen } ?: players.firstOrNull()
	Surface(shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
		Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				IconBadge(ElIcons.Music, MaterialTheme.colorScheme.tertiary)
				Spacer(Modifier.width(12.dp))
				Text(player?.name ?: stringResource(R.string.control_media), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
			}
			if (players.size > 1) {
				Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
					for (p in players) FilterChip(selected = p.id == player?.id, onClick = { chosen = p.id }, label = { Text(p.name) })
				}
			}
			if (player != null) NowPlaying(t, player)
			if (volume != null) VolumeRow(t, volume)
		}
	}
}

/** How big a cover shows, and so how big it is decoded. */
private val ART_SIZE = 72.dp

/** How often a playing track's position moves on between samples. */
private const val POSITION_TICK_MS = 500L

@Composable
private fun NowPlaying(t: TabScope, p: Player) {
	val artPx = with(LocalDensity.current) { ART_SIZE.roundToPx() }
	var art by remember(p.id, p.title) { mutableStateOf<Bitmap?>(null) }
	LaunchedEffect(p.id, p.title, p.hasArt) { art = if (p.hasArt) t.vm.art(p.id, artPx) else null }
	val playing = p.status == "Playing"
	// The service reports once a sample, maybe seconds apart; in between the position moves on by itself.
	val moving = playing && t.state.conn == Conn.Live
	var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
	LaunchedEffect(moving) {
		while (moving) {
			now = System.currentTimeMillis()
			delay(POSITION_TICK_MS)
		}
	}
	Row(verticalAlignment = Alignment.CenterVertically) {
		Box(
			Modifier.size(ART_SIZE).clip(RoundedCornerShape(16.dp)).background(
				Brush.linearGradient(listOf(MaterialTheme.colorScheme.tertiaryContainer, MaterialTheme.colorScheme.primaryContainer))
			),
			contentAlignment = Alignment.Center,
		) {
			val a = art
			if (a != null) Image(a.asImageBitmap(), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
			else Icon(ElIcons.Music, null, tint = MaterialTheme.colorScheme.onTertiaryContainer, modifier = Modifier.size(32.dp))
		}
		Spacer(Modifier.width(14.dp))
		Column(Modifier.weight(1f)) {
			Text(p.title ?: stringResource(if (p.status == "Stopped") R.string.control_nothing_playing else R.string.control_unknown_track), style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
			listOfNotNull(p.artist, p.album).joinToString(" · ").takeIf { it.isNotEmpty() }?.let {
				Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
			}
		}
	}
	val length = p.length
	if (length != null && length > 0) {
		// A new sample may be newer than the last tick: it is where the position starts again.
		val pos = playbackPosition(p.position, length, t.state.receivedAt, maxOf(now, t.state.receivedAt), moving)
		var dragging by remember { mutableStateOf<Float?>(null) }
		Column {
			Slider(
				value = dragging ?: (pos.toFloat() / length),
				onValueChange = { dragging = it },
				onValueChangeFinished = {
					dragging?.let { f -> t.vm.media(p.id, "seek", (f * length / 1e6)) }
					dragging = null
				},
				enabled = p.canSeek,
			)
			Row {
				Text(Fmt.clock(pos), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				Spacer(Modifier.weight(1f))
				Text(Fmt.clock(length), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
	Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.CenterVertically) {
		IconButton(onClick = { t.vm.media(p.id, "previous") }, enabled = p.canPrevious) { Icon(ElIcons.Previous, stringResource(R.string.control_previous)) }
		FilledIconButton(onClick = { t.vm.media(p.id, "play_pause") }, enabled = p.canPlay || p.canPause, modifier = Modifier.size(64.dp)) {
			Icon(if (playing) ElIcons.Pause else ElIcons.Play, stringResource(if (playing) R.string.control_pause else R.string.control_play), modifier = Modifier.size(30.dp))
		}
		IconButton(onClick = { t.vm.media(p.id, "next") }, enabled = p.canNext) { Icon(ElIcons.Next, stringResource(R.string.control_next)) }
	}
}

@Composable
private fun VolumeRow(t: TabScope, v: ua.elamon.data.Volume) {
	var dragging by remember { mutableStateOf<Float?>(null) }
	Row(verticalAlignment = Alignment.CenterVertically) {
		IconButton(onClick = { t.vm.volume(muted = !v.muted) }) {
			Icon(if (v.muted) ElIcons.Mute else ElIcons.Volume, stringResource(if (v.muted) R.string.control_unmute else R.string.control_mute), tint = if (v.muted) Metric.colors.hot else MaterialTheme.colorScheme.onSurface)
		}
		Slider(
			value = dragging ?: v.level.coerceIn(0f, 1f),
			onValueChange = { dragging = it },
			onValueChangeFinished = {
				dragging?.let { t.vm.volume(level = it) }
				dragging = null
			},
			modifier = Modifier.weight(1f),
		)
		Text(Fmt.percent((dragging ?: v.level) * 100), style = MaterialTheme.typography.labelLarge, modifier = Modifier.width(44.dp).padding(start = 6.dp))
	}
}

// ---- power profile, actions, power --------------------------------------------------------

@Composable
private fun ProfileSection(active: String, available: List<String>, onPick: (String) -> Unit) {
	Section(stringResource(R.string.control_power_profile), ElIcons.Bolt, accent = Metric.colors.battery) {
		Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
			for (p in available) {
				val on = p == active
				val bg by animateColorAsState(if (on) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerHigh, label = "profile")
				val fg = if (on) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant
				Column(
					Modifier.weight(1f).fillMaxHeight().clip(RoundedCornerShape(18.dp)).background(bg)
						.combinedClickableCompat { onPick(p) }
						.padding(vertical = 14.dp, horizontal = 6.dp),
					horizontalAlignment = Alignment.CenterHorizontally,
					verticalArrangement = Arrangement.spacedBy(6.dp),
				) {
					Icon(profileIcon(p), null, tint = fg)
					Text(profileName(p), style = MaterialTheme.typography.labelLarge, color = fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
				}
			}
		}
	}
}

private fun profileIcon(p: String): ImageVector = when (p) {
	"power-saver", "low-power", "quiet", "cool" -> ElIcons.Leaf
	"performance" -> ElIcons.Rocket
	else -> ElIcons.Gauge
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
private fun Modifier.combinedClickableCompat(onClick: () -> Unit) = this.then(Modifier.combinedClickable(onClick = onClick))

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ActionsSection(t: TabScope, actions: List<ActionInfo>) {
	val prefs = LocalPrefs.current
	val state by t.vm.actions.collectAsStateWithLifecycle()
	var confirm by remember { mutableStateOf<ActionInfo?>(null) }
	Section(stringResource(R.string.control_actions), ElIcons.Terminal, accent = MaterialTheme.colorScheme.tertiary) {
		for (a in actions) {
			val last = state.results[a.id]
			Surface(shape = RoundedCornerShape(18.dp), color = MaterialTheme.colorScheme.surfaceContainerHigh, modifier = Modifier.fillMaxWidth()) {
				Row(
					Modifier.combinedClickableCompat { if (state.running == null) { if (a.confirm && prefs.confirmActions) confirm = a else t.vm.runAction(a.id) } }
						.padding(horizontal = 16.dp, vertical = 14.dp),
					verticalAlignment = Alignment.CenterVertically,
				) {
					Icon(ElIcons.Play, null, tint = MaterialTheme.colorScheme.tertiary, modifier = Modifier.size(18.dp))
					Spacer(Modifier.width(12.dp))
					Text(a.name, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
					when {
						state.running == a.id -> androidx.compose.material3.CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
						// The last run's outcome; its output opens on a tap.
						last != null -> ActionOutcome(last, Modifier.clip(RoundedCornerShape(50)).combinedClickableCompat { t.vm.showActionResult(a.id) })
					}
				}
			}
		}
	}
	confirm?.let { a ->
		GlassDialog(
			onDismiss = { confirm = null },
			title = stringResource(R.string.control_run_title, a.name),
			icon = ElIcons.Terminal,
			confirm = { TextButton(onClick = { confirm = null; t.vm.runAction(a.id) }) { Text(stringResource(R.string.control_run)) } },
			dismiss = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
		)
	}
	val shown = state.shown?.let { id -> actions.find { it.id == id }?.let { a -> state.results[id]?.let { a to it } } }
	shown?.let { (a, r) ->
		ModalBottomSheet(onDismissRequest = { t.vm.showActionResult(null) }, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
			Column(Modifier.padding(start = 24.dp, end = 24.dp, bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
				Row(verticalAlignment = Alignment.CenterVertically) {
					Text(a.name, style = MaterialTheme.typography.headlineSmall, modifier = Modifier.weight(1f))
					ActionOutcome(r)
				}
				Surface(shape = RoundedCornerShape(14.dp), color = MaterialTheme.colorScheme.surfaceContainerHighest, modifier = Modifier.fillMaxWidth().heightIn(max = 360.dp)) {
					SelectionContainer {
						Text(
							r.output.ifBlank { stringResource(R.string.control_no_output) },
							fontFamily = FontFamily.Monospace,
							style = MaterialTheme.typography.bodySmall,
							modifier = Modifier.verticalScroll(rememberScrollState()).padding(12.dp),
						)
					}
				}
			}
		}
	}
}

@Composable
private fun ActionOutcome(r: ActionResult, modifier: Modifier = Modifier) {
	when {
		r.timedOut -> Pill(stringResource(R.string.control_timed_out), Metric.colors.warn, modifier, dot = true)
		r.exitCode == 0 -> Pill(stringResource(R.string.control_done), Metric.colors.good, modifier, dot = true)
		else -> Pill(stringResource(R.string.control_exit_code, r.exitCode?.toString() ?: "?"), Metric.colors.hot, modifier, dot = true)
	}
}

private data class PowerAction(val key: String, @StringRes val label: Int, val icon: ImageVector, @StringRes val ask: Int, @StringRes val explain: Int)

private val POWER = listOf(
	PowerAction("lock", R.string.power_lock, ElIcons.Lock, R.string.power_lock_ask, R.string.power_lock_explain),
	PowerAction("suspend", R.string.power_sleep, ElIcons.Moon, R.string.power_sleep_ask, R.string.power_sleep_explain),
	PowerAction("reboot", R.string.power_restart, ElIcons.Restart, R.string.power_restart_ask, R.string.power_restart_explain),
	PowerAction("poweroff", R.string.power_shut_down, ElIcons.Power, R.string.power_shut_down_ask, R.string.power_shut_down_explain),
)

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PowerSection(t: TabScope, control: ControlInfo) {
	val prefs = LocalPrefs.current
	var confirm by remember { mutableStateOf<PowerAction?>(null) }
	var denied by remember { mutableStateOf<PowerAction?>(null) }
	val caps = control.power
	val shown = POWER.filter {
		when (it.key) {
			"lock" -> caps.lock
			"suspend" -> caps.suspend == "yes" || caps.suspend == "challenge"
			"reboot" -> caps.reboot == "yes" || caps.reboot == "challenge"
			else -> caps.poweroff == "yes" || caps.poweroff == "challenge"
		}
	}
	if (shown.isEmpty()) return
	Section(stringResource(R.string.control_power), ElIcons.Power, accent = Metric.colors.hot) {
		FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp), maxItemsInEachRow = 2) {
			for (a in shown) {
				val needsAuth = when (a.key) {
					"suspend" -> caps.suspend == "challenge"
					"reboot" -> caps.reboot == "challenge"
					"poweroff" -> caps.poweroff == "challenge"
					else -> false
				}
				val danger = a.key == "poweroff" || a.key == "reboot"
				Column(
					Modifier.weight(1f).heightIn(min = 112.dp).clip(RoundedCornerShape(18.dp))
						.background(if (danger) Metric.colors.hot.copy(alpha = 0.12f) else MaterialTheme.colorScheme.surfaceContainerHigh)
						.combinedClickableCompat {
							when {
								needsAuth -> denied = a
								prefs.confirmActions -> confirm = a
								else -> t.vm.power(a.key)
							}
						}
						.padding(vertical = 16.dp),
					horizontalAlignment = Alignment.CenterHorizontally,
					verticalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterVertically),
				) {
					Icon(a.icon, null, tint = if (danger) Metric.colors.hot else MaterialTheme.colorScheme.onSurface)
					Text(stringResource(a.label), style = MaterialTheme.typography.labelLarge)
					if (needsAuth) Text(stringResource(R.string.power_needs_permission), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
		}
	}
	confirm?.let { a ->
		GlassDialog(
			onDismiss = { confirm = null },
			title = stringResource(a.ask),
			icon = a.icon,
			text = { Text(stringResource(a.explain)) },
			confirm = { TextButton(onClick = { confirm = null; t.vm.power(a.key) }) { Text(stringResource(a.label)) } },
			dismiss = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
		)
	}
	denied?.let { a ->
		GlassDialog(
			onDismiss = { denied = null },
			title = stringResource(R.string.power_needs_permission_title, stringResource(a.label)),
			icon = ElIcons.Lock,
			text = {
				Text(
					stringResource(R.string.power_needs_permission_body),
				)
			},
			confirm = { TextButton(onClick = { denied = null; t.vm.power(a.key) }) { Text(stringResource(R.string.power_try_anyway)) } },
			dismiss = { TextButton(onClick = { denied = null; t.openHelp(null) }) { Text(stringResource(R.string.help)) } },
		)
	}
}
