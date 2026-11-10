package ua.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import ua.elamon.MachineState
import ua.elamon.MachineViewModel
import ua.elamon.ProcRequest
import ua.elamon.R
import ua.elamon.data.Proc

@Composable
fun detailTitle(kind: String, state: MachineState): String = when {
	kind == "cpu" -> stringResource(R.string.processor)
	kind == "memory" -> stringResource(R.string.memory)
	kind.startsWith("gpu/") -> state.info?.gpus?.find { it.id == kind.removePrefix("gpu/") }?.name?.let(::shortGpuName) ?: stringResource(R.string.graphics)
	kind == "network" -> stringResource(R.string.network)
	kind == "storage" -> stringResource(R.string.storage)
	kind == "battery" -> stringResource(R.string.battery)
	kind == "temps" -> stringResource(R.string.temperatures)
	else -> kind
}

@Composable
fun DetailPage(kind: String, state: MachineState, vm: MachineViewModel, list: LazyListState) {
	val sort = when {
		kind == "cpu" -> "cpu"
		kind == "memory" -> "mem"
		kind.startsWith("gpu/") -> "gpu"
		kind == "storage" -> "io"
		else -> null
	}
	// The heaviest processes for what this page is about.
	DisposableEffect(kind) {
		if (sort != null) vm.setProcesses(ProcRequest(sort, ProcRequest.TOP))
		onDispose { if (sort != null) vm.setProcesses(null) }
	}
	val s = state.sample ?: return
	LazyColumn(
		Modifier.fillMaxSize(),
		state = list,
		contentPadding = listPadding(bottomNav = false),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		item(key = "title") { BigTitle(detailTitle(kind, state), list = list) }
		when {
			kind == "cpu" -> cpu(state)
			kind == "memory" -> memory(state)
			kind.startsWith("gpu/") -> gpu(state, kind.removePrefix("gpu/"))
			kind == "network" -> network(state)
			kind == "storage" -> storage(state)
			kind == "battery" -> battery(state)
			kind == "temps" -> sensorItems(state)
		}
		if (sort != null) topProcesses(state.processes, sort)
	}
}

private fun LazyListScope.cpu(state: MachineState) {
	val s = state.sample ?: return
	item(key = "chart") {
		val c = Metric.colors
		Section(stringResource(R.string.details_load), ElIcons.Chip, accent = c.cpu, trailing = { RollingText(Fmt.percent(s.cpu.usage), MaterialTheme.typography.titleLarge, color = c.cpu) }) {
			LineChart(
				listOf(ChartSeries(state.hist.cpu, c.cpu, format = { Fmt.percent(it) })),
				maxValue = 100f, height = 200.dp, label = { Fmt.percent(100f) },
			)
		}
	}
	item(key = "stats") {
		val p = LocalPrefs.current
		val info = state.info
		Section(stringResource(R.string.details), ElIcons.Overview) {
			info?.cpuModel?.let { StatRow(stringResource(R.string.model), it) }
			if (info != null) {
				val physical = info.cpuPhysicalCores?.let { pluralStringResource(R.plurals.details_cores_physical, it, it) }
				val threads = pluralStringResource(R.plurals.details_threads, info.cpuLogicalCores, info.cpuLogicalCores)
				StatRow(stringResource(R.string.details_cores), listOfNotNull(physical, threads).joinToString(" · "))
			}
			s.cpu.temp?.let { StatRow(stringResource(R.string.temperature), Fmt.temp(it, p.fahrenheit), valueColor = tempColor(it)) }
			if (s.cpu.freq.isNotEmpty()) StatRow(stringResource(R.string.details_clock_average_top), "${Fmt.mhz(s.cpu.freq.average().toInt())} / ${Fmt.mhz(s.cpu.freq.max())}")
			if (s.cpu.load.size == 3) StatRow(stringResource(R.string.details_load_average), s.cpu.load.joinToString("  ") { "%.2f".format(it) })
			StatRow(stringResource(R.string.details_uptime), Fmt.duration(resources(), s.uptime))
		}
	}
	item(key = "cores") { Cores(state) }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Cores(state: MachineState) {
	val s = state.sample ?: return
	val c = Metric.colors
	Section(stringResource(R.string.details_cores), ElIcons.Chip, accent = c.cpu) {
		CoreBars(s.cpu.cores, height = 56.dp)
		FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp), maxItemsInEachRow = 4) {
			s.cpu.cores.forEachIndexed { i, u ->
				Column(
					Modifier.weight(1f).clip(RoundedCornerShape(14.dp)).background(MaterialTheme.colorScheme.surfaceContainerHigh).padding(10.dp),
				) {
					Text(stringResource(R.string.details_core, i), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
					Text(Fmt.percent(u), style = MaterialTheme.typography.titleSmall, color = loadColor(u / 100f, c.cpu))
					s.cpu.freq.getOrNull(i)?.let { Text(Fmt.mhz(it), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1) }
				}
			}
		}
	}
}

