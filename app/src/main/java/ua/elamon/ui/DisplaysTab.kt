package ua.elamon.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import ua.elamon.MachineState
import ua.elamon.R
import ua.elamon.data.Display
import kotlin.math.max
import kotlin.math.min

@Composable
fun DisplaysTab(state: MachineState, padding: PaddingValues) {
	val s = state.sample ?: return
	val connected = s.displays.filter { it.connected }
	val free = s.displays.filter { !it.connected }
	val gpuNames = state.info?.gpus.orEmpty().associate { it.id to it.name }
	val active = connected.filter { it.active && it.mode != null && it.position != null }

	LazyColumn(
		Modifier.fillMaxSize(),
		contentPadding = PaddingValues(16.dp, padding.calculateTopPadding() + 8.dp, 16.dp, padding.calculateBottomPadding() + 16.dp),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		if (active.size > 1) {
			item(key = "layout") {
				Section(stringResource(R.string.display_layout), ElIcons.Overview) { DisplayArrangement(active) }
			}
		}
		items(connected, key = { it.gpu + it.connector }) { d -> DisplayCard(d, gpuNames[d.gpu]) }
		if (connected.isEmpty()) {
			item { Notice(ElIcons.Monitor, stringResource(R.string.no_displays), stringResource(R.string.no_displays_body)) }
		}
		if (free.isNotEmpty()) {
			item(key = "free") {
				Text(
					stringResource(R.string.free_ports, free.joinToString(", ") { it.connector }),
					style = MaterialTheme.typography.bodySmall,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
					modifier = Modifier.fillMaxWidth(),
					textAlign = TextAlign.Center,
				)
			}
		}
	}
}

@Composable
private fun DisplayCard(d: Display, gpuName: String?) {
	val ctx = LocalContext.current
	val c = Metric.colors
	val name = listOfNotNull(d.vendor, d.model).joinToString(" ").ifEmpty {
		stringResource(if (d.internal) R.string.display_builtin else R.string.display_external)
	}
	val status = when {
		d.active -> R.string.display_on
		d.mode != null || d.dpms == "Off" -> R.string.display_off
		else -> R.string.display_unused
	}
	Section(name, if (d.internal) ElIcons.Laptop else ElIcons.Monitor, trailing = { Pill(stringResource(status), if (d.active) c.good else MaterialTheme.colorScheme.secondary) }) {
		d.mode?.let { m ->
			Column {
				Text("${m.width} × ${m.height}", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
				Text(Fmt.hz(ctx, m.refresh), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
			}
		}
		StatRow(stringResource(R.string.display_connector), d.connector + (gpuName?.let { " · $it" } ?: ""))
		if (d.widthMm != null && d.heightMm != null) {
			StatRow(stringResource(R.string.display_size), "${Fmt.inches(d.widthMm, d.heightMm)} · ${d.widthMm / 10} × ${d.heightMm / 10} cm")
		}
		d.preferred?.let { StatRow(stringResource(R.string.display_native), "${it.width} × ${it.height} · ${Fmt.hz(ctx, it.refresh)}") }
		d.maxRefresh?.let { StatRow(stringResource(R.string.display_max_refresh), Fmt.hz(ctx, it)) }
		d.position?.takeIf { d.mode != null }?.let { StatRow(stringResource(R.string.display_position), "${it[0]}, ${it[1]}") }
		d.year?.let { StatRow(stringResource(R.string.display_year), it.toString()) }
	}
}

/** The monitors as the compositor places them, scaled to fit. */
@Composable
private fun DisplayArrangement(displays: List<Display>) {
	val measurer = rememberTextMeasurer()
	val fill = MaterialTheme.colorScheme.primaryContainer
	val edge = MaterialTheme.colorScheme.primary
	val text = MaterialTheme.colorScheme.onPrimaryContainer
	val minX = displays.minOf { it.position!![0] }
	val minY = displays.minOf { it.position!![1] }
	val maxX = displays.maxOf { it.position!![0] + it.mode!!.width }
	val maxY = displays.maxOf { it.position!![1] + it.mode!!.height }
	val spanW = (maxX - minX).coerceAtLeast(1)
	val spanH = (maxY - minY).coerceAtLeast(1)
	Canvas(Modifier.fillMaxWidth().height(140.dp)) {
		val scale = min(size.width / spanW, size.height / spanH)
		val ox = (size.width - spanW * scale) / 2
		val oy = (size.height - spanH * scale) / 2
		val gap = 3.dp.toPx()
		for (d in displays) {
			val p = d.position!!
			val m = d.mode!!
			val tl = Offset(ox + (p[0] - minX) * scale + gap, oy + (p[1] - minY) * scale + gap)
			val sz = Size(max(m.width * scale - 2 * gap, 1f), max(m.height * scale - 2 * gap, 1f))
			val r = CornerRadius(8.dp.toPx())
			drawRoundRect(fill, tl, sz, r)
			drawRoundRect(edge, tl, sz, r, style = Stroke(2.dp.toPx()))
			val label = measurer.measure(d.connector, TextStyle(color = text, fontSize = 13.sp, fontWeight = FontWeight.SemiBold))
			drawText(
				label,
				topLeft = Offset(tl.x + (sz.width - label.size.width) / 2, tl.y + (sz.height - label.size.height) / 2),
			)
		}
	}
}
