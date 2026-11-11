package io.github.elaizes.elamon.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.ProcRequest
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.orNull
import io.github.elaizes.elamon.data.Proc
import io.github.elaizes.elamon.data.ProtocolJson
import java.text.DateFormat
import java.util.Date

private data class ProcItem(val key: String, val name: String, val sub: String, val cpu: Float, val mem: Long, val gpu: Float?, val io: Double, val proc: Proc?)

@Composable
fun ProcessesTab(t: TabScope) {
	var sort by rememberSaveable { mutableStateOf("cpu") }
	var query by rememberSaveable { mutableStateOf("") }
	var grouped by rememberSaveable { mutableStateOf(false) }
	// The open sheet's process, its last readings with it: a rotation reopens it as it was.
	var selected by rememberSaveable(stateSaver = ProcSaver) { mutableStateOf<Proc?>(null) }
	val vm = t.vm

	// Searching or grouping needs every process, not just the heaviest ones.
	val all = grouped || query.isNotBlank()
	LaunchedEffect(sort, all) { vm.setProcesses(ProcRequest(if (sort == "name") "cpu" else sort, if (all) ProcRequest.ALL else ProcRequest.LIST)) }
	DisposableEffect(Unit) { onDispose { vm.setProcesses(null) } }

	val procs = t.state.processes
	val ncpu = t.state.info?.cpuLogicalCores ?: 1
	val res = resources()
	val rows = remember(procs, sort, query, grouped, res) {
		val list = procs.orEmpty().filter {
			query.isBlank() || it.name.contains(query, true) || it.cmd.contains(query, true) || it.pid.toString() == query.trim()
		}
		val rows = if (grouped) {
			list.groupBy { it.name }.map { (name, ps) ->
				ProcItem(
					key = "g:$name",
					name = name,
					sub = res.getQuantityString(R.plurals.processes_count, ps.size, ps.size),
					cpu = ps.sumOf { it.cpu.toDouble() }.toFloat(),
					mem = ps.sumOf { it.mem },
					gpu = ps.mapNotNull { it.gpu }.takeIf { it.isNotEmpty() }?.sum(),
					io = ps.sumOf { it.read + it.write },
					proc = ps.singleOrNull(),
				)
			}
		} else {
			list.map { ProcItem("p:${it.pid}", it.name, listOfNotNull(it.user, it.pid.toString()).joinToString(" · "), it.cpu, it.mem, it.gpu, it.read + it.write, it) }
		}
		when (sort) {
			"mem" -> rows.sortedByDescending { it.mem }
			"gpu" -> rows.sortedWith(compareByDescending<ProcItem> { it.gpu ?: -1f }.thenByDescending { it.cpu })
			"io" -> rows.sortedByDescending { it.io }
			"name" -> rows.sortedBy { it.name.lowercase() }
			else -> rows.sortedWith(compareByDescending<ProcItem> { it.cpu }.thenByDescending { it.mem })
		}
	}

	LazyColumn(Modifier.fillMaxSize(), state = t.list, contentPadding = listPadding()) {
		item(key = "header") { t.header() }
		item(key = "search") {
			Column(Modifier.padding(bottom = Space.s), verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
				OutlinedTextField(
					value = query,
					onValueChange = { query = it },
					modifier = Modifier.fillMaxWidth(),
					singleLine = true,
					shape = RoundedCornerShape(Radius.field),
					placeholder = { Text(stringResource(R.string.processes_search)) },
					leadingIcon = { Icon(Icons.Default.Search, null) },
					trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Default.Clear, stringResource(R.string.processes_clear)) } },
					colors = OutlinedTextFieldDefaults.colors(
						unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
						focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
						unfocusedBorderColor = MaterialTheme.colorScheme.surfaceContainerLow,
					),
				)
				Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
					for ((key, label) in listOf("cpu" to R.string.cpu, "mem" to R.string.memory, "gpu" to R.string.gpu, "io" to R.string.processes_sort_disk, "name" to R.string.processes_sort_name)) {
						FilterChip(selected = sort == key, onClick = { sort = key }, label = { Text(stringResource(label)) }, shape = PillShape)
					}
					FilterChip(selected = grouped, onClick = { grouped = !grouped }, label = { Text(stringResource(R.string.processes_by_app)) }, shape = PillShape)
				}
				Text(
					if (procs == null) stringResource(R.string.loading)
					else pluralStringResource(R.plurals.processes_count, t.state.processCount, t.state.processCount) + " · " +
						pluralStringResource(R.plurals.processes_cpu_share, ncpu, ncpu),
					style = MaterialTheme.typography.labelMedium,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
					modifier = Modifier.padding(start = Space.xs),
				)
			}
		}
		item(key = "list-top") { Spacer(Modifier.height(Space.xs)) }
		items(rows, key = { it.key }) { r ->
			ProcRow(r, onClick = r.proc?.let { p -> { selected = p } } ?: { query = r.name; grouped = false })
		}
	}

	// The sheet stays until it is closed: a process that leaves the list (it ended, or fell out of
	// the heaviest ones) keeps its last readings, marked as such. It is known by its pid and when it
	// started, so a new process given the same pid is another one.
	selected?.let { last ->
		val live = procs?.find { it.pid == last.pid && it.start == last.start }
		if (live != null && live != last) SideEffect { selected = live }
		ProcessSheet(t, live ?: last, gone = procs != null && live == null, onDismiss = { selected = null })
	}
}

