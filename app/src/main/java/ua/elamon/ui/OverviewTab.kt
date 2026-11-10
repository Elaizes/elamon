package ua.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import ua.elamon.R
import ua.elamon.data.Gpu
import ua.elamon.data.GpuInfo
import ua.elamon.data.Sample

/** Padding for a list under a see-through bar: the big title takes the top, the nav bar the bottom. */
@Composable
fun listPadding(bottomNav: Boolean = true): PaddingValues {
	val d = LocalDensity.current
	val status = with(d) { WindowInsets.statusBars.getTop(d).toDp() }
	val nav = with(d) { WindowInsets.navigationBars.getBottom(d).toDp() }
	return PaddingValues(start = 16.dp, end = 16.dp, top = status, bottom = nav + if (bottomNav) 108.dp else 24.dp)
}

@Composable
fun OverviewTab(t: TabScope) {
	val s = t.state.sample ?: return
	val infos = t.state.info?.gpus.orEmpty()
	LazyColumn(
		Modifier.fillMaxSize(),
		state = t.list,
		contentPadding = listPadding(),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		item(key = "header") { t.header() }
		item(key = "cpu-mem") {
			TileRow {
				CpuTile(t, s, Modifier.weight(1f))
				MemoryTile(t, s, Modifier.weight(1f))
			}
		}
		// GPUs pair up; an odd one shares its row with the temperatures.
		val gpus = s.gpus
		gpus.chunked(2).forEachIndexed { i, pair ->
			item(key = "gpus$i") {
				TileRow {
					pair.forEach { g -> GpuTile(t, g, infos.find { it.id == g.id }, Modifier.weight(1f)) }
					if (pair.size == 1) TempTile(t, s, Modifier.weight(1f))
				}
			}
		}
		if (gpus.size % 2 == 0) {
			item(key = "temps") { TempTile(t, s, Modifier.fillMaxWidth()) }
		}
		item(key = "net") { NetworkTile(t, s) }
		item(key = "storage") { StorageTile(t, s) }
		s.battery?.let { item(key = "battery") { BatteryTile(t, s) } }
	}
}

@Composable
private fun TileRow(content: @Composable RowScope.() -> Unit) {
	Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(12.dp), content = content)
}

@Composable
private fun TileFooter(text: String) {
	Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
}