private fun LazyListScope.memory(state: MachineState) {
	val s = state.sample ?: return
	val m = s.memory
	item(key = "chart") {
		val c = Metric.colors
		val res = resources()
		Section(stringResource(R.string.details_memory_use), ElIcons.Memory, accent = c.memory, trailing = { RollingText(Fmt.bytes(m.used), MaterialTheme.typography.titleLarge, color = c.memory) }) {
			LineChart(
				listOf(
					ChartSeries(state.hist.mem, c.memory, format = { res.getString(R.string.details_chart_ram, Fmt.bytes(it.toDouble())) }),
					ChartSeries(state.hist.swap, c.swap, fill = false, format = { res.getString(R.string.details_chart_swap, Fmt.bytes(it.toDouble())) }),
				),
				maxValue = m.total.toFloat().coerceAtLeast(1f), height = 200.dp, label = { Fmt.bytes(it.toDouble()) },
			)
			Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
				Legend(c.memory, stringResource(R.string.memory))
				if (m.swapTotal > 0) Legend(c.swap, stringResource(R.string.details_swap))
			}
		}
	}
	item(key = "stats") {
		val c = Metric.colors
		Section(stringResource(R.string.details), ElIcons.Overview) {
			StatRow(stringResource(R.string.details_used), stringResource(R.string.of_total, Fmt.bytes(m.used), Fmt.bytes(m.total)))
			UsageBar(if (m.total > 0) m.used.toFloat() / m.total else 0f, c.memory, height = 10.dp)
			StatRow(stringResource(R.string.details_available), Fmt.bytes(m.available))
			if (m.swapTotal > 0) {
				StatRow(stringResource(R.string.details_swap), stringResource(R.string.of_total, Fmt.bytes(m.swapUsed), Fmt.bytes(m.swapTotal)))
				UsageBar(m.swapUsed.toFloat() / m.swapTotal, c.swap, height = 10.dp)
			}
		}
	}
}

private fun LazyListScope.gpu(state: MachineState, id: String) {
	val s = state.sample ?: return
	val g = s.gpus.find { it.id == id } ?: return
	val info = state.info?.gpus?.find { it.id == id }
	item(key = "chart") {
		val c = Metric.colors
		val p = LocalPrefs.current
		if (g.state == "sleeping") {
			Section(stringResource(R.string.gpu_asleep), ElIcons.Moon, accent = MaterialTheme.colorScheme.secondary) {
				Text(
					stringResource(R.string.details_gpu_asleep_body),
					style = MaterialTheme.typography.bodyMedium,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
				)
			}
		} else {
			Section(stringResource(R.string.details_load), ElIcons.Gpu, accent = c.gpu, trailing = { g.usage?.let { RollingText(Fmt.percent(it), MaterialTheme.typography.titleLarge, color = c.gpu) } }) {
				// Load and °C share the 0–100 scale; its top is 212 °F for those who read Fahrenheit.
				LineChart(
					listOf(
						ChartSeries(state.hist.gpuUsage(id), c.gpu, format = { Fmt.percent(it) }),
						ChartSeries(state.hist.gpuTemp(id), c.hot, fill = false, format = { Fmt.temp(it, p.fahrenheit) }),
					),
					maxValue = 100f, height = 200.dp, label = { "${Fmt.percent(it)} · ${Fmt.temp(it, p.fahrenheit)}" },
				)
				Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
					Legend(c.gpu, stringResource(R.string.details_legend_load, Fmt.PERCENT))
					Legend(c.hot, stringResource(R.string.details_legend_temperature, if (p.fahrenheit) Fmt.FAHRENHEIT else Fmt.CELSIUS))
				}
				if (g.usage == null && info?.vendor == "intel") {
					Text(
						stringResource(R.string.details_gpu_intel),
						style = MaterialTheme.typography.bodySmall,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
					)
				}
			}
		}
	}
	item(key = "stats") {
		val p = LocalPrefs.current
		Section(stringResource(R.string.details), ElIcons.Overview) {
			info?.let {
				StatRow(stringResource(R.string.model), it.name)
				StatRow(stringResource(R.string.details_driver), it.driver)
				StatRow(stringResource(R.string.details_pci_slot), it.id)
			}
			g.temp?.let { StatRow(stringResource(R.string.temperature), Fmt.temp(it, p.fahrenheit), valueColor = tempColor(it)) }
			g.freq?.let { f ->
				val clock = when {
					f == 0 -> stringResource(R.string.details_clock_idle)
					g.freqMax != null -> stringResource(R.string.of_total, Fmt.mhz(f), Fmt.mhz(g.freqMax))
					else -> Fmt.mhz(f)
				}
				StatRow(stringResource(R.string.details_clock), clock)
			}
			g.memFreq?.let { StatRow(stringResource(R.string.details_memory_clock), Fmt.mhz(it)) }
			g.power?.let { StatRow(stringResource(R.string.details_power), Fmt.watts(it)) }
			g.fan?.let { StatRow(stringResource(R.string.details_fan), Fmt.percent(it.toFloat())) }
			if (g.memUsed != null && g.memTotal != null && g.memTotal > 0) {
				StatRow(stringResource(R.string.details_video_memory), stringResource(R.string.of_total, Fmt.bytes(g.memUsed), Fmt.bytes(g.memTotal)))
				UsageBar(g.memUsed.toFloat() / g.memTotal, Metric.colors.gpu, height = 10.dp)
			}
		}
	}
}

