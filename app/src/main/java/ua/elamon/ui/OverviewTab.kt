package ua.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
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
import ua.elamon.data.Battery
import ua.elamon.data.Gpu
import ua.elamon.data.GpuInfo
import ua.elamon.data.Sample

@Composable
fun OverviewTab(state: MachineState, padding: PaddingValues) {
	val s = state.sample ?: return
	LazyColumn(
		Modifier.fillMaxSize(),
		contentPadding = PaddingValues(16.dp, padding.calculateTopPadding() + 8.dp, 16.dp, padding.calculateBottomPadding() + 16.dp),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		item(key = "system") { SystemCard(state, s) }
		item(key = "cpu") { CpuCard(state, s) }
		item(key = "memory") { MemoryCard(state, s) }
		val infos = state.info?.gpus.orEmpty()
		items(s.gpus.size, key = { "gpu" + s.gpus[it].id }) { i ->
			val g = s.gpus[i]
			GpuCard(state, g, infos.find { it.id == g.id })
		}
		s.battery?.let { b -> item(key = "battery") { BatteryCard(state, b) } }
		item(key = "net") { NetworkCard(state, s) }
		item(key = "disks") { DisksCard(state, s) }
	}
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SystemCard(state: MachineState, s: Sample) {
	val ctx = LocalContext.current
	val info = state.info
	Section(info?.os ?: stringResource(R.string.system), ElIcons.Laptop) {
		if (info != null) {
			Text(
				listOf(info.cpuModel, "Linux ${info.kernel}").filter { it.isNotBlank() }.joinToString(" · "),
				style = MaterialTheme.typography.bodyMedium,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
			)
		}
		FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
			Pill(stringResource(R.string.uptime, Fmt.duration(ctx, s.uptime)), MaterialTheme.colorScheme.secondary)
			when (s.power.onAc) {
				true -> Pill(stringResource(R.string.on_ac), Metric.colors.good)
				false -> Pill(stringResource(R.string.on_battery), Metric.colors.warn)
				null -> {}
			}
			s.power.profile?.let { Pill(stringResource(R.string.profile, profileName(it)), MaterialTheme.colorScheme.tertiary) }
			s.power.epp?.let { Pill(stringResource(R.string.epp, it.replace('_', ' ')), MaterialTheme.colorScheme.secondary) }
		}
	}
}

@Composable
fun profileName(p: String): String = when (p) {
	"low-power" -> stringResource(R.string.profile_low_power)
	"cool" -> stringResource(R.string.profile_cool)
	"quiet" -> stringResource(R.string.profile_quiet)
	"balanced" -> stringResource(R.string.profile_balanced)
	"balanced-performance" -> stringResource(R.string.profile_balanced_performance)
	"performance" -> stringResource(R.string.profile_performance)
	else -> p
}

