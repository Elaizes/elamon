package io.github.elaizes.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Temp
import io.github.elaizes.elamon.hasData

private val KIND_ORDER = listOf("cpu", "gpu", "storage", "wireless", "board", "battery", "other")

private data class Group(val kind: String, val chip: String, val device: String?, val temps: List<Temp>)

@Composable
fun SensorsTab(t: TabScope) {
	LazyColumn(
		Modifier.fillMaxSize(),
		state = t.list,
		contentPadding = listPadding(),
		verticalArrangement = Arrangement.spacedBy(Space.m),
	) {
		item(key = "header") { t.header() }
		sensorItems(t.state)
	}
}

/** The temperature chart, every sensor grouped by device, and the fans. */
fun LazyListScope.sensorItems(state: MachineState) {
	val s = state.sample ?: return
	val groups = s.temps
		.groupBy { Triple(it.kind, it.chip, it.device) }
		.map { (k, v) -> Group(k.first, k.second, k.third, v.sortedWith(compareBy({ !isMain(it) }, { naturalKey(it.label) }))) }
		.sortedWith(compareBy({ KIND_ORDER.indexOf(it.kind).let { i -> if (i < 0) KIND_ORDER.size else i } }, { it.chip }, { it.device }))

	item(key = "chart") {
		val c = Metric.colors
		val p = LocalPrefs.current
		val res = resources()
		Section(stringResource(R.string.temperatures), ElIcons.Thermometer, accent = c.hot) {
			val cpuName = stringResource(R.string.cpu)
			val gpuName = stringResource(R.string.gpu)
			val temp = { v: Float -> Fmt.temp(v, p.fahrenheit) }
			val series = mutableListOf(
				ChartSeries(state.hist.cpuTemp, c.cpu, fill = false, name = cpuName, value = temp, format = { res.getString(R.string.cpu_value, temp(it)) }),
			)
			val gpuTemps = state.hist.gpuIds.map { state.hist.gpuTemp(it) }.filter { it.hasData() }
			gpuTemps.forEach { series += ChartSeries(it, c.gpu, fill = false, name = gpuName, value = temp, format = { v -> res.getString(R.string.gpu_value, temp(v)) }) }
			LineChart(series, minScale = Thresholds.CHART_MIN_TEMP, height = ChartStyle.temperatures, label = temp)
			Row(horizontalArrangement = Arrangement.spacedBy(Space.l)) {
				Legend(c.cpu, cpuName)
				if (gpuTemps.isNotEmpty()) Legend(c.gpu, gpuName)
			}
		}
	}
	val gpusWithTemp = s.gpus.filter { it.temp != null }
	if (gpusWithTemp.isNotEmpty()) {
		item(key = "gpus") {
			val names = state.info?.gpus.orEmpty().associate { it.id to it.name }
			Section(stringResource(R.string.graphics), ElIcons.Gpu, accent = Metric.colors.gpu) {
				for (g in gpusWithTemp) TempRow(names[g.id] ?: g.id, g.temp!!, null, null)
			}
		}
	}
	for (g in groups) {
		item(key = "g" + g.kind + g.chip + g.device) {
			val title = kindName(g.kind) + (g.device?.let { " · $it" } ?: "")
			Section(title, iconFor(g.kind), accent = accentFor(g.kind)) {
				for (t in g.temps) TempRow(t.label, t.value, t.high, t.crit)
			}
		}
	}
	if (s.fans.isNotEmpty()) {
		item(key = "fans") {
			Section(stringResource(R.string.sensors_fans), ElIcons.Fan) {
				val top = fanBarTop(s.fans.maxOf { it.rpm })
				for (f in s.fans) {
					Column(verticalArrangement = Arrangement.spacedBy(Space.xsPlus)) {
						StatRow(f.label, if (f.rpm == 0) stringResource(R.string.sensors_fan_stopped) else Fmt.rpm(f.rpm))
						UsageBar(f.rpm / top, MaterialTheme.colorScheme.primary, height = Sizes.barThin)
					}
				}
			}
		}
	}
	if (s.temps.isEmpty() && s.fans.isEmpty()) {
		item {
			Notice(ElIcons.Thermometer, stringResource(R.string.sensors_none), stringResource(R.string.sensors_none_body))
		}
	}
}

@Composable
private fun TempRow(label: String, value: Float, high: Float?, crit: Float?) {
	val p = LocalPrefs.current
	val color = tempColor(value, high, crit)
	Column(verticalArrangement = Arrangement.spacedBy(Space.xsPlus)) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			Text(label, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
			// The limit only matters when it gets close; on every row it is noise.
			if (crit != null && limitNear(value, crit)) {
				Text(stringResource(R.string.sensors_limit, Fmt.temp(crit, p.fahrenheit)), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				Spacer(Modifier.width(Space.sPlus))
			}
			Text(Fmt.temp(value, p.fahrenheit), style = MaterialTheme.typography.titleSmall, color = color)
		}
		UsageBar(tempBarFraction(value, high, crit), color, height = Sizes.barThin)
	}
}

private fun isMain(t: Temp): Boolean {
	val l = t.label.lowercase()
	return l.startsWith("package") || l == "tctl" || l == "tdie" || l == "composite" || l == "edge"
}

/** Numbers padded this far sort as numbers: "Core 10" after "Core 2". */
private const val NATURAL_DIGITS = 6

/** "Core 10" after "Core 2". */
private fun naturalKey(s: String): String = s.replace(Regex("\\d+")) { it.value.padStart(NATURAL_DIGITS, '0') }

@Composable
private fun kindName(kind: String): String = stringResource(
	when (kind) {
		"cpu" -> R.string.processor
		"gpu" -> R.string.graphics
		"storage" -> R.string.sensors_drive
		"wireless" -> R.string.sensors_wireless
		"board" -> R.string.sensors_board
		"battery" -> R.string.battery
		else -> R.string.sensors_other
	}
)

private fun iconFor(kind: String) = when (kind) {
	"cpu" -> ElIcons.Chip
	"gpu" -> ElIcons.Gpu
	"storage" -> ElIcons.Storage
	"wireless" -> ElIcons.Network
	"battery" -> ElIcons.Battery
	else -> ElIcons.Thermometer
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