private fun LazyListScope.network(state: MachineState) {
	val s = state.sample ?: return
	item(key = "chart") {
		val c = Metric.colors
		val bits = LocalPrefs.current.networkBits
		Section(stringResource(R.string.details_traffic), ElIcons.Network, accent = c.rx) {
			LineChart(
				listOf(
					ChartSeries(state.hist.rx, c.rx, format = { "↓ " + Fmt.rate(it.toDouble(), bits) }),
					ChartSeries(state.hist.tx, c.tx, format = { "↑ " + Fmt.rate(it.toDouble(), bits) }),
				),
				minScale = 64f * 1024, binary = true, height = 200.dp, label = { Fmt.rate(it.toDouble(), bits) },
			)
			Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
				Legend(c.rx, stringResource(R.string.details_download))
				Legend(c.tx, stringResource(R.string.details_upload))
			}
		}
	}
	for (n in s.net.filter { it.up || it.rx > 0 || it.tx > 0 }) {
		item(key = "if-" + n.name) {
			val c = Metric.colors
			val bits = LocalPrefs.current.networkBits
			val kind = stringResource(
				when (n.kind) {
					"wired" -> R.string.details_net_cable
					"wireless" -> R.string.details_net_wifi
					"vpn" -> R.string.details_net_vpn
					else -> R.string.details_net_virtual
				}
			)
			Section(n.name, ElIcons.Network, accent = if (n.kind == "vpn") MaterialTheme.colorScheme.tertiary else c.rx, trailing = { Pill(kind, MaterialTheme.colorScheme.secondary) }) {
				Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
					BigStat(stringResource(R.string.details_rx_now), Fmt.rate(n.rx, bits), color = c.rx)
					BigStat(stringResource(R.string.details_tx_now), Fmt.rate(n.tx, bits), color = c.tx)
				}
				StatRow(stringResource(R.string.details_received), Fmt.bytes(n.rxTotal))
				StatRow(stringResource(R.string.details_sent), Fmt.bytes(n.txTotal))
				n.speed?.let { StatRow(stringResource(R.string.details_link_speed), Fmt.linkSpeed(it)) }
				if (n.addrs.isNotEmpty()) StatRow(stringResource(R.string.details_addresses), n.addrs.filter { !it.startsWith("fe80") }.joinToString(", "))
			}
		}
	}
}

