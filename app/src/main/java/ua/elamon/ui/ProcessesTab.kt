package ua.elamon.ui

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
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import ua.elamon.ProcRequest
import ua.elamon.R
import ua.elamon.data.Proc
import java.text.DateFormat
import java.util.Date

private data class ProcItem(val key: String, val name: String, val sub: String, val cpu: Float, val mem: Long, val gpu: Float?, val io: Double, val proc: Proc?)

@Composable
fun ProcessesTab(t: TabScope) {
	var sort by rememberSaveable { mutableStateOf("cpu") }
	var query by rememberSaveable { mutableStateOf("") }
	var grouped by rememberSaveable { mutableStateOf(false) }
	var selected by remember { mutableStateOf<Int?>(null) }
	val vm = t.vm

	// Searching or grouping needs every process, not just the heaviest ones.
	val all = grouped || query.isNotBlank()
	LaunchedEffect(sort, all) { vm.setProcesses(ProcRequest(if (sort == "name") "cpu" else sort, all)) }
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
			Column(Modifier.padding(bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
				OutlinedTextField(
					value = query,
					onValueChange = { query = it },
					modifier = Modifier.fillMaxWidth(),
					singleLine = true,
					shape = RoundedCornerShape(28.dp),
					placeholder = { Text(stringResource(R.string.processes_search)) },
					leadingIcon = { Icon(Icons.Default.Search, null) },
					trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Default.Clear, stringResource(R.string.processes_clear)) } },
					colors = OutlinedTextFieldDefaults.colors(
						unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
						focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
						unfocusedBorderColor = MaterialTheme.colorScheme.surfaceContainerLow,
					),
				)
				Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
					for ((key, label) in listOf("cpu" to R.string.cpu, "mem" to R.string.memory, "gpu" to R.string.gpu, "io" to R.string.processes_sort_disk, "name" to R.string.processes_sort_name)) {
						FilterChip(selected = sort == key, onClick = { sort = key }, label = { Text(stringResource(label)) }, shape = RoundedCornerShape(50))
					}
					FilterChip(selected = grouped, onClick = { grouped = !grouped }, label = { Text(stringResource(R.string.processes_by_app)) }, shape = RoundedCornerShape(50))
				}
				Text(
					if (procs == null) stringResource(R.string.loading)
					else pluralStringResource(R.plurals.processes_count, t.state.processCount, t.state.processCount) + " · " +
						pluralStringResource(R.plurals.processes_cpu_share, ncpu, ncpu),
					style = MaterialTheme.typography.labelMedium,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
					modifier = Modifier.padding(start = 4.dp),
				)
			}
		}
		item(key = "list-top") { Spacer(Modifier.height(4.dp)) }
		items(rows, key = { it.key }) { r ->
			ProcRow(r, onClick = r.proc?.let { p -> { selected = p.pid } } ?: { query = r.name; grouped = false })
		}
	}

	selected?.let { pid ->
		val live = procs?.find { it.pid == pid }
		if (live != null) ProcessSheet(t, live, onDismiss = { selected = null }) else LaunchedEffect(pid) { selected = null }
	}
}