@Composable
private fun ProcRow(r: ProcItem, onClick: () -> Unit) {
	val c = Metric.colors
	Surface(
		Modifier.fillMaxWidth().padding(vertical = Space.xsPlus / 2),
		shape = RoundedCornerShape(Radius.xl),
		color = MaterialTheme.colorScheme.surfaceContainerLow,
	) {
		Row(Modifier.clickable(onClick = onClick).padding(horizontal = Space.mPlus, vertical = Space.m), verticalAlignment = Alignment.CenterVertically) {
			LetterAvatar(r.name)
			Spacer(Modifier.width(Space.m))
			Column(Modifier.weight(1f)) {
				Text(r.name, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text(r.sub, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
			}
			Spacer(Modifier.width(Space.sPlus))
			Column(horizontalAlignment = Alignment.End, modifier = Modifier.width(Sizes.processFigures)) {
				Row(verticalAlignment = Alignment.CenterVertically) {
					r.gpu?.let {
						Text(stringResource(R.string.gpu_value, Fmt.percent(it)), style = MaterialTheme.typography.labelSmall, color = c.gpu)
						Spacer(Modifier.width(Space.xsPlus))
					}
					Text(Fmt.percent(r.cpu, 1), style = MaterialTheme.typography.titleSmall, color = if (r.cpu >= Thresholds.PROCESS_CPU_SHOWN) loadColor(processCpuFraction(r.cpu), c.cpu) else MaterialTheme.colorScheme.onSurfaceVariant)
				}
				Text(Fmt.bytes(r.mem), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				Spacer(Modifier.height(Space.xs))
				UsageBar(processCpuFraction(r.cpu).coerceIn(0f, 1f), loadColor(processCpuFraction(r.cpu), c.cpu), height = Sizes.barHair)
			}
		}
	}
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProcessSheet(t: TabScope, p: Proc, gone: Boolean, onDismiss: () -> Unit) {
	val guard = LocalGuard.current
	val res = resources()
	// Quitting or killing closes the sheet, once something has asked first.
	fun send(signal: String) {
		val kill = signal == "kill"
		val ask = Ask(
			Guarded.Processes,
			prompt = res.getString(if (kill) R.string.unlock_force_kill else R.string.unlock_quit, p.name, t.name),
			question = res.getString(if (kill) R.string.processes_force_kill_title else R.string.processes_quit_title, p.name),
			button = res.getString(if (kill) R.string.processes_force_kill else R.string.processes_quit),
			body = res.getString(if (kill) R.string.processes_force_kill_body else R.string.processes_quit_body),
		)
		guard.run(ask, afterAsking = onDismiss) { t.vm.signal(p.pid, p.start, signal) }
	}
	ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(start = Space.xxl, end = Space.xxl, bottom = Space.xxxl), verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				LetterAvatar(p.name, Sizes.avatarSheet)
				Spacer(Modifier.width(Space.mPlus))
				Column {
					Text(p.name, style = MaterialTheme.typography.headlineSmall)
					Text(listOfNotNull(p.user, stringResource(R.string.processes_pid, p.pid)).joinToString(" · "), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			if (gone) {
				Text(stringResource(R.string.processes_gone), style = MaterialTheme.typography.bodySmall, color = Metric.colors.warn)
			}
			Row(horizontalArrangement = Arrangement.spacedBy(Space.xxl), modifier = Modifier.padding(vertical = Space.xsPlus)) {
				BigStat(stringResource(R.string.cpu), Fmt.percent(p.cpu, 1), color = Metric.colors.cpu)
				BigStat(stringResource(R.string.memory), Fmt.bytes(p.mem), color = Metric.colors.memory)
				p.gpu?.let { BigStat(stringResource(R.string.gpu), Fmt.percent(it, 1), color = Metric.colors.gpu) }
			}
			p.ppid?.let { StatRow(stringResource(R.string.processes_parent_pid), it.toString()) }
			StatRow(stringResource(R.string.processes_state), statusName(p.status))
			if (p.start > 0) StatRow(stringResource(R.string.processes_started), DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(p.start * MS_PER_SECOND)))
			StatRow(stringResource(R.string.processes_disk_read), Fmt.rate(p.read))
			StatRow(stringResource(R.string.processes_disk_write), Fmt.rate(p.write))
			p.gpuMem?.let { StatRow(stringResource(R.string.processes_gpu_memory), Fmt.bytes(it)) }
			if (p.cmd.isNotBlank()) {
				Text(stringResource(R.string.processes_command), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				SelectionContainer {
					Text(p.cmd, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
				}
			}
			// Quitting and killing need the admin role and control on.
			if (t.perms.signal) {
				Row(horizontalArrangement = Arrangement.spacedBy(Space.sPlus), modifier = Modifier.padding(top = Space.s)) {
					FilledTonalButton(onClick = { send("term") }, modifier = Modifier.weight(1f)) {
						Text(stringResource(R.string.processes_quit))
					}
					FilledTonalButton(onClick = { send("kill") }, modifier = Modifier.weight(1f)) {
						Text(stringResource(R.string.processes_force_kill), color = Metric.colors.hot)
					}
				}
			}
		}
	}
}

@Composable
private fun statusName(s: String): String = when (s.lowercase()) {
	"run", "runnable", "running" -> stringResource(R.string.process_running)
	"sleep", "sleeping" -> stringResource(R.string.process_sleeping)
	"idle" -> stringResource(R.string.process_idle)
	"zombie" -> stringResource(R.string.process_zombie)
	"stop", "stopped" -> stringResource(R.string.process_stopped)
	"uninterruptibledisksleep", "uninterruptible disk sleep", "disk sleep" -> stringResource(R.string.process_disk_sleep)
	else -> s.lowercase()
}

private const val MS_PER_SECOND = 1000L

/** A process kept across a rotation, in the service's own JSON for it. */
private val ProcSaver = Saver<Proc?, String>(
	save = { p -> p?.let { ProtocolJson.encodeToString(Proc.serializer(), it) } },
	restore = { orNull { ProtocolJson.decodeFromString(Proc.serializer(), it) } },
)
