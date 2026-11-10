package ua.elamon.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import ua.elamon.MachineState
import ua.elamon.R
import ua.elamon.data.Display
import kotlin.math.max
import kotlin.math.min

/**
 * The computer's monitors, as a section of the control panel: what's connected, how it's arranged
 * and the free ports. Only information, so it shows whether control is on or not.
 */
@Composable
fun DisplaysSection(state: MachineState) {
	val s = state.sample ?: return
	val connected = s.displays.filter { it.connected }
	val free = s.displays.filter { !it.connected }
	val gpuNames = state.info?.gpus.orEmpty().associate { it.id to shortGpuName(it.name) }
	val active = connected.filter { it.active && it.mode != null && it.position != null }

	Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
		Text(stringResource(R.string.displays_title), style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(start = Space.xs))
		if (active.size > 1) {
			Section(stringResource(R.string.displays_arrangement), ElIcons.Overview) { DisplayLayout(active) }
		}
		for (d in connected) key(d.gpu + d.connector) { DisplayCard(d, gpuNames[d.gpu]) }
		if (connected.isEmpty()) {
			Section(stringResource(R.string.displays_none), ElIcons.Monitor) {
				Text(stringResource(R.string.displays_none_body), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
		if (free.isNotEmpty()) {
			Text(
				stringResource(R.string.displays_free_ports, free.joinToString(", ") { it.connector }),
				style = MaterialTheme.typography.bodySmall,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
				modifier = Modifier.fillMaxWidth(),
				textAlign = TextAlign.Center,
			)
		}
	}
}

@Composable
private fun DisplayCard(d: Display, gpuName: String?) {
	val c = Metric.colors
	val fallbackName = stringResource(if (d.internal) R.string.displays_built_in else R.string.displays_monitor)
	val name = listOfNotNull(d.vendor, d.model).joinToString(" ").ifEmpty { fallbackName }
	val (status, color) = when {
		d.active -> stringResource(R.string.displays_on) to c.good
		d.mode != null || d.dpms == "Off" -> stringResource(R.string.displays_off) to MaterialTheme.colorScheme.secondary
		else -> stringResource(R.string.displays_not_in_use) to MaterialTheme.colorScheme.secondary
	}
	Section(name, if (d.internal) ElIcons.Laptop else ElIcons.Monitor, trailing = { Pill(status, color, dot = true) }) {
		d.mode?.let { m ->
			Row(verticalAlignment = Alignment.Bottom) {
				Text("${m.width} × ${m.height}", style = MaterialTheme.typography.headlineMedium)
				Spacer(Modifier.width(Space.sPlus))
				Text(Fmt.hz(m.refresh), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(bottom = Space.unitLiftSmall))
			}
		}
		StatRow(stringResource(R.string.displays_port), d.connector + (gpuName?.let { " · $it" } ?: ""))
		if (d.widthMm != null && d.heightMm != null) {
			StatRow(stringResource(R.string.displays_size), "${Fmt.inches(d.widthMm, d.heightMm)} · ${Fmt.centimetres(d.widthMm, d.heightMm)}")
		}
		d.preferred?.let { StatRow(stringResource(R.string.displays_native_mode), "${it.width} × ${it.height} · ${Fmt.hz(it.refresh)}") }
		d.maxRefresh?.let { StatRow(stringResource(R.string.displays_highest_refresh), Fmt.hz(it)) }
		d.position?.takeIf { d.mode != null }?.let { StatRow(stringResource(R.string.displays_position), "${it[0]}, ${it[1]}") }
		d.year?.let { StatRow(stringResource(R.string.displays_made_in), it.toString()) }
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