@Composable
private fun CpuTile(t: TabScope, s: Sample, modifier: Modifier) {
	val c = Metric.colors
	val p = LocalPrefs.current
	val cpu = s.cpu
	Tile(modifier.fillMaxHeight(), accent = c.cpu, onClick = { t.openDetail("cpu") }) {
		TileLabel(stringResource(R.string.cpu), ElIcons.Chip, c.cpu)
		Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
			RingGauge(cpu.usage / 100f, loadColor(cpu.usage / 100f, c.cpu), size = 116.dp) {
				Column(horizontalAlignment = Alignment.CenterHorizontally) {
					RollingText(Fmt.percent(cpu.usage), MaterialTheme.typography.headlineMedium)
					Text(stringResource(R.string.overview_cpu_load), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
		}
		val parts = listOfNotNull(cpu.temp?.let { Fmt.temp(it, p.fahrenheit) }, cpu.freq.maxOrNull()?.let { Fmt.mhz(it) })
		TileFooter(parts.joinToString(" · "))
	}
}

@Composable
private fun MemoryTile(t: TabScope, s: Sample, modifier: Modifier) {
	val c = Metric.colors
	val m = s.memory
	val frac = if (m.total > 0) m.used.toFloat() / m.total else 0f
	Tile(modifier.fillMaxHeight(), accent = c.memory, onClick = { t.openDetail("memory") }) {
		TileLabel(stringResource(R.string.memory), ElIcons.Memory, c.memory)
		Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
			RingGauge(frac, loadColor(frac, c.memory), size = 116.dp) {
				Column(horizontalAlignment = Alignment.CenterHorizontally) {
					RollingText(Fmt.percent(frac * 100), MaterialTheme.typography.headlineMedium)
					Text(stringResource(R.string.overview_memory_used), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
		}
		TileFooter(stringResource(R.string.of_total, Fmt.bytes(m.used), Fmt.bytes(m.total)))
	}
}

@Composable
private fun GpuTile(t: TabScope, g: Gpu, info: GpuInfo?, modifier: Modifier) {
	val c = Metric.colors
	val p = LocalPrefs.current
	val name = info?.name?.let(::shortGpuName) ?: stringResource(R.string.gpu)
	val active = stringResource(R.string.overview_gpu_active)
	Tile(modifier.fillMaxHeight(), accent = c.gpu, onClick = { t.openDetail("gpu/" + g.id) }) {
		TileLabel(name, ElIcons.Gpu, c.gpu)
		when {
			g.state == "sleeping" -> {
				Row(verticalAlignment = Alignment.CenterVertically) {
					Icon(ElIcons.Moon, null, tint = MaterialTheme.colorScheme.secondary, modifier = Modifier.size(22.dp))
					Spacer(Modifier.width(8.dp))
					Text(stringResource(R.string.gpu_asleep), style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.secondary)
				}
				TileFooter(stringResource(R.string.overview_gpu_powered_down))
			}
			g.usage != null -> {
				BigValue(Fmt.percent(g.usage).dropLast(1), Fmt.PERCENT)
				Sparkline(t.state.hist.gpuUsage(g.id), c.gpu, maxValue = 100f)
				TileFooter(listOfNotNull(g.temp?.let { Fmt.temp(it, p.fahrenheit) }, g.power?.let { Fmt.watts(it) }).joinToString(" · ").ifEmpty { active })
			}
			g.freq != null -> {
				if (g.freq == 0) {
					Text(stringResource(R.string.overview_gpu_idle), style = MaterialTheme.typography.headlineSmall)
				} else {
					val parts = Fmt.mhz(g.freq).split(' ')
					BigValue(parts[0], parts.getOrNull(1), large = false)
				}
				TileFooter(g.freqMax?.let { stringResource(R.string.overview_gpu_up_to, Fmt.mhz(it)) } ?: active)
			}
			else -> {
				Text(if (g.state == "active") active else "—", style = MaterialTheme.typography.headlineSmall)
				TileFooter(stringResource(R.string.overview_gpu_no_readings))
			}
		}
	}
}

/** "GeForce RTX 3050 Ti Mobile" → "RTX 3050 Ti": a tile has room for the part people say. */
fun shortGpuName(name: String): String =
	name.removePrefix("NVIDIA ").removePrefix("GeForce ").removePrefix("AMD ").removePrefix("Radeon ").removeSuffix(" Mobile").removeSuffix(" Laptop GPU")

@Composable
private fun TempTile(t: TabScope, s: Sample, modifier: Modifier) {
	val c = Metric.colors
	val p = LocalPrefs.current
	val temp = s.cpu.temp ?: s.temps.maxOfOrNull { it.value }
	Tile(modifier.fillMaxHeight(), accent = c.hot, onClick = { t.openDetail("temps") }) {
		TileLabel(stringResource(R.string.temperature), ElIcons.Thermometer, c.hot)
		if (temp != null) {
			val text = Fmt.temp(temp, p.fahrenheit)
			BigValue(text.dropLast(2), text.takeLast(2), color = tempColor(temp))
		} else {
			Text("—", style = MaterialTheme.typography.headlineLarge)
		}
		Sparkline(t.state.hist.cpuTemp, tempColor(temp ?: 40f), maxValue = 100f)
		val fans = s.fans.filter { it.rpm > 0 }
		TileFooter(
			when {
				fans.isNotEmpty() -> pluralStringResource(R.plurals.overview_fans, fans.size, fans.size) + " · " + Fmt.rpm(fans.maxOf { it.rpm })
				s.fans.isNotEmpty() -> stringResource(R.string.overview_fans_stopped)
				else -> pluralStringResource(R.plurals.overview_sensors, s.temps.size, s.temps.size)
			}
		)
	}
}

@Composable
private fun NetworkTile(t: TabScope, s: Sample) {
	val c = Metric.colors
	val bits = LocalPrefs.current.networkBits
	val real = s.net.filter { it.kind == "wired" || it.kind == "wireless" }
	Tile(Modifier.fillMaxWidth(), accent = c.rx, onClick = { t.openDetail("network") }) {
		TileLabel(stringResource(R.string.network), ElIcons.Network, c.rx)
		Row(horizontalArrangement = Arrangement.spacedBy(28.dp)) {
			Column {
				Text(stringResource(R.string.overview_download), style = MaterialTheme.typography.labelMedium, color = c.rx)
				val (v, u) = Fmt.rateParts(real.sumOf { it.rx }, bits)
				BigValue(v, u, large = false)
			}
			Column {
				Text(stringResource(R.string.overview_upload), style = MaterialTheme.typography.labelMedium, color = c.tx)
				val (v, u) = Fmt.rateParts(real.sumOf { it.tx }, bits)
				BigValue(v, u, large = false)
			}
		}
		LineChart(
			listOf(ChartSeries(t.state.hist.rx, c.rx), ChartSeries(t.state.hist.tx, c.tx)),
			minScale = 64f * 1024,
			binary = true,
			height = 56.dp,
			grid = false,
			interactive = false,
		)
		TileFooter(s.net.filter { it.up && it.kind != "virtual" }.joinToString(" · ") { it.name })
	}
}

@Composable
private fun StorageTile(t: TabScope, s: Sample) {
	val c = Metric.colors
	Tile(Modifier.fillMaxWidth(), accent = c.read, onClick = { t.openDetail("storage") }) {
		TileLabel(stringResource(R.string.storage), ElIcons.Storage, c.read)
		for (d in s.disks.take(3)) {
			val used = d.total - d.available
			val frac = if (d.total > 0) used.toFloat() / d.total else 0f
			Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
				Row(verticalAlignment = Alignment.Bottom) {
					Text(d.mount, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
					Text(stringResource(R.string.overview_disk_free, Fmt.bytes(d.available)), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
				UsageBar(frac, loadColor(frac, c.read), height = 10.dp)
			}
		}
		if (s.diskIo.isNotEmpty()) {
			TileFooter(stringResource(R.string.overview_disk_io, Fmt.rate(s.diskIo.sumOf { it.read }), Fmt.rate(s.diskIo.sumOf { it.write })))
		}
	}
}

@Composable
private fun BatteryTile(t: TabScope, s: Sample) {
	val c = Metric.colors
	val b = s.battery ?: return
	val pct = b.percent ?: 0f
	val color = when {
		pct < 15f -> c.hot
		pct < 30f -> c.warn
		else -> c.good
	}
	Tile(Modifier.fillMaxWidth(), accent = c.battery, onClick = { t.openDetail("battery") }) {
		TileLabel(stringResource(R.string.battery), ElIcons.Battery, c.battery)
		Row(verticalAlignment = Alignment.CenterVertically) {
			BigValue(Fmt.percent(pct).dropLast(1), Fmt.PERCENT, color = color)
			Spacer(Modifier.weight(1f))
			Pill(batteryStatus(b.status), color, dot = true)
		}
		UsageBar(pct / 100f, color, height = 10.dp)
		val res = resources()
		val parts = listOfNotNull(
			b.power?.takeIf { it > 0.05f }?.let { Fmt.watts(it) },
			b.timeLeft?.let { stringResource(if (b.status == "charging") R.string.overview_battery_full_in else R.string.overview_battery_left, Fmt.duration(res, it)) },
			if (b.energyFull != null && b.energyDesign != null && b.energyDesign > 0) stringResource(R.string.overview_battery_health, Fmt.percent(b.energyFull / b.energyDesign * 100)) else null,
		)
		if (parts.isNotEmpty()) TileFooter(parts.joinToString(" · "))
	}
}

@Composable
fun batteryStatus(s: String) = stringResource(
	when (s) {
		"charging" -> R.string.battery_charging
		"discharging" -> R.string.battery_discharging
		"full" -> R.string.battery_full
		"not_charging" -> R.string.battery_not_charging
		else -> R.string.battery_unknown
	}
)