@Composable
private fun ProcRow(r: ProcItem, onClick: () -> Unit) {
	val c = Metric.colors
	Surface(
		Modifier.fillMaxWidth().padding(vertical = 3.dp),
		shape = RoundedCornerShape(20.dp),
		color = MaterialTheme.colorScheme.surfaceContainerLow,
	) {
		Row(Modifier.clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
			LetterAvatar(r.name)
			Spacer(Modifier.width(12.dp))
			Column(Modifier.weight(1f)) {
				Text(r.name, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text(r.sub, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
			}
			Spacer(Modifier.width(10.dp))
			Column(horizontalAlignment = Alignment.End, modifier = Modifier.width(96.dp)) {
				Row(verticalAlignment = Alignment.CenterVertically) {
					r.gpu?.let {
						Text(stringResource(R.string.gpu_value, Fmt.percent(it)), style = MaterialTheme.typography.labelSmall, color = c.gpu)
						Spacer(Modifier.width(6.dp))
					}
					Text(Fmt.percent(r.cpu, 1), style = MaterialTheme.typography.titleSmall, color = if (r.cpu >= 1f) loadColor(r.cpu / 25f, c.cpu) else MaterialTheme.colorScheme.onSurfaceVariant)
				}
				Text(Fmt.bytes(r.mem), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				Spacer(Modifier.height(4.dp))
				UsageBar((r.cpu / 25f).coerceIn(0f, 1f), loadColor(r.cpu / 25f, c.cpu), height = 4.dp)
			}
		}
	}
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProcessSheet(t: TabScope, p: Proc, onDismiss: () -> Unit) {
	val control by t.vm.control.collectAsStateWithLifecycle()
	val prefs = LocalPrefs.current
	var confirm by remember { mutableStateOf<String?>(null) }
	ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(start = 24.dp, end = 24.dp, bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				LetterAvatar(p.name, 48.dp)
				Spacer(Modifier.width(14.dp))
				Column {
					Text(p.name, style = MaterialTheme.typography.headlineSmall)
					Text(listOfNotNull(p.user, stringResource(R.string.processes_pid, p.pid)).joinToString(" · "), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			Row(horizontalArrangement = Arrangement.spacedBy(24.dp), modifier = Modifier.padding(vertical = 6.dp)) {
				BigStat(stringResource(R.string.cpu), Fmt.percent(p.cpu, 1), color = Metric.colors.cpu)
				BigStat(stringResource(R.string.memory), Fmt.bytes(p.mem), color = Metric.colors.memory)
				p.gpu?.let { BigStat(stringResource(R.string.gpu), Fmt.percent(it, 1), color = Metric.colors.gpu) }
			}
			p.ppid?.let { StatRow(stringResource(R.string.processes_parent_pid), it.toString()) }
			StatRow(stringResource(R.string.processes_state), statusName(p.status))
			if (p.start > 0) StatRow(stringResource(R.string.processes_started), DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(p.start * 1000)))
			StatRow(stringResource(R.string.processes_disk_read), Fmt.rate(p.read))
			StatRow(stringResource(R.string.processes_disk_write), Fmt.rate(p.write))
			p.gpuMem?.let { StatRow(stringResource(R.string.processes_gpu_memory), Fmt.bytes(it)) }
			if (p.cmd.isNotBlank()) {
				Text(stringResource(R.string.processes_command), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				SelectionContainer {
					Text(p.cmd, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
				}
			}
			if (control?.enabled == true) {
				Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.padding(top = 8.dp)) {
					FilledTonalButton(onClick = { if (prefs.confirmActions) confirm = "term" else t.vm.signal(p.pid, "term") }, modifier = Modifier.weight(1f)) {
						Text(stringResource(R.string.processes_quit))
					}
					FilledTonalButton(onClick = { if (prefs.confirmActions) confirm = "kill" else t.vm.signal(p.pid, "kill") }, modifier = Modifier.weight(1f)) {
						Text(stringResource(R.string.processes_force_kill), color = Metric.colors.hot)
					}
				}
			}
		}
	}
	confirm?.let { sig ->
		GlassDialog(
			onDismiss = { confirm = null },
			title = stringResource(if (sig == "kill") R.string.processes_force_kill_title else R.string.processes_quit_title, p.name),
			text = {
				Text(
					stringResource(if (sig == "kill") R.string.processes_force_kill_body else R.string.processes_quit_body),
				)
			},
			confirm = {
				TextButton(onClick = {
					t.vm.signal(p.pid, sig)
					confirm = null
					onDismiss()
				}) { Text(stringResource(if (sig == "kill") R.string.processes_force_kill else R.string.processes_quit)) }
			},
			dismiss = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
		)
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
