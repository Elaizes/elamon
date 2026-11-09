package ua.elamon.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import ua.elamon.data.Display
import kotlin.math.max
import kotlin.math.min

@Composable
fun DisplaysTab(t: TabScope) {
	val s = t.state.sample ?: return
	val connected = s.displays.filter { it.connected }
	val free = s.displays.filter { !it.connected }
	val gpuNames = t.state.info?.gpus.orEmpty().associate { it.id to shortGpuName(it.name) }
	val active = connected.filter { it.active && it.mode != null && it.position != null }

	LazyColumn(
		Modifier.fillMaxSize(),
		state = t.list,
		contentPadding = listPadding(),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		item(key = "header") { t.header() }
		if (active.size > 1) {
			item(key = "layout") {
				Section("Arrangement", ElIcons.Overview) { DisplayLayout(active) }
			}
		}
		items(connected, key = { it.gpu + it.connector }) { d -> DisplayCard(d, gpuNames[d.gpu]) }
		if (connected.isEmpty()) {
			item { Notice(ElIcons.Monitor, "No monitors connected", "The computer runs without a screen, or its graphics driver doesn't report monitors.") }
		}
		if (free.isNotEmpty()) {
			item(key = "free") {
				Text(
					"Free ports: " + free.joinToString(", ") { it.connector },
					style = MaterialTheme.typography.bodySmall,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
					modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
					textAlign = TextAlign.Center,
				)
			}
		}
	}
}

@Composable
private fun DisplayCard(d: Display, gpuName: String?) {
	val c = Metric.colors
	val name = listOfNotNull(d.vendor, d.model).joinToString(" ").ifEmpty { if (d.internal) "Built-in display" else "Monitor" }
	val (status, color) = when {
		d.active -> "On" to c.good
		d.mode != null || d.dpms == "Off" -> "Off" to MaterialTheme.colorScheme.secondary
		else -> "Not in use" to MaterialTheme.colorScheme.secondary
	}
	Section(name, if (d.internal) ElIcons.Laptop else ElIcons.Monitor, trailing = { Pill(status, color, dot = true) }) {
		d.mode?.let { m ->
			Row(verticalAlignment = Alignment.Bottom) {
				Text("${m.width} × ${m.height}", style = MaterialTheme.typography.headlineMedium)
				Spacer(Modifier.width(10.dp))
				Text(Fmt.hz(m.refresh), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(bottom = 3.dp))
			}
		}
		StatRow("Port", d.connector + (gpuName?.let { " · $it" } ?: ""))
		if (d.widthMm != null && d.heightMm != null) {
			StatRow("Size", "${Fmt.inches(d.widthMm, d.heightMm)} · ${d.widthMm / 10} × ${d.heightMm / 10} cm")
		}
		d.preferred?.let { StatRow("Native mode", "${it.width} × ${it.height} · ${Fmt.hz(it.refresh)}") }
		d.maxRefresh?.let { StatRow("Highest refresh", Fmt.hz(it)) }
		d.position?.takeIf { d.mode != null }?.let { StatRow("Position", "${it[0]}, ${it[1]}") }
		d.year?.let { StatRow("Made in", it.toString()) }
	}
}

/** The monitors as the compositor places them, scaled to fit. */
@Composable
private fun DisplayLayout(displays: List<Display>) {
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
	Canvas(Modifier.fillMaxWidth().height(150.dp)) {
		val scale = min(size.width / spanW, size.height / spanH)
		val ox = (size.width - spanW * scale) / 2
		val oy = (size.height - spanH * scale) / 2
		val gap = 4.dp.toPx()
		for (d in displays) {
			val p = d.position!!
			val m = d.mode!!
			val tl = Offset(ox + (p[0] - minX) * scale + gap, oy + (p[1] - minY) * scale + gap)
			val sz = Size(max(m.width * scale - 2 * gap, 1f), max(m.height * scale - 2 * gap, 1f))
			val r = CornerRadius(12.dp.toPx())
			drawRoundRect(Brush.linearGradient(listOf(fill, fill.copy(alpha = 0.6f)), tl, Offset(tl.x + sz.width, tl.y + sz.height)), tl, sz, r)
			drawRoundRect(edge, tl, sz, r, style = Stroke(2.dp.toPx()))
			val label = measurer.measure(d.connector, TextStyle(color = text, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, fontFamily = Inter))
			drawText(label, topLeft = Offset(tl.x + (sz.width - label.size.width) / 2, tl.y + (sz.height - label.size.height) / 2))
		}
	}
}