private fun LazyListScope.storage(state: MachineState) {
	val s = state.sample ?: return
	item(key = "io") {
		val c = Metric.colors
		val res = resources()
		Section(stringResource(R.string.details_throughput), ElIcons.Storage, accent = c.read) {
			LineChart(
				listOf(
					ChartSeries(state.hist.read, c.read, format = { res.getString(R.string.details_chart_read, Fmt.rate(it.toDouble())) }),
					ChartSeries(state.hist.write, c.write, format = { res.getString(R.string.details_chart_write, Fmt.rate(it.toDouble())) }),
				),
				minScale = 1024f * 1024, binary = true, height = 200.dp, label = { Fmt.rate(it.toDouble()) },
			)
			Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
				Legend(c.read, stringResource(R.string.details_read))
				Legend(c.write, stringResource(R.string.details_write))
			}
			for (d in s.diskIo) {
				StatRow(d.model ?: d.name, "↓ ${Fmt.rate(d.read)}  ↑ ${Fmt.rate(d.write)}")
			}
		}
	}
	for (d in s.disks) {
		item(key = "disk-" + d.mount) {
			val c = Metric.colors
			val used = d.total - d.available
			val frac = if (d.total > 0) used.toFloat() / d.total else 0f
			Section(d.mount, ElIcons.Storage, accent = c.read, trailing = { Pill(d.fs, MaterialTheme.colorScheme.secondary) }) {
				Row(verticalAlignment = Alignment.Bottom) {
					BigValue(Fmt.percent(frac * 100).dropLast(1), Fmt.PERCENT, large = false)
					Spacer(Modifier.weight(1f))
					Text(stringResource(R.string.of_total, Fmt.bytes(used), Fmt.bytes(d.total)), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
				UsageBar(frac, loadColor(frac, c.read), height = 12.dp)
				StatRow(stringResource(R.string.details_free), Fmt.bytes(d.available))
				StatRow(stringResource(R.string.details_device), d.device)
			}
		}
	}
}

private fun LazyListScope.battery(state: MachineState) {
	val s = state.sample ?: return
	val b = s.battery ?: return
	item(key = "chart") {
		val c = Metric.colors
		Section(stringResource(R.string.details_power_draw), ElIcons.Bolt, accent = c.battery) {
			LineChart(
				listOf(ChartSeries(state.hist.batteryPower, c.battery, format = { Fmt.watts(it) })),
				minScale = 5f, height = 180.dp, label = { Fmt.watts(it) },
			)
		}
	}
	item(key = "stats") {
		Section(stringResource(R.string.details), ElIcons.Battery) {
			b.percent?.let { StatRow(stringResource(R.string.details_charge), Fmt.percent(it)) }
			StatRow(stringResource(R.string.details_battery_state), batteryStatus(b.status))
			b.power?.let { StatRow(stringResource(R.string.details_power), Fmt.watts(it)) }
			b.timeLeft?.let { StatRow(stringResource(if (b.status == "charging") R.string.details_until_full else R.string.details_left), Fmt.duration(resources(), it)) }
			b.energyNow?.let { StatRow(stringResource(R.string.details_energy_now), Fmt.wattHours(it)) }
			b.energyFull?.let { StatRow(stringResource(R.string.details_full_charge), Fmt.wattHours(it)) }
			b.energyDesign?.let { StatRow(stringResource(R.string.details_design_capacity), Fmt.wattHours(it)) }
			if (b.energyFull != null && b.energyDesign != null && b.energyDesign > 0) StatRow(stringResource(R.string.details_health), Fmt.percent(b.energyFull / b.energyDesign * 100))
			b.cycles?.let { StatRow(stringResource(R.string.details_charge_cycles), it.toString()) }
			b.model?.let { StatRow(stringResource(R.string.model), it) }
		}
	}
}

private fun LazyListScope.topProcesses(procs: List<Proc>?, sort: String) {
	item(key = "top") {
		Section(stringResource(R.string.details_top), ElIcons.Processes) {
			if (procs == null) {
				Text(stringResource(R.string.loading), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			} else {
				val top = when (sort) {
					"mem" -> procs.sortedByDescending { it.mem }
					"gpu" -> procs.filter { it.gpu != null || it.gpuMem != null }.sortedByDescending { (it.gpu ?: 0f) * 1e12f + (it.gpuMem ?: 0) }
					"io" -> procs.sortedByDescending { it.read + it.write }
					else -> procs.sortedByDescending { it.cpu }
				}.take(ProcRequest.TOP)
				if (top.isEmpty()) {
					Text(stringResource(R.string.details_top_none), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
				for (p in top) {
					Row(verticalAlignment = Alignment.CenterVertically) {
						LetterAvatar(p.name, 32.dp)
						Spacer(Modifier.width(12.dp))
						Text(p.name, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
						Text(
							when (sort) {
								"mem" -> Fmt.bytes(p.mem)
								"gpu" -> listOfNotNull(p.gpu?.let { Fmt.percent(it, 1) }, p.gpuMem?.let { Fmt.bytes(it) }).joinToString(" · ")
								"io" -> Fmt.rate(p.read + p.write)
								else -> Fmt.percent(p.cpu, 1)
							},
							style = MaterialTheme.typography.titleSmall,
						)
					}
				}
			}
		}
	}
}
