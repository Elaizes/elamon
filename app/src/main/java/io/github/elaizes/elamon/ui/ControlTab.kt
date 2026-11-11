package io.github.elaizes.elamon.ui

import android.content.Intent
import android.content.res.Resources
import android.graphics.Bitmap
import android.net.Uri
import androidx.annotation.StringRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
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
import androidx.compose.runtime.saveable.rememberSaveable
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import io.github.elaizes.elamon.Can
import io.github.elaizes.elamon.Conn
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.PhoneRole
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.ActionInfo
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.ControlInfo
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.Listener
import io.github.elaizes.elamon.data.Player
import io.github.elaizes.elamon.data.LinkDef
import io.github.elaizes.elamon.data.LinkStatus
import io.github.elaizes.elamon.data.UnitInfo
import io.github.elaizes.elamon.data.linkUrl
import io.github.elaizes.elamon.data.takeCodePoints
import io.github.elaizes.elamon.data.takeUtf8Bytes

@Composable
fun ControlTab(t: TabScope) {
	val vm = t.vm
	val control by vm.control.collectAsStateWithLifecycle()
	val controlError by vm.controlError.collectAsStateWithLifecycle()
	val s = t.state.sample ?: return
	// What is open is kept by the link's id, so it outlives a rotation; the link itself is
	// looked up in each sample, and one gone from the computer closes what was open for it.
	var openId by rememberSaveable { mutableStateOf<String?>(null) }
	var editId by rememberSaveable { mutableStateOf<String?>(null) }
	var adding by rememberSaveable { mutableStateOf(false) }
	var localOnlyId by rememberSaveable { mutableStateOf<String?>(null) }
	fun linkById(id: String?) = id?.let { i -> s.links.find { it.id == i } }
	val open = linkById(openId)
	val edit = linkById(editId)
	val localOnly = linkById(localOnlyId)
	val ctx = LocalContext.current
	val perms = t.perms
	val guard = LocalGuard.current
	val res = resources()

	LaunchedEffect(Unit) { vm.refreshControl() }
	// Players ride the stream only while they can be shown here.
	DisposableEffect(perms.mediaRead) {
		vm.setMedia(perms.mediaRead)
		onDispose { vm.setMedia(false) }
	}

	fun launch(link: LinkStatus) {
		val host = t.state.host ?: return
		runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(linkUrl(link.url, host)))) }
	}

	val enabled = control?.enabled == true
	LazyColumn(
		Modifier.fillMaxSize(),
		state = t.list,
		contentPadding = listPadding(),
		verticalArrangement = Arrangement.spacedBy(Space.m),
	) {
		item(key = "header") { t.header() }
		controlError?.let { e ->
			item(key = "error") { ControlError(e.text(resources())) { vm.refreshControl() } }
		}
		// What this phone's role leaves out is left out, with a word on how to change it.
		when {
			perms.known == PhoneRole.Glance -> item(key = "role") { RoleNotice(R.string.control_glance_title, R.string.control_glance_body, vm.clientId, PhoneRole.Monitor) }
			perms.known == PhoneRole.Monitor -> item(key = "role") { RoleNotice(R.string.control_monitor_title, R.string.control_monitor_body, vm.clientId, PhoneRole.Operate) }
			control != null && !enabled && perms.mayAct -> item(key = "off") { ControlOff { t.openHelp(null) } }
		}
		if (perms.links) {
			item(key = "links") {
				LinksSection(
					links = s.links,
					onOpen = { if (it.localOnly) localOnlyId = it.id else launch(it) },
					onMenu = { openId = it.id },
					onAdd = if (perms.linksEdit) ({ adding = true }) else null,
				)
			}
		}
		if (perms.displays) {
			item(key = "displays") { DisplaysSection(t.state) }
		}
		val media = s.media
		if (perms.mediaRead && media != null && (media.players.isNotEmpty() || media.volume != null)) {
			item(key = "media") { MediaSection(t, media.players, media.volume) }
		}
		control?.profiles?.takeIf { perms.profile && it.available.isNotEmpty() }?.let { p ->
			item(key = "profile") {
				ProfileSection(s.power.ppdProfile ?: p.active, p.available) { profile, label ->
					val ask = Ask(Guarded.Profile, res.getString(R.string.unlock_profile, t.name, label), res.getString(R.string.control_profile_title, label), res.getString(R.string.control_switch), confirmable = false)
					guard.run(ask) { vm.setProfile(profile) }
				}
			}
		}
		control?.actions?.takeIf { perms.actions && it.isNotEmpty() }?.let { actions ->
			item(key = "actions") { ActionsSection(t, actions) }
		}
		control?.takeIf { perms.power }?.let { c ->
			item(key = "power") { PowerSection(t, c) }
		}
		if (perms.log) {
			item(key = "activity") {
				ListGroup {
					ListRow(
						stringResource(R.string.activity_title),
						summary = stringResource(R.string.activity_summary),
						icon = ElIcons.Clock,
						accent = MaterialTheme.colorScheme.secondary,
						onClick = { t.openDetail(ACTIVITY_PAGE) },
					)
				}
			}
		}
	}

	open?.let { link ->
		LinkSheet(
			t, link,
			onDismiss = { openId = null },
			onOpen = { openId = null; if (link.localOnly) localOnlyId = link.id else launch(link) },
			onEdit = { openId = null; editId = link.id },
		)
	}
	if (perms.linksEdit && (adding || edit != null)) {
		LinkEditor(t, edit, onDismiss = { adding = false; editId = null })
	}
	localOnly?.let { link ->
		GlassDialog(
			onDismiss = { localOnlyId = null },
			title = stringResource(R.string.control_local_only_title),
			icon = Icons.Default.Warning,
			text = {
				Text(stringResource(R.string.control_local_only_body, link.name))
			},
			confirm = { TextButton(onClick = { localOnlyId = null; launch(link) }) { Text(stringResource(R.string.control_open_anyway)) } },
			dismiss = { TextButton(onClick = { localOnlyId = null }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

/** A new link's address before its port: the computer's own address, over plain HTTP. */
private const val LINK_URL_START = "http://{host}:"

/** Tiles inside a card: profiles, actions, power. */
private val InnerTileShape = RoundedCornerShape(Radius.lPlus)

/** The shortest address worth saving, like "a:80". */
private const val URL_MIN = 4

/** A track's length and position come in microseconds. */
private const val US_PER_SECOND = 1e6

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

/**
 * This phone's role leaves out most of the panel: what it may do, and the command that lets it do
 * more, [next] being the role up, with this phone's id when the pairing told it.
 */
@Composable
private fun RoleNotice(@StringRes title: Int, @StringRes body: Int, clientId: String?, next: PhoneRole) {
	Section(stringResource(title), ElIcons.Eye, accent = MaterialTheme.colorScheme.secondary) {
		Text(stringResource(body), style = MaterialTheme.typography.bodyMedium)
		CodeBlock("elamond clients role ${clientId ?: "ID"} ${next.key}")
		Text(stringResource(R.string.control_role_where), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
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

// ---- links -----------------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun LinksSection(links: List<LinkStatus>, onOpen: (LinkStatus) -> Unit, onMenu: (LinkStatus) -> Unit, onAdd: (() -> Unit)?) {
	Column(verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
		// As tall with the add button as without it, so the title doesn't move when the role changes.
		Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(start = Space.xs).heightIn(min = Sizes.touchTarget)) {
			Text(stringResource(R.string.control_links), style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
			if (onAdd != null) {
				FilledIconButton(onClick = onAdd, colors = IconButtonDefaults.filledIconButtonColors(containerColor = MaterialTheme.colorScheme.primaryContainer, contentColor = MaterialTheme.colorScheme.onPrimaryContainer)) {
					Icon(Icons.Default.Add, stringResource(R.string.control_add_link))
				}
			}
		}
		if (links.isEmpty()) {
			Surface(shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
				Column(Modifier.padding(Space.xl), verticalArrangement = Arrangement.spacedBy(Space.xsPlus)) {
					Text(stringResource(R.string.control_links_empty), style = MaterialTheme.typography.titleMedium)
					Text(
						stringResource(if (onAdd != null) R.string.control_links_empty_body else R.string.control_links_empty_watch),
						style = MaterialTheme.typography.bodyMedium,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
					)
				}
			}
		} else {
			FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sPlus), verticalArrangement = Arrangement.spacedBy(Space.sPlus), maxItemsInEachRow = 2) {
				for (link in links) LinkTile(link, Modifier.weight(1f), onOpen = { onOpen(link) }, onMenu = { onMenu(link) })
				if (links.size % 2 == 1) Spacer(Modifier.weight(1f))
			}
		}
	}
}

@Composable
private fun linkState(link: LinkStatus): Pair<String, Color> {
	val c = Metric.colors
	val (label, color) = when {
		link.unitState == "failed" -> R.string.link_failed to c.hot
		link.unitState == "missing" -> R.string.link_no_unit to c.warn
		link.unitState == "activating" || link.unitState == "reloading" -> R.string.link_starting to c.warn
		link.unitState == "deactivating" -> R.string.link_stopping to c.warn
		link.up == true && link.localOnly -> R.string.link_local_only to c.warn
		link.up == true -> R.string.link_running to c.good
		link.unitState == "active" -> R.string.link_running to c.good
		link.up == false -> R.string.link_stopped to MaterialTheme.colorScheme.outline
		else -> R.string.link_unknown to MaterialTheme.colorScheme.outline
	}
	return stringResource(label) to color
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
private fun LinkTile(link: LinkStatus, modifier: Modifier, onOpen: () -> Unit, onMenu: () -> Unit) {
	val haptics = LocalHapticFeedback.current
	val prefs = LocalPrefs.current
	val (label, color) = linkState(link)
	val menu = stringResource(R.string.control_link_menu)
	Surface(modifier = modifier, shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Box {
			Column(
				Modifier
					.fillMaxWidth()
					.combinedClickable(
						onClickLabel = stringResource(R.string.control_open),
						role = Role.Button,
						onLongClickLabel = menu,
						onLongClick = {
							if (prefs.haptics) haptics.performHapticFeedback(HapticFeedbackType.LongPress)
							onMenu()
						},
						onClick = onOpen,
					)
					.padding(Space.mPlus),
				verticalArrangement = Arrangement.spacedBy(Space.sPlus),
			) {
				LetterAvatar(link.name, Sizes.avatar)
				Column {
					Text(link.name, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
					Text(link.port?.let { ":$it" } ?: link.url, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
				}
				Pill(label, color, dot = true)
			}
			// Drawn where a small button beside the avatar would be, but a full touch target, over the tile's padding.
			IconButton(
				onClick = onMenu,
				modifier = Modifier.align(Alignment.TopEnd).padding(
					top = Space.mPlus + (Sizes.avatar - Sizes.touchTarget) / 2,
					end = Space.mPlus + (Sizes.tileMenu - Sizes.touchTarget) / 2,
				),
			) {
				Icon(Icons.Default.MoreVert, menu, modifier = Modifier.size(Sizes.iconM))
			}
		}
	}
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LinkSheet(t: TabScope, link: LinkStatus, onDismiss: () -> Unit, onOpen: () -> Unit, onEdit: () -> Unit) {
	val perms = t.perms
	val guard = LocalGuard.current
	val res = resources()
	var confirmRemove by rememberSaveable { mutableStateOf(false) }
	ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(start = Space.xxl, end = Space.xxl, bottom = Space.xxxl), verticalArrangement = Arrangement.spacedBy(Space.m)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				LetterAvatar(link.name, Sizes.avatarSheet)
				Spacer(Modifier.width(Space.mPlus))
				Column(Modifier.weight(1f)) {
					Text(link.name, style = MaterialTheme.typography.headlineSmall)
					Text(link.url, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
				}
				val (label, color) = linkState(link)
				Pill(label, color, dot = true)
			}
			link.unit?.let { StatRow(stringResource(R.string.control_unit), "$it · ${link.scope}") }
			link.unitState?.let { StatRow(stringResource(R.string.control_unit_state), it) }
			link.port?.let { StatRow(stringResource(R.string.control_port), if (link.localOnly) stringResource(R.string.control_port_computer_only, it) else it.toString()) }
			Button(onClick = onOpen, modifier = Modifier.fillMaxWidth()) {
				Icon(ElIcons.Globe, null, Modifier.size(Sizes.iconM))
				Spacer(Modifier.width(Space.s))
				Text(stringResource(R.string.control_open))
			}
			if (link.unit != null) {
				if (perms.units) {
					Row(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
						val running = link.unitState == "active" || link.unitState == "activating"
						FilledTonalButton(
							onClick = {
								// Starting and stopping weren't confirmed before; unlocking may be asked for.
								val ask = Ask(
									Guarded.Units,
									prompt = res.getString(if (running) R.string.unlock_stop else R.string.unlock_start, link.name, t.name),
									question = res.getString(if (running) R.string.control_stop_title else R.string.control_start_title, link.name),
									button = res.getString(if (running) R.string.control_stop else R.string.control_start),
									confirmable = false,
								)
								guard.run(ask) { t.vm.unit(link.id, if (running) "stop" else "start") }
							},
							modifier = Modifier.weight(1f),
						) {
							Icon(if (running) ElIcons.Pause else ElIcons.Play, null, Modifier.size(Sizes.iconS))
							Spacer(Modifier.width(Space.xsPlus))
							Text(stringResource(if (running) R.string.control_stop else R.string.control_start))
						}
						FilledTonalButton(
							onClick = {
								val ask = Ask(
									Guarded.Units,
									prompt = res.getString(R.string.unlock_restart_unit, link.name, t.name),
									question = res.getString(R.string.control_restart_title, link.name),
									button = res.getString(R.string.control_restart),
									body = res.getString(R.string.control_restart_body),
								)
								guard.run(ask, afterAsking = onDismiss) { t.vm.unit(link.id, "restart") }
							},
							modifier = Modifier.weight(1f),
						) {
							Icon(ElIcons.Restart, null, Modifier.size(Sizes.iconS))
							Spacer(Modifier.width(Space.xsPlus))
							Text(stringResource(R.string.control_restart))
						}
					}
				} else if (perms.roleAllows(Can.Units)) {
					// The role would let this phone, but control is off.
					Text(stringResource(R.string.control_needs_control), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			// A phone that may only look at links isn't told where they're changed.
			if (perms.linksEdit && link.editable) {
				Row(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
					TextButton(onClick = onEdit) {
						Icon(Icons.Default.Edit, null, Modifier.size(Sizes.iconS))
						Spacer(Modifier.width(Space.xsPlus))
						Text(stringResource(R.string.control_edit))
					}
					TextButton(onClick = { confirmRemove = true }) {
						Icon(Icons.Default.Delete, null, Modifier.size(Sizes.iconS), tint = Metric.colors.hot)
						Spacer(Modifier.width(Space.xsPlus))
						Text(stringResource(R.string.control_remove), color = Metric.colors.hot)
					}
				}
			} else if (perms.linksEdit) {
				Text(stringResource(R.string.control_set_on_computer), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
	if (confirmRemove) {
		GlassDialog(
			onDismiss = { confirmRemove = false },
			title = stringResource(R.string.control_remove_title, link.name),
			text = { Text(stringResource(R.string.control_remove_body)) },
			confirm = {
				TextButton(onClick = {
					t.vm.deleteLink(link.id)
					confirmRemove = false
					onDismiss()
				}) { Text(stringResource(R.string.control_remove)) }
			},
			dismiss = { TextButton(onClick = { confirmRemove = false }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
private fun LinkEditor(t: TabScope, existing: LinkStatus?, onDismiss: () -> Unit) {
	// What has been typed outlives a rotation; what the computer suggests is asked for again.
	var name by rememberSaveable { mutableStateOf(existing?.name.orEmpty()) }
	var url by rememberSaveable { mutableStateOf(existing?.url ?: LINK_URL_START) }
	var unit by rememberSaveable { mutableStateOf(existing?.unit.orEmpty()) }
	var scope by rememberSaveable { mutableStateOf(existing?.scope ?: "user") }
	var detected by remember { mutableStateOf<List<Listener>>(emptyList()) }
	var units by remember { mutableStateOf<List<UnitInfo>>(emptyList()) }
	val taken = t.state.sample?.links.orEmpty().mapNotNull { it.port }.toSet()

	LaunchedEffect(Unit) { detected = t.vm.detected() }
	LaunchedEffect(scope) { units = t.vm.units(scope) }

	ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier.padding(start = Space.xxl, end = Space.xxl, bottom = Space.xxxl).verticalScroll(rememberScrollState()),
			verticalArrangement = Arrangement.spacedBy(Space.m),
		) {
			Text(if (existing == null) stringResource(R.string.control_add_title) else stringResource(R.string.control_edit_title, existing.name), style = MaterialTheme.typography.headlineSmall)
			val suggestions = detected.filter { it.port !in taken && it.port < Limits.EPHEMERAL_PORT_MIN && it.process != "elamond" }
			if (existing == null && suggestions.isNotEmpty()) {
				Text(stringResource(R.string.control_listening), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
				Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
					for (l in suggestions) {
						FilterChip(
							selected = url.endsWith(":${l.port}") || url.contains(":${l.port}/"),
							onClick = {
								url = "$LINK_URL_START${l.port}"
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
			OutlinedTextField(value = name, onValueChange = { name = it.takeCodePoints(Limits.LINK_NAME_MAX) }, label = { Text(stringResource(R.string.control_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
			OutlinedTextField(
				value = url,
				onValueChange = { url = it.takeUtf8Bytes(Limits.LINK_URL_MAX) },
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
			OutlinedTextField(value = unit, onValueChange = { unit = it.trim().takeUtf8Bytes(Limits.UNIT_MAX) }, label = { Text(stringResource(R.string.control_unit)) }, placeholder = { Text("name.service") }, singleLine = true, modifier = Modifier.fillMaxWidth())
			val matches = units.filter { unit.isBlank() || it.name.contains(unit, true) || it.description.contains(unit, true) }.take(Limits.UNIT_SUGGESTIONS)
			if (matches.isNotEmpty() && matches.none { it.name == unit }) {
				FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
					for (u in matches) FilterChip(selected = false, onClick = { unit = u.name }, label = { Text(u.name.removeSuffix(".service")) })
				}
			}
			Row(horizontalArrangement = Arrangement.spacedBy(Space.s), modifier = Modifier.padding(top = Space.xs)) {
				TextButton(onClick = onDismiss, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.cancel)) }
				Button(
					enabled = name.isNotBlank() && url.length >= URL_MIN,
					onClick = {
						t.vm.saveLink(LinkDef(existing?.id, name.trim(), url.trim(), unit.ifBlank { null }, scope))
						onDismiss()
					},
					modifier = Modifier.weight(1f),
				) { Text(stringResource(R.string.control_save)) }
			}
		}
	}
}

// ---- media ------------------------------------------------------------------------------

/** What plays, and the volume; a phone not allowed to change them sees them without the buttons. */
@Composable
private fun MediaSection(t: TabScope, players: List<Player>, volume: io.github.elaizes.elamon.data.Volume?) {
	var chosen by remember { mutableStateOf<String?>(null) }
	val player = players.find { it.id == chosen } ?: players.firstOrNull()
	Surface(shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
		Column(Modifier.padding(Space.lPlus), verticalArrangement = Arrangement.spacedBy(Space.mPlus)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				IconBadge(ElIcons.Music, MaterialTheme.colorScheme.tertiary)
				Spacer(Modifier.width(Space.m))
				Text(player?.name ?: stringResource(R.string.control_media), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
			}
			if (players.size > 1) {
				Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
					for (p in players) FilterChip(selected = p.id == player?.id, onClick = { chosen = p.id }, label = { Text(p.name) })
				}
			}
			if (player != null) NowPlaying(t, player)
			if (volume != null) VolumeRow(t, volume)
		}
	}
}


/** Playback and the volume are sent as they are, unless the phone is to be unlocked for media. */
private fun mediaAsk(res: Resources, name: String) =
	Ask(Guarded.Media, res.getString(R.string.unlock_media, name), res.getString(R.string.control_media_title, name), res.getString(R.string.control_continue), confirmable = false)

private fun volumeAsk(res: Resources, name: String) =
	Ask(Guarded.Media, res.getString(R.string.unlock_volume, name), res.getString(R.string.control_volume_title, name), res.getString(R.string.control_continue), confirmable = false)

@Composable
private fun NowPlaying(t: TabScope, p: Player) {
	val guard = LocalGuard.current
	val res = resources()
	fun send(op: String, value: Double? = null) = guard.run(mediaAsk(res, t.name)) { t.vm.media(p.id, op, value) }
	val artPx = with(LocalDensity.current) { Sizes.art.roundToPx() }
	var art by remember(p.id, p.title) { mutableStateOf<Bitmap?>(null) }
	LaunchedEffect(p.id, p.title, p.hasArt) { art = if (p.hasArt) t.vm.art(p.id, artPx) else null }
	val playing = p.status == "Playing"
	// The service reports once a sample, maybe seconds apart; in between the position moves on by itself.
	val moving = playing && t.state.conn == Conn.Live
	var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
	LaunchedEffect(moving) {
		while (moving) {
			now = System.currentTimeMillis()
			delay(Limits.MEDIA_TICK_MS)
		}
	}
	Row(verticalAlignment = Alignment.CenterVertically) {
		Box(
			Modifier.size(Sizes.art).clip(RoundedCornerShape(Radius.l)).background(
				Brush.linearGradient(listOf(MaterialTheme.colorScheme.tertiaryContainer, MaterialTheme.colorScheme.primaryContainer))
			),
			contentAlignment = Alignment.Center,
		) {
			val a = art
			if (a != null) Image(a.asImageBitmap(), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
			else Icon(ElIcons.Music, null, tint = MaterialTheme.colorScheme.onTertiaryContainer, modifier = Modifier.size(Sizes.iconArt))
		}
		Spacer(Modifier.width(Space.mPlus))
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
					dragging?.let { f -> send("seek", f * length / US_PER_SECOND) }
					dragging = null
				},
				enabled = p.canSeek && t.perms.media,
			)
			Row {
				Text(Fmt.clock(pos), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				Spacer(Modifier.weight(1f))
				Text(Fmt.clock(length), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
	if (t.perms.media) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.CenterVertically) {
		IconButton(onClick = { send("previous") }, enabled = p.canPrevious) { Icon(ElIcons.Previous, stringResource(R.string.control_previous)) }
		FilledIconButton(onClick = { send("play_pause") }, enabled = p.canPlay || p.canPause, modifier = Modifier.size(Sizes.playButton)) {
			Icon(if (playing) ElIcons.Pause else ElIcons.Play, stringResource(if (playing) R.string.control_pause else R.string.control_play), modifier = Modifier.size(Sizes.iconXl))
		}
		IconButton(onClick = { send("next") }, enabled = p.canNext) { Icon(ElIcons.Next, stringResource(R.string.control_next)) }
	}
}

@Composable
private fun VolumeRow(t: TabScope, v: io.github.elaizes.elamon.data.Volume) {
	val guard = LocalGuard.current
	val res = resources()
	fun send(level: Float? = null, muted: Boolean? = null) = guard.run(volumeAsk(res, t.name)) { t.vm.volume(level, muted) }
	var dragging by remember { mutableStateOf<Float?>(null) }
	val canSet = t.perms.volume
	Row(verticalAlignment = Alignment.CenterVertically) {
		IconButton(onClick = { send(muted = !v.muted) }, enabled = canSet) {
			Icon(if (v.muted) ElIcons.Mute else ElIcons.Volume, stringResource(if (v.muted) R.string.control_unmute else R.string.control_mute), tint = if (v.muted) Metric.colors.hot else MaterialTheme.colorScheme.onSurface)
		}
		Slider(
			value = dragging ?: v.level.coerceIn(0f, 1f),
			onValueChange = { dragging = it },
			onValueChangeFinished = {
				dragging?.let { send(level = it) }
				dragging = null
			},
			enabled = canSet,
			modifier = Modifier.weight(1f),
		)
		Text(Fmt.percent((dragging ?: v.level) * Thresholds.PERCENT), style = MaterialTheme.typography.labelLarge, modifier = Modifier.width(Sizes.volumeLabel).padding(start = Space.xsPlus))
	}
}

// ---- power profile, actions, power --------------------------------------------------------

/** The profiles side by side; [onPick] gets one with its name as shown. */
@Composable
private fun ProfileSection(active: String, available: List<String>, onPick: (String, String) -> Unit) {
	Section(stringResource(R.string.control_power_profile), ElIcons.Bolt, accent = Metric.colors.battery) {
		Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min).selectableGroup(), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
			for (p in available) {
				val on = p == active
				val label = profileName(p)
				val bg by animateColorAsState(if (on) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerHigh, label = "profile")
				val fg = if (on) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant
				Column(
					Modifier.weight(1f).fillMaxHeight().clip(InnerTileShape).background(bg)
						.selectable(selected = on, role = Role.Button, onClick = { onPick(p, label) })
						.padding(vertical = Space.mPlus, horizontal = Space.xsPlus),
					horizontalAlignment = Alignment.CenterHorizontally,
					verticalArrangement = Arrangement.spacedBy(Space.xsPlus),
				) {
					Icon(profileIcon(p), null, tint = fg)
					Text(label, style = MaterialTheme.typography.labelLarge, color = fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ActionsSection(t: TabScope, actions: List<ActionInfo>) {
	val state by t.vm.actions.collectAsStateWithLifecycle()
	val guard = LocalGuard.current
	val res = resources()
	fun run(a: ActionInfo) {
		val ask = Ask(
			Guarded.Actions,
			prompt = res.getString(R.string.unlock_run, a.name, t.name),
			question = res.getString(R.string.control_run_title, a.name),
			button = res.getString(R.string.control_run),
			icon = ElIcons.Terminal,
			// The computer's settings say whether it's confirmed.
			confirmable = a.confirm,
		)
		guard.run(ask) { t.vm.runAction(a.id) }
	}
	Section(stringResource(R.string.control_actions), ElIcons.Terminal, accent = MaterialTheme.colorScheme.tertiary) {
		for (a in actions) {
			val last = state.results[a.id]
			Surface(shape = InnerTileShape, color = MaterialTheme.colorScheme.surfaceContainerHigh, modifier = Modifier.fillMaxWidth()) {
				Row(
					Modifier.clickable(role = Role.Button) { if (state.running == null) run(a) }
						.padding(horizontal = Space.l, vertical = Space.mPlus),
					verticalAlignment = Alignment.CenterVertically,
				) {
					Icon(ElIcons.Play, null, tint = MaterialTheme.colorScheme.tertiary, modifier = Modifier.size(Sizes.iconM))
					Spacer(Modifier.width(Space.m))
					Text(a.name, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
					when {
						state.running == a.id -> androidx.compose.material3.CircularProgressIndicator(Modifier.size(Sizes.iconM), strokeWidth = Sizes.spinnerStroke)
						// The last run's outcome; its output opens on a tap.
						last != null -> ActionOutcome(last, Modifier.clip(PillShape).clickable(role = Role.Button) { t.vm.showActionResult(a.id) })
					}
				}
			}
		}
	}
	val shown = state.shown?.let { id -> actions.find { it.id == id }?.let { a -> state.results[id]?.let { a to it } } }
	shown?.let { (a, r) ->
		ModalBottomSheet(onDismissRequest = { t.vm.showActionResult(null) }, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
			Column(Modifier.padding(start = Space.xxl, end = Space.xxl, bottom = Space.xxxl), verticalArrangement = Arrangement.spacedBy(Space.m)) {
				Row(verticalAlignment = Alignment.CenterVertically) {
					Text(a.name, style = MaterialTheme.typography.headlineSmall, modifier = Modifier.weight(1f))
					ActionOutcome(r)
				}
				Surface(shape = RoundedCornerShape(Radius.mPlus), color = MaterialTheme.colorScheme.surfaceContainerHighest, modifier = Modifier.fillMaxWidth().heightIn(max = Sizes.outputMax)) {
					SelectionContainer {
						Text(
							r.output.ifBlank { stringResource(R.string.control_no_output) },
							fontFamily = FontFamily.Monospace,
							style = MaterialTheme.typography.bodySmall,
							modifier = Modifier.verticalScroll(rememberScrollState()).padding(Space.m),
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

/** A power tile: what it sends, its label, the confirmation's question and why, and the unlock prompt's title with the computer's name. */
private data class PowerAction(val key: String, @StringRes val label: Int, val icon: ImageVector, @StringRes val ask: Int, @StringRes val explain: Int, @StringRes val unlock: Int)

private val POWER = listOf(
	PowerAction("lock", R.string.power_lock, ElIcons.Lock, R.string.power_lock_ask, R.string.power_lock_explain, R.string.unlock_lock),
	PowerAction("suspend", R.string.power_sleep, ElIcons.Moon, R.string.power_sleep_ask, R.string.power_sleep_explain, R.string.unlock_sleep),
	PowerAction("reboot", R.string.power_restart, ElIcons.Restart, R.string.power_restart_ask, R.string.power_restart_explain, R.string.unlock_restart),
	PowerAction("poweroff", R.string.power_shut_down, ElIcons.Power, R.string.power_shut_down_ask, R.string.power_shut_down_explain, R.string.unlock_shut_down),
)

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PowerSection(t: TabScope, control: ControlInfo) {
	val guard = LocalGuard.current
	val res = resources()
	// [confirmable] false where the dialog about permission has just asked.
	fun send(a: PowerAction, confirmable: Boolean = true) {
		val ask = Ask(
			Guarded.Power,
			prompt = res.getString(a.unlock, t.name),
			question = res.getString(a.ask),
			button = res.getString(a.label),
			body = res.getString(a.explain),
			icon = a.icon,
			confirmable = confirmable,
		)
		guard.run(ask) { t.vm.power(a.key) }
	}
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
		FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s), verticalArrangement = Arrangement.spacedBy(Space.s), maxItemsInEachRow = 2) {
			for (a in shown) {
				val needsAuth = when (a.key) {
					"suspend" -> caps.suspend == "challenge"
					"reboot" -> caps.reboot == "challenge"
					"poweroff" -> caps.poweroff == "challenge"
					else -> false
				}
				val danger = a.key == "poweroff" || a.key == "reboot"
				Column(
					Modifier.weight(1f).heightIn(min = Sizes.powerTileMin).clip(InnerTileShape)
						.background(if (danger) Metric.colors.hot.copy(alpha = Alpha.DANGER) else MaterialTheme.colorScheme.surfaceContainerHigh)
						.clickable(role = Role.Button) {
							if (needsAuth) denied = a else send(a)
						}
						.padding(vertical = Space.l),
					horizontalAlignment = Alignment.CenterHorizontally,
					verticalArrangement = Arrangement.spacedBy(Space.xsPlus, Alignment.CenterVertically),
				) {
					Icon(a.icon, null, tint = if (danger) Metric.colors.hot else MaterialTheme.colorScheme.onSurface)
					Text(stringResource(a.label), style = MaterialTheme.typography.labelLarge)
					if (needsAuth) Text(stringResource(R.string.power_needs_permission), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
		}
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
			confirm = { TextButton(onClick = { denied = null; send(a, confirmable = false) }) { Text(stringResource(R.string.power_try_anyway)) } },
			dismiss = { TextButton(onClick = { denied = null; t.openHelp(null) }) { Text(stringResource(R.string.help)) } },
		)
	}
}
