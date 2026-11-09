package ua.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import ua.elamon.MachineState
import ua.elamon.MachineViewModel
import ua.elamon.R
import ua.elamon.data.Temp

private val KIND_ORDER = listOf("cpu", "gpu", "storage", "wireless", "board", "battery", "other")

private data class Group(val kind: String, val chip: String, val device: String?, val temps: List<Temp>)

@Composable
fun SensorsTab(state: MachineState, padding: PaddingValues) {
	val s = state.sample ?: return
	val ctx = LocalContext.current
	val c = Metric.colors
	val groups = s.temps
		.groupBy { Triple(it.kind, it.chip, it.device) }
		.map { (k, v) -> Group(k.first, k.second, k.third, v.sortedWith(compareBy({ !isMain(it) }, { naturalKey(it.label) }))) }
		.sortedWith(compareBy({ KIND_ORDER.indexOf(it.kind).let { i -> if (i < 0) 99 else i } }, { it.chip }, { it.device }))
	val gpuNames = state.info?.gpus.orEmpty().associate { it.id to it.name }

	LazyColumn(
		Modifier.fillMaxSize(),
		contentPadding = PaddingValues(16.dp, padding.calculateTopPadding() + 8.dp, 16.dp, padding.calculateBottomPadding() + 16.dp),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		item(key = "chart") {
			Section(stringResource(R.string.temperatures), ElIcons.Thermometer, accent = c.hot) {
				val series = mutableListOf(ChartSeries(state.hist.cpuTemp, c.cpu, fill = false))
				val gpuTemps = state.hist.gpuTemp.filterValues { v -> v.any { !it.isNaN() } }
				gpuTemps.values.forEach { series += ChartSeries(it, c.gpu, fill = false) }
				LineChart(series, MachineViewModel.HISTORY_CAP, minScale = 60f, height = 120.dp, label = { Fmt.temp(it) })
				Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
					Legend(c.cpu, stringResource(R.string.cpu))
					if (gpuTemps.isNotEmpty()) Legend(c.gpu, stringResource(R.string.gpu))
				}
			}
		}
		val gpusWithTemp = s.gpus.filter { it.temp != null }
		if (gpusWithTemp.isNotEmpty()) {
			item(key = "gpus") {
				Section(stringResource(R.string.sensor_kind_gpu), ElIcons.Gpu, accent = c.gpu) {
					for (g in gpusWithTemp) TempRow(gpuNames[g.id] ?: g.id, g.temp!!, null, null)
				}
			}
		}
		items(groups, key = { "g" + it.kind + it.chip + it.device }) { g ->
			val title = kindName(g.kind) + (g.device?.let { " · $it" } ?: "")
			Section(title, iconFor(g.kind), accent = accentFor(g.kind)) {
				for (t in g.temps) TempRow(t.label, t.value, t.high, t.crit)
			}
		}
		if (s.fans.isNotEmpty()) {
			item(key = "fans") {
				Section(stringResource(R.string.fans), ElIcons.Fan) {
					val top = (s.fans.maxOf { it.rpm } * 1.25f).coerceAtLeast(3000f)
					for (f in s.fans) {
						Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
							StatRow(f.label, if (f.rpm == 0) stringResource(R.string.fan_stopped) else Fmt.rpm(ctx, f.rpm))
							UsageBar(f.rpm / top, MaterialTheme.colorScheme.primary, height = 6.dp)
						}
					}
				}
			}
		}
		if (s.temps.isEmpty() && s.fans.isEmpty()) {
			item { Notice(ElIcons.Thermometer, stringResource(R.string.no_sensors), stringResource(R.string.no_sensors_body)) }
		}
	}
}

@Composable
private fun TempRow(label: String, value: Float, high: Float?, crit: Float?) {
	val color = tempColor(value, high, crit)
	val top = crit ?: high ?: 100f
	Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			Text(label, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
			// The limit only matters when it gets close; on every row it is noise.
			if (crit != null && value >= crit - 15f) {
				Text(
					stringResource(R.string.temp_limit, Fmt.temp(crit)),
					style = MaterialTheme.typography.bodySmall,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
				)
				Spacer(Modifier.width(10.dp))
			}
			Text(Fmt.temp(value), style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold, color = color)
		}
		UsageBar(value / top, color, height = 6.dp)
	}
}

private fun isMain(t: Temp): Boolean {
	val l = t.label.lowercase()
	return l.startsWith("package") || l == "tctl" || l == "tdie" || l == "composite" || l == "edge"
}

/** "Core 10" after "Core 2". */
private fun naturalKey(s: String): String = s.replace(Regex("\\d+")) { it.value.padStart(6, '0') }

@Composable
private fun kindName(kind: String): String = when (kind) {
	"cpu" -> stringResource(R.string.sensor_kind_cpu)
	"gpu" -> stringResource(R.string.sensor_kind_gpu)
	"storage" -> stringResource(R.string.sensor_kind_storage)
	"wireless" -> stringResource(R.string.sensor_kind_wireless)
	"board" -> stringResource(R.string.sensor_kind_board)
	"battery" -> stringResource(R.string.sensor_kind_battery)
	else -> stringResource(R.string.sensor_kind_other)
}

@Composable
private fun accentFor(kind: String) = when (kind) {
	"cpu" -> Metric.colors.cpu
	"gpu" -> Metric.colors.gpu
	"storage" -> Metric.colors.read
	"wireless" -> Metric.colors.rx
	"battery" -> Metric.colors.battery
	else -> MaterialTheme.colorScheme.secondary
}

private fun iconFor(kind: String) = when (kind) {
	"cpu" -> ElIcons.Chip
	"gpu" -> ElIcons.Gpu
	"storage" -> ElIcons.Storage
	"wireless" -> ElIcons.Network
	"battery" -> ElIcons.Battery
	else -> ElIcons.Thermometer
}