@Composable
private fun CpuCard(state: MachineState, s: Sample) {
	val ctx = LocalContext.current
	val c = Metric.colors
	val cpu = s.cpu
	Section(stringResource(R.string.cpu), ElIcons.Chip, accent = c.cpu) {
		Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(20.dp)) {
			RingGauge(cpu.usage / 100f, loadColor(cpu.usage / 100f)) {
				Column(horizontalAlignment = Alignment.CenterHorizontally) {
					Text(Fmt.percent(cpu.usage), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
					Text(stringResource(R.string.load), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
				cpu.temp?.let { BigStat(stringResource(R.string.temperature), Fmt.temp(it), color = tempColor(it)) }
				if (cpu.freq.isNotEmpty()) {
					BigStat(stringResource(R.string.clock_avg_max), "${Fmt.mhz(ctx, cpu.freq.average().toInt())} / ${Fmt.mhz(ctx, cpu.freq.max())}")
				}
				if (cpu.load.size == 3) {
					BigStat(stringResource(R.string.load_average), cpu.load.joinToString("  ") { "%.2f".format(it) })
				}
			}
		}
		LineChart(
			listOf(ChartSeries(state.hist.cpu, c.cpu)),
			MachineViewModel.HISTORY_CAP,
			maxValue = 100f,
			label = { "100%" },
		)
		if (cpu.cores.isNotEmpty()) {
			Text(
				stringResource(R.string.cores_count, cpu.cores.size),
				style = MaterialTheme.typography.labelMedium,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
			)
			CoreBars(cpu.cores)
		}
	}
}

@Composable
private fun MemoryCard(state: MachineState, s: Sample) {
	val ctx = LocalContext.current
	val c = Metric.colors
	val m = s.memory
	val frac = if (m.total > 0) m.used.toFloat() / m.total else 0f
	Section(stringResource(R.string.memory), ElIcons.Memory, accent = c.memory) {
		Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(20.dp)) {
			RingGauge(frac, c.memory) {
				Column(horizontalAlignment = Alignment.CenterHorizontally) {
					Text(Fmt.percent(frac * 100), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
					Text(stringResource(R.string.used), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
				BigStat(stringResource(R.string.used_label), "${Fmt.bytes(ctx, m.used)} / ${Fmt.bytes(ctx, m.total)}")
				BigStat(stringResource(R.string.available), Fmt.bytes(ctx, m.available))
				if (m.swapTotal > 0) {
					BigStat(stringResource(R.string.swap), "${Fmt.bytes(ctx, m.swapUsed)} / ${Fmt.bytes(ctx, m.swapTotal)}", color = c.swap)
				}
			}
		}
		LineChart(
			listOf(ChartSeries(state.hist.mem, c.memory), ChartSeries(state.hist.swap, c.swap, fill = false)),
			MachineViewModel.HISTORY_CAP,
			maxValue = m.total.toFloat().coerceAtLeast(1f),
		)
		if (m.swapTotal > 0) {
			Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
				Legend(c.memory, stringResource(R.string.memory))
				Legend(c.swap, stringResource(R.string.swap))
			}
		}
	}
}

@Composable
private fun GpuCard(state: MachineState, g: Gpu, info: GpuInfo?) {
	val ctx = LocalContext.current
	val c = Metric.colors
	val title = info?.name ?: stringResource(R.string.gpu)
	val sleeping = g.state == "sleeping"
	Section(
		title,
		ElIcons.Gpu,
		accent = c.gpu,
		trailing = {
			when (g.state) {
				"sleeping" -> Pill(stringResource(R.string.gpu_sleeping), MaterialTheme.colorScheme.secondary)
				"active" -> Pill(stringResource(R.string.gpu_active), c.good)
				else -> {}
			}
		},
	) {
		if (sleeping) {
			Text(stringResource(R.string.gpu_sleeping_body), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			return@Section
		}
		Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(20.dp)) {
			if (g.usage != null) {
				RingGauge(g.usage / 100f, c.gpu) {
					Column(horizontalAlignment = Alignment.CenterHorizontally) {
						Text(Fmt.percent(g.usage), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
						Text(stringResource(R.string.load), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
					}
				}
			}
			Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
				g.temp?.let { BigStat(stringResource(R.string.temperature), Fmt.temp(it), color = tempColor(it)) }
				g.freq?.let { f ->
					val max = g.freqMax?.let { " / ${Fmt.mhz(ctx, it)}" }.orEmpty()
					BigStat(stringResource(R.string.clock), if (f == 0) stringResource(R.string.gpu_idle) else Fmt.mhz(ctx, f) + max)
				}
				g.power?.let { BigStat(stringResource(R.string.power_draw), Fmt.watts(ctx, it)) }
			}
		}
		if (g.memUsed != null && g.memTotal != null && g.memTotal > 0) {
			StatRow(stringResource(R.string.vram), "${Fmt.bytes(ctx, g.memUsed)} / ${Fmt.bytes(ctx, g.memTotal)}")
			UsageBar(g.memUsed.toFloat() / g.memTotal, c.gpu)
		}
		g.fan?.let { StatRow(stringResource(R.string.fan), Fmt.percent(it.toFloat())) }
		val usage = state.hist.gpuUsage[g.id].orEmpty()
		if (usage.any { !it.isNaN() }) {
			LineChart(listOf(ChartSeries(usage, c.gpu)), MachineViewModel.HISTORY_CAP, maxValue = 100f, label = { "100%" })
		}
		if (g.usage == null && g.state == "active" && info?.vendor == "intel") {
			Text(stringResource(R.string.gpu_intel_note), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
		}
	}
}

@Composable
private fun BatteryCard(state: MachineState, b: Battery) {
	val ctx = LocalContext.current
	val c = Metric.colors
	val pct = b.percent ?: 0f
	val statusText = when (b.status) {
		"charging" -> stringResource(R.string.battery_charging)
		"discharging" -> stringResource(R.string.battery_discharging)
		"full" -> stringResource(R.string.battery_full)
		"not_charging" -> stringResource(R.string.battery_not_charging)
		else -> stringResource(R.string.battery_unknown)
	}
	val color = when {
		pct < 15f -> c.hot
		pct < 30f -> c.warn
		else -> c.good
	}
	Section(stringResource(R.string.battery), ElIcons.Battery, accent = c.battery, trailing = { Pill(statusText, color) }) {
		Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(20.dp)) {
			RingGauge(pct / 100f, color) {
				Text(Fmt.percent(pct), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
			}
			Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
				b.power?.takeIf { it > 0.05f }?.let { BigStat(stringResource(R.string.power_draw), Fmt.watts(ctx, it)) }
				b.timeLeft?.let {
					val label = if (b.status == "charging") R.string.until_full else R.string.until_empty
					BigStat(stringResource(label), Fmt.duration(ctx, it))
				}
				if (b.energyFull != null && b.energyDesign != null && b.energyDesign > 0) {
					BigStat(stringResource(R.string.battery_health), Fmt.percent(b.energyFull / b.energyDesign * 100))
				}
			}
		}
		b.cycles?.let { StatRow(stringResource(R.string.battery_cycles), it.toString()) }
		if (b.energyFull != null) StatRow(stringResource(R.string.battery_capacity), stringResource(R.string.unit_wh, "%.1f".format(b.energyFull)))
		val power = state.hist.batteryPower
		if (power.any { !it.isNaN() && it > 0.05f }) {
			LineChart(listOf(ChartSeries(power, c.battery)), MachineViewModel.HISTORY_CAP, minScale = 5f, label = { Fmt.watts(ctx, it) })
		}
	}
}

@Composable
private fun NetworkCard(state: MachineState, s: Sample) {
	val ctx = LocalContext.current
	val c = Metric.colors
	val real = s.net.filter { it.kind == "wired" || it.kind == "wireless" }
	Section(stringResource(R.string.network), ElIcons.Network, accent = c.rx) {
		Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
			BigStat(stringResource(R.string.download), Fmt.rate(ctx, real.sumOf { it.rx }), color = c.rx)
			BigStat(stringResource(R.string.upload), Fmt.rate(ctx, real.sumOf { it.tx }), color = c.tx)
		}
		LineChart(
			listOf(ChartSeries(state.hist.rx, c.rx), ChartSeries(state.hist.tx, c.tx)),
			MachineViewModel.HISTORY_CAP,
			minScale = 64f * 1024,
			binary = true,
			label = { Fmt.rate(ctx, it.toDouble()) },
		)
		for (n in s.net.filter { it.up && it.kind != "virtual" }) {
			val kind = when (n.kind) {
				"wired" -> stringResource(R.string.net_wired)
				"wireless" -> stringResource(R.string.net_wireless)
				"vpn" -> stringResource(R.string.net_vpn)
				else -> ""
			}
			Column {
				Row(verticalAlignment = Alignment.CenterVertically) {
					Text(n.name, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium)
					Text(
						"  $kind" + (n.speed?.let { " · " + stringResource(R.string.unit_mbps, it) } ?: ""),
						style = MaterialTheme.typography.bodySmall,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
						modifier = Modifier.weight(1f),
					)
					Text("↓ ${Fmt.rate(ctx, n.rx)}  ↑ ${Fmt.rate(ctx, n.tx)}", style = MaterialTheme.typography.bodySmall)
				}
				val v4 = n.addrs.filter { !it.contains(':') }
				if (v4.isNotEmpty()) {
					Text(
						v4.joinToString(", "),
						style = MaterialTheme.typography.bodySmall,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
						maxLines = 1,
						overflow = TextOverflow.Ellipsis,
					)
				}
			}
		}
	}
}

@Composable
private fun DisksCard(state: MachineState, s: Sample) {
	val ctx = LocalContext.current
	val c = Metric.colors
	Section(stringResource(R.string.storage), ElIcons.Storage, accent = c.read) {
		for (d in s.disks) {
			val used = d.total - d.available
			val frac = if (d.total > 0) used.toFloat() / d.total else 0f
			Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
				Row(verticalAlignment = Alignment.CenterVertically) {
					Text(d.mount, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
					Text(
						"${Fmt.bytes(ctx, used)} / ${Fmt.bytes(ctx, d.total)} · ${d.fs}",
						style = MaterialTheme.typography.bodySmall,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
					)
				}
				UsageBar(frac, loadColor(frac).takeIf { frac >= 0.75f } ?: c.read)
			}
		}
		if (s.diskIo.isNotEmpty()) {
			Row(horizontalArrangement = Arrangement.spacedBy(24.dp), modifier = Modifier.fillMaxWidth()) {
				BigStat(stringResource(R.string.disk_read), Fmt.rate(ctx, s.diskIo.sumOf { it.read }), color = c.read)
				BigStat(stringResource(R.string.disk_write), Fmt.rate(ctx, s.diskIo.sumOf { it.write }), color = c.write)
			}
			LineChart(
				listOf(ChartSeries(state.hist.read, c.read), ChartSeries(state.hist.write, c.write)),
				MachineViewModel.HISTORY_CAP,
				minScale = 1024f * 1024,
				binary = true,
				label = { Fmt.rate(ctx, it.toDouble()) },
			)
		}
	}
}
