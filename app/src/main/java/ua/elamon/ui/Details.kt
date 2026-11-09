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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import ua.elamon.MachineState
import ua.elamon.MachineViewModel
import ua.elamon.ProcRequest
import ua.elamon.data.Proc

fun detailTitle(kind: String, state: MachineState): String = when {
	kind == "cpu" -> "Processor"
	kind == "memory" -> "Memory"
	kind.startsWith("gpu/") -> state.info?.gpus?.find { it.id == kind.removePrefix("gpu/") }?.name?.let(::shortGpuName) ?: "Graphics"
	kind == "network" -> "Network"
	kind == "storage" -> "Storage"
	kind == "battery" -> "Battery"
	kind == "temps" -> "Temperatures"
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
		if (sort != null) vm.setProcesses(ProcRequest(sort, false))
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

private const val CAP = MachineViewModel.HISTORY_CAP

private fun LazyListScope.cpu(state: MachineState) {
	val s = state.sample ?: return
	item(key = "chart") {
		val c = Metric.colors
		Section("Load", ElIcons.Chip, accent = c.cpu, trailing = { RollingText(Fmt.percent(s.cpu.usage), MaterialTheme.typography.titleLarge, color = c.cpu) }) {
			LineChart(
				listOf(ChartSeries(state.hist.cpu, c.cpu, format = { Fmt.percent(it) })),
				CAP, maxValue = 100f, height = 200.dp, label = { "100%" },
			)
		}
	}
	item(key = "stats") {
		val p = LocalPrefs.current
		val info = state.info
		Section("Details", ElIcons.Overview) {
			info?.cpuModel?.let { StatRow("Model", it) }
			if (info != null) StatRow("Cores", listOfNotNull(info.cpuPhysicalCores?.let { "$it physical" }, "${info.cpuLogicalCores} threads").joinToString(" · "))
			s.cpu.temp?.let { StatRow("Temperature", Fmt.temp(it, p.fahrenheit), valueColor = tempColor(it)) }
			if (s.cpu.freq.isNotEmpty()) StatRow("Clock, average / top", "${Fmt.mhz(s.cpu.freq.average().toInt())} / ${Fmt.mhz(s.cpu.freq.max())}")
			if (s.cpu.load.size == 3) StatRow("Load average 1 · 5 · 15 min", s.cpu.load.joinToString("  ") { "%.2f".format(it) })
			StatRow("Up", Fmt.duration(s.uptime))
		}
	}
	item(key = "cores") { Cores(state) }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Cores(state: MachineState) {
	val s = state.sample ?: return
	val c = Metric.colors
	Section("Cores", ElIcons.Chip, accent = c.cpu) {
		CoreBars(s.cpu.cores, height = 56.dp)
		FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp), maxItemsInEachRow = 4) {
			s.cpu.cores.forEachIndexed { i, u ->
				Column(
					Modifier.weight(1f).clip(RoundedCornerShape(14.dp)).background(MaterialTheme.colorScheme.surfaceContainerHigh).padding(10.dp),
				) {
					Text("Core $i", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
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
		Section("Use", ElIcons.Memory, accent = c.memory, trailing = { RollingText(Fmt.bytes(m.used), MaterialTheme.typography.titleLarge, color = c.memory) }) {
			LineChart(
				listOf(
					ChartSeries(state.hist.mem, c.memory, format = { "RAM " + Fmt.bytes(it.toDouble()) }),
					ChartSeries(state.hist.swap, c.swap, fill = false, format = { "swap " + Fmt.bytes(it.toDouble()) }),
				),
				CAP, maxValue = m.total.toFloat().coerceAtLeast(1f), height = 200.dp, label = { Fmt.bytes(it.toDouble()) },
			)
			Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
				Legend(c.memory, "Memory")
				if (m.swapTotal > 0) Legend(c.swap, "Swap")
			}
		}
	}
	item(key = "stats") {
		val c = Metric.colors
		Section("Details", ElIcons.Overview) {
			StatRow("Used", "${Fmt.bytes(m.used)} of ${Fmt.bytes(m.total)}")
			UsageBar(if (m.total > 0) m.used.toFloat() / m.total else 0f, c.memory, height = 10.dp)
			StatRow("Available", Fmt.bytes(m.available))
			if (m.swapTotal > 0) {
				StatRow("Swap", "${Fmt.bytes(m.swapUsed)} of ${Fmt.bytes(m.swapTotal)}")
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
			Section("Asleep", ElIcons.Moon, accent = MaterialTheme.colorScheme.secondary) {
				Text(
					"Powered down because nothing uses it. Elamon doesn't wake it to read it; it shows here when a program starts using it.",
					style = MaterialTheme.typography.bodyMedium,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
				)
			}
		} else {
			Section("Load", ElIcons.Gpu, accent = c.gpu, trailing = { g.usage?.let { RollingText(Fmt.percent(it), MaterialTheme.typography.titleLarge, color = c.gpu) } }) {
				LineChart(
					listOf(
						ChartSeries(state.hist.gpuUsage[id].orEmpty(), c.gpu, format = { Fmt.percent(it) }),
						ChartSeries(state.hist.gpuTemp[id].orEmpty(), c.hot, fill = false, format = { Fmt.temp(it, p.fahrenheit) }),
					),
					CAP, maxValue = 100f, height = 200.dp, label = { "100" },
				)
				Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
					Legend(c.gpu, "Load, %")
					Legend(c.hot, "Temperature, °C")
				}
				if (g.usage == null && info?.vendor == "intel") {
					Text(
						"Intel graphics don't report their load without extra privileges; the clock shows how hard it works.",
						style = MaterialTheme.typography.bodySmall,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
					)
				}
			}
		}
	}
	item(key = "stats") {
		val p = LocalPrefs.current
		Section("Details", ElIcons.Overview) {
			info?.let {
				StatRow("Model", it.name)
				StatRow("Driver", it.driver)
				StatRow("PCI slot", it.id)
			}
			g.temp?.let { StatRow("Temperature", Fmt.temp(it, p.fahrenheit), valueColor = tempColor(it)) }
			g.freq?.let { f -> StatRow("Clock", if (f == 0) "idle" else Fmt.mhz(f) + (g.freqMax?.let { " of ${Fmt.mhz(it)}" } ?: "")) }
			g.memFreq?.let { StatRow("Memory clock", Fmt.mhz(it)) }
			g.power?.let { StatRow("Power", Fmt.watts(it)) }
			g.fan?.let { StatRow("Fan", Fmt.percent(it.toFloat())) }
			if (g.memUsed != null && g.memTotal != null && g.memTotal > 0) {
				StatRow("Video memory", "${Fmt.bytes(g.memUsed)} of ${Fmt.bytes(g.memTotal)}")
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
		Section("Traffic", ElIcons.Network, accent = c.rx) {
			LineChart(
				listOf(
					ChartSeries(state.hist.rx, c.rx, format = { "↓ " + Fmt.rate(it.toDouble(), bits) }),
					ChartSeries(state.hist.tx, c.tx, format = { "↑ " + Fmt.rate(it.toDouble(), bits) }),
				),
				CAP, minScale = 64f * 1024, binary = true, height = 200.dp, label = { Fmt.rate(it.toDouble(), bits) },
			)
			Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
				Legend(c.rx, "Download")
				Legend(c.tx, "Upload")
			}
		}
	}
	for (n in s.net.filter { it.up || it.rx > 0 || it.tx > 0 }) {
		item(key = "if-" + n.name) {
			val c = Metric.colors
			val bits = LocalPrefs.current.networkBits
			val kind = when (n.kind) {
				"wired" -> "Cable"
				"wireless" -> "Wi-Fi"
				"vpn" -> "VPN"
				else -> "Virtual"
			}
			Section(n.name, ElIcons.Network, accent = if (n.kind == "vpn") MaterialTheme.colorScheme.tertiary else c.rx, trailing = { Pill(kind, MaterialTheme.colorScheme.secondary) }) {
				Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
					BigStat("↓ Now", Fmt.rate(n.rx, bits), color = c.rx)
					BigStat("↑ Now", Fmt.rate(n.tx, bits), color = c.tx)
				}
				StatRow("Received since boot", Fmt.bytes(n.rxTotal))
				StatRow("Sent since boot", Fmt.bytes(n.txTotal))
				n.speed?.let { StatRow("Link speed", "$it Mbit/s") }
				if (n.addrs.isNotEmpty()) StatRow("Addresses", n.addrs.filter { !it.startsWith("fe80") }.joinToString(", "))
			}
		}
	}
}

private fun LazyListScope.storage(state: MachineState) {
	val s = state.sample ?: return
	item(key = "io") {
		val c = Metric.colors
		Section("Throughput", ElIcons.Storage, accent = c.read) {
			LineChart(
				listOf(
					ChartSeries(state.hist.read, c.read, format = { "read " + Fmt.rate(it.toDouble()) }),
					ChartSeries(state.hist.write, c.write, format = { "write " + Fmt.rate(it.toDouble()) }),
				),
				CAP, minScale = 1024f * 1024, binary = true, height = 200.dp, label = { Fmt.rate(it.toDouble()) },
			)
			Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
				Legend(c.read, "Read")
				Legend(c.write, "Write")
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
					BigValue(Fmt.percent(frac * 100).dropLast(1), "%", large = false)
					Spacer(Modifier.weight(1f))
					Text("${Fmt.bytes(used)} of ${Fmt.bytes(d.total)}", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
				UsageBar(frac, loadColor(frac, c.read), height = 12.dp)
				StatRow("Free", Fmt.bytes(d.available))
				StatRow("Device", d.device)
			}
		}
	}
}

private fun LazyListScope.battery(state: MachineState) {
	val s = state.sample ?: return
	val b = s.battery ?: return
	item(key = "chart") {
		val c = Metric.colors
		Section("Power draw", ElIcons.Bolt, accent = c.battery) {
			LineChart(
				listOf(ChartSeries(state.hist.batteryPower, c.battery, format = { Fmt.watts(it) })),
				CAP, minScale = 5f, height = 180.dp, label = { Fmt.watts(it) },
			)
		}
	}
	item(key = "stats") {
		Section("Details", ElIcons.Battery) {
			b.percent?.let { StatRow("Charge", Fmt.percent(it)) }
			StatRow("State", batteryStatus(b.status))
			b.power?.let { StatRow("Power", Fmt.watts(it)) }
			b.timeLeft?.let { StatRow(if (b.status == "charging") "Until full" else "Left", Fmt.duration(it)) }
			b.energyNow?.let { StatRow("Energy now", "%.1f Wh".format(it)) }
			b.energyFull?.let { StatRow("Full charge", "%.1f Wh".format(it)) }
			b.energyDesign?.let { StatRow("Design capacity", "%.1f Wh".format(it)) }
			if (b.energyFull != null && b.energyDesign != null && b.energyDesign > 0) StatRow("Health", Fmt.percent(b.energyFull / b.energyDesign * 100))
			b.cycles?.let { StatRow("Charge cycles", it.toString()) }
			b.model?.let { StatRow("Model", it) }
		}
	}
}

private fun LazyListScope.topProcesses(procs: List<Proc>?, sort: String) {
	item(key = "top") {
		Section("Using the most", ElIcons.Processes) {
			if (procs == null) {
				Text("Loading…", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			} else {
				val top = when (sort) {
					"mem" -> procs.sortedByDescending { it.mem }
					"gpu" -> procs.filter { it.gpu != null || it.gpuMem != null }.sortedByDescending { (it.gpu ?: 0f) * 1e12f + (it.gpuMem ?: 0) }
					"io" -> procs.sortedByDescending { it.read + it.write }
					else -> procs.sortedByDescending { it.cpu }
				}.take(8)
				if (top.isEmpty()) {
					Text("Nothing uses it right now.", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
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
