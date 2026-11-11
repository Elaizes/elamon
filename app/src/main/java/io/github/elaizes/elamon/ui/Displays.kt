package io.github.elaizes.elamon.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.data.Display
import kotlin.math.max
import kotlin.math.min

/**
 * The computer's monitors, as a section of the control panel. Closed (as it starts) it's one line
 * that counts them; open, a row for each, whose details open in turn, then how they're arranged
 * and the free ports. Only information, so it shows whether control is on or not.
 */
@Composable
fun DisplaysSection(state: MachineState) {
	val s = state.sample ?: return
	val settings = Settings.get(LocalContext.current)
	val open by settings.displaysOpen.collectAsStateWithLifecycle()
	// The monitors whose details are open; kept here rather than in each row, so closing the section doesn't close them.
	var openMonitors by rememberSaveable { mutableStateOf(emptyList<String>()) }
	val connected = s.displays.filter { it.connected }
	val free = s.displays.filter { !it.connected }
	val gpuNames = state.info?.gpus.orEmpty().associate { it.id to shortGpuName(it.name) }
	val active = connected.filter { it.active && it.mode != null && it.position != null }
	val res = resources()
	val title = stringResource(R.string.displays_title)
	val summary = displaysSummary(s.displays).map { it.text(res) }
	val animate = LocalPrefs.current.animations

	Surface(shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
		Column {
			ExpanderRow(
				open,
				onToggle = { settings.setDisplaysOpen(!open) },
				padding = PaddingValues(horizontal = Space.xl, vertical = Space.m),
				description = (listOf(title) + summary).joinToString(stringResource(R.string.a11y_list_separator)),
			) {
				IconBadge(ElIcons.Monitor, MaterialTheme.colorScheme.primary)
				Spacer(Modifier.width(Space.m))
				// Said as the row's description instead.
				Column(Modifier.weight(1f).clearAndSetSemantics {}) {
					Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
					Text(summary.joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			Unfold(open, animate) {
				Column(Modifier.padding(bottom = Space.xs)) {
					for (d in connected) key(d.key) {
						val id = d.key
						GroupDivider()
						MonitorRow(d, gpuNames[d.gpu], open = id in openMonitors, animate = animate) {
							openMonitors = if (id in openMonitors) openMonitors - id else openMonitors + id
						}
					}
					if (connected.isEmpty()) {
						GroupDivider()
						Text(
							stringResource(R.string.displays_none_body),
							style = MaterialTheme.typography.bodyMedium,
							color = MaterialTheme.colorScheme.onSurfaceVariant,
							modifier = Modifier.padding(horizontal = Space.xl, vertical = Space.m),
						)
					}
					if (active.size > 1 || free.isNotEmpty()) {
						GroupDivider()
						Column(Modifier.padding(horizontal = Space.xl, vertical = Space.m), verticalArrangement = Arrangement.spacedBy(Space.s)) {
							if (active.size > 1) {
								Text(stringResource(R.string.displays_arrangement), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
								DisplayLayout(active)
							}
							if (free.isNotEmpty()) {
								Text(
									stringResource(R.string.displays_free_ports, free.joinToString(", ") { it.connector }),
									style = MaterialTheme.typography.bodySmall,
									color = MaterialTheme.colorScheme.onSurfaceVariant,
								)
							}
						}
					}
				}
			}
		}
	}
}

/** Which monitor a row is, from one sample to the next. */
private val Display.key: String get() = "$gpu/$connector"

/** A connected monitor: its name, current mode and state; touched, its details open under it. */
@Composable
private fun MonitorRow(d: Display, gpuName: String?, open: Boolean, animate: Boolean, onToggle: () -> Unit) {
	val (status, color) = displayStatus(d)
	Column {
		ExpanderRow(open, onToggle, padding = PaddingValues(horizontal = Space.xl, vertical = Space.s)) {
			// As wide as the header's badge, so the names line up under its title.
			Box(Modifier.width(Sizes.badge), contentAlignment = Alignment.Center) {
				Icon(if (d.internal) ElIcons.Laptop else ElIcons.Monitor, null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(Sizes.iconL))
			}
			Spacer(Modifier.width(Space.m))
			Column(Modifier.weight(1f)) {
				Text(displayName(d), style = MaterialTheme.typography.bodyLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
				d.mode?.let { Text(modeText(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
			}
			Spacer(Modifier.width(Space.s))
			Pill(status, color, dot = true)
			Spacer(Modifier.width(Space.xs))
		}
		Unfold(open, animate) {
			Column(
				Modifier.padding(start = Space.xl + Sizes.badge + Space.m, end = Space.xl, bottom = Space.m),
				verticalArrangement = Arrangement.spacedBy(Space.xsPlus),
			) {
				StatRow(stringResource(R.string.displays_port), d.connector + (gpuName?.let { " · $it" } ?: ""))
				if (d.widthMm != null && d.heightMm != null) {
					StatRow(stringResource(R.string.displays_size), "${Fmt.inches(d.widthMm, d.heightMm)} · ${Fmt.centimetres(d.widthMm, d.heightMm)}")
				}
				d.preferred?.let { StatRow(stringResource(R.string.displays_native_mode), modeText(it)) }
				d.maxRefresh?.let { StatRow(stringResource(R.string.displays_highest_refresh), Fmt.hz(it)) }
				d.position?.takeIf { d.mode != null }?.let { StatRow(stringResource(R.string.displays_position), "${it[0]}, ${it[1]}") }
				d.year?.let { StatRow(stringResource(R.string.displays_made_in), it.toString()) }
			}
		}
	}
}

@Composable
private fun displayStatus(d: Display): Pair<String, Color> = when {
	d.active -> stringResource(R.string.displays_on) to Metric.colors.good
	d.mode != null || d.dpms == "Off" -> stringResource(R.string.displays_off) to MaterialTheme.colorScheme.secondary
	else -> stringResource(R.string.displays_not_in_use) to MaterialTheme.colorScheme.secondary
}

/**
 * A row that opens and closes what's under it, ending in a chevron: one button to a screen reader,
 * which hears whether it's open and can open or close it. [description], when given, is what it
 * says for the row; the caller hides the text it stands for.
 */
@Composable
private fun ExpanderRow(
	open: Boolean,
	onToggle: () -> Unit,
	padding: PaddingValues,
	description: String? = null,
	content: @Composable RowScope.() -> Unit,
) {
	val state = stringResource(if (open) R.string.a11y_expanded else R.string.a11y_collapsed)
	Row(
		Modifier
			.fillMaxWidth()
			.clickable(role = Role.Button, onClick = onToggle)
			.semantics {
				stateDescription = state
				if (description != null) contentDescription = description
				if (open) collapse { onToggle(); true } else expand { onToggle(); true }
			}
			.heightIn(min = Sizes.touchTarget)
			.padding(padding),
		verticalAlignment = Alignment.CenterVertically,
	) {
		content()
		Icon(if (open) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
	}
}

/** What opens downward and closes back up; at once with animations off. */
@Composable
private fun Unfold(visible: Boolean, animate: Boolean, content: @Composable () -> Unit) {
	val unfold = if (animate) Motion.UNFOLD else 0
	val fold = if (animate) Motion.FOLD else 0
	AnimatedVisibility(
		visible,
		enter = expandVertically(tween(unfold)) + fadeIn(tween(unfold)),
		exit = shrinkVertically(tween(fold)) + fadeOut(tween(fold)),
	) {
		content()
	}
}

/** The monitors as the compositor places them, scaled to fit. */
@Composable
private fun DisplayLayout(displays: List<Display>) {
	val measurer = rememberTextMeasurer()
	val fill = MaterialTheme.colorScheme.primaryContainer
	val edge = MaterialTheme.colorScheme.primary
	val labelStyle = ChartStyle.monitorText.copy(color = MaterialTheme.colorScheme.onPrimaryContainer)
	val minX = displays.minOf { it.position!![0] }
	val minY = displays.minOf { it.position!![1] }
	val maxX = displays.maxOf { it.position!![0] + it.mode!!.width }
	val maxY = displays.maxOf { it.position!![1] + it.mode!!.height }
	val spanW = (maxX - minX).coerceAtLeast(1)
	val spanH = (maxY - minY).coerceAtLeast(1)
	Canvas(Modifier.fillMaxWidth().height(ChartStyle.displays)) {
		val scale = min(size.width / spanW, size.height / spanH)
		val ox = (size.width - spanW * scale) / 2
		val oy = (size.height - spanH * scale) / 2
		val gap = ChartStyle.monitorGap.toPx()
		for (d in displays) {
			val p = d.position!!
			val m = d.mode!!
			val tl = Offset(ox + (p[0] - minX) * scale + gap, oy + (p[1] - minY) * scale + gap)
			val sz = Size(max(m.width * scale - 2 * gap, 1f), max(m.height * scale - 2 * gap, 1f))
			val r = CornerRadius(Radius.m.toPx())
			drawRoundRect(Brush.linearGradient(listOf(fill, fill.copy(alpha = Alpha.FADE_END)), tl, Offset(tl.x + sz.width, tl.y + sz.height)), tl, sz, r)
			drawRoundRect(edge, tl, sz, r, style = Stroke(ChartStyle.outline.toPx()))
			val label = measurer.measure(d.connector, labelStyle)
			drawText(label, topLeft = Offset(tl.x + (sz.width - label.size.width) / 2, tl.y + (sz.height - label.size.height) / 2))
		}
	}
}
