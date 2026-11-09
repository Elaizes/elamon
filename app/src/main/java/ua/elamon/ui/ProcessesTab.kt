package ua.elamon.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import ua.elamon.MachineState
import ua.elamon.ProcRequest
import ua.elamon.R
import ua.elamon.data.Proc
import java.text.DateFormat
import java.util.Date

private data class ProcItem(val key: String, val name: String, val sub: String, val cpu: Float, val mem: Long, val gpu: Float?, val io: Double, val proc: Proc?)

@Composable
fun ProcessesTab(state: MachineState, padding: PaddingValues, setRequest: (ProcRequest?) -> Unit) {
	val ctx = LocalContext.current
	var sort by rememberSaveable { mutableStateOf("cpu") }
	var query by rememberSaveable { mutableStateOf("") }
	var grouped by rememberSaveable { mutableStateOf(false) }
	var selected by remember { mutableStateOf<Proc?>(null) }

	// Searching or grouping needs every process, not just the heaviest ones.
	val all = grouped || query.isNotBlank()
	LaunchedEffect(sort, all) { setRequest(ProcRequest(if (sort == "name") "cpu" else sort, all)) }
	DisposableEffect(Unit) { onDispose { setRequest(null) } }

	val procs = state.processes
	val rows = remember(procs, sort, query, grouped) {
		val list = procs.orEmpty().filter {
			query.isBlank() || it.name.contains(query, true) || it.cmd.contains(query, true) || it.pid.toString() == query.trim()
		}
		val rows = if (grouped) {
			list.groupBy { it.name }.map { (name, ps) ->
				ProcItem(
					key = "g:$name",
					name = name,
					sub = "×${ps.size}",
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

	Column(Modifier.fillMaxSize().padding(top = padding.calculateTopPadding())) {
		Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
			OutlinedTextField(
				value = query,
				onValueChange = { query = it },
				modifier = Modifier.fillMaxWidth(),
				singleLine = true,
				shape = RoundedCornerShape(28.dp),
				placeholder = { Text(stringResource(R.string.search_processes)) },
				leadingIcon = { Icon(Icons.Default.Search, null) },
				trailingIcon = {
					if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Default.Clear, stringResource(R.string.clear)) }
				},
			)
			Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
				for ((key, label) in listOf(
					"cpu" to R.string.sort_cpu,
					"mem" to R.string.sort_memory,
					"gpu" to R.string.sort_gpu,
					"io" to R.string.sort_io,
					"name" to R.string.sort_name,
				)) {
					FilterChip(selected = sort == key, onClick = { sort = key }, label = { Text(stringResource(label)) })
				}
				FilterChip(selected = grouped, onClick = { grouped = !grouped }, label = { Text(stringResource(R.string.group_by_app)) })
			}
			Text(
				if (procs == null) stringResource(R.string.loading) else stringResource(R.string.process_count, state.processCount),
				style = MaterialTheme.typography.labelMedium,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
			)
		}
		LazyColumn(
			Modifier.fillMaxSize(),
			contentPadding = PaddingValues(bottom = padding.calculateBottomPadding() + 16.dp),
		) {
			items(rows, key = { it.key }) { r ->
				ProcRow(r, onClick = r.proc?.let { p -> { selected = p } } ?: { query = r.name; grouped = false })
				HorizontalDivider(Modifier.padding(start = 16.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
			}
		}
	}

	selected?.let { p ->
		// Show the latest reading of the process while the sheet is open.
		val live = procs?.find { it.pid == p.pid } ?: p
		ProcessSheet(live, onDismiss = { selected = null })
	}
}

@Composable
private fun ProcRow(r: ProcItem, onClick: () -> Unit) {
	val ctx = LocalContext.current
	Row(
		Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		Column(Modifier.weight(1f)) {
			Text(r.name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
			Text(r.sub, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
		}
		r.gpu?.let {
			Text(
				stringResource(R.string.gpu_short, Fmt.percent(it)),
				style = MaterialTheme.typography.bodySmall,
				color = Metric.colors.gpu,
			)
			Spacer(Modifier.width(12.dp))
		}
		Column(horizontalAlignment = Alignment.End, modifier = Modifier.width(84.dp)) {
			Text(
				Fmt.percent(r.cpu, 1),
				style = MaterialTheme.typography.bodyLarge,
				fontWeight = FontWeight.SemiBold,
				color = if (r.cpu >= 1f) loadColor(r.cpu / 100f * 4) else MaterialTheme.colorScheme.onSurfaceVariant,
				textAlign = TextAlign.End,
			)
			Text(Fmt.bytes(ctx, r.mem), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
		}
	}
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProcessSheet(p: Proc, onDismiss: () -> Unit) {
	val ctx = LocalContext.current
	ModalBottomSheet(onDismissRequest = onDismiss) {
		Column(Modifier.padding(start = 24.dp, end = 24.dp, bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
			Text(p.name, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
			StatRow(stringResource(R.string.proc_pid), p.pid.toString())
			p.ppid?.let { StatRow(stringResource(R.string.proc_parent), it.toString()) }
			p.user?.let { StatRow(stringResource(R.string.proc_user), it) }
			StatRow(stringResource(R.string.proc_status), statusName(p.status))
			if (p.start > 0) {
				StatRow(stringResource(R.string.proc_started), DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(p.start * 1000)))
			}
			StatRow(stringResource(R.string.cpu), Fmt.percent(p.cpu, 1))
			StatRow(stringResource(R.string.memory), Fmt.bytes(ctx, p.mem))
			StatRow(stringResource(R.string.disk_read), Fmt.rate(ctx, p.read))
			StatRow(stringResource(R.string.disk_write), Fmt.rate(ctx, p.write))
			p.gpu?.let { StatRow(stringResource(R.string.gpu), Fmt.percent(it, 1)) }
			p.gpuMem?.let { StatRow(stringResource(R.string.gpu_memory), Fmt.bytes(ctx, it)) }
			if (p.cmd.isNotBlank()) {
				Text(stringResource(R.string.proc_command), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				SelectionContainer {
					Text(p.cmd, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
				}
			}
		}
	}
}

@Composable
private fun statusName(s: String): String = when (s.lowercase()) {
	"run", "runnable", "running" -> stringResource(R.string.proc_running)
	"sleep", "sleeping" -> stringResource(R.string.proc_sleeping)
	"idle" -> stringResource(R.string.proc_idle)
	"zombie" -> stringResource(R.string.proc_zombie)
	"stop", "stopped" -> stringResource(R.string.proc_stopped)
	"uninterruptibledisksleep", "uninterruptible disk sleep", "disk sleep" -> stringResource(R.string.proc_disk_wait)
	else -> s
}
