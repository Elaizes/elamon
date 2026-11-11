package io.github.elaizes.elamon.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.LogEntry

/** The page the Control tab's Activity row opens, as a detail page is named. */
const val ACTIVITY_PAGE = "activity"

/**
 * What phones asked the computer to do, newest first: what, which phone, how long ago and how
 * it ended. Read when the page opens and again from the bar's Refresh.
 */
@Composable
fun ActivityPage(vm: MachineViewModel, state: MachineState, list: LazyListState) {
	val activity by vm.activity.collectAsStateWithLifecycle()
	LaunchedEffect(Unit) { vm.refreshActivity() }
	val entries = activity.entries
	// Times are the service's: "now" is its clock as of the last sample, moved on since.
	val now = state.sample?.t?.let { it + (System.currentTimeMillis() - state.receivedAt) } ?: System.currentTimeMillis()
	LazyColumn(
		Modifier.fillMaxSize(),
		state = list,
		contentPadding = listPadding(),
		verticalArrangement = Arrangement.spacedBy(Space.s),
	) {
		item(key = "title") {
			BigTitle(
				stringResource(R.string.activity_title),
				list = list,
				subtitle = { Text(stringResource(R.string.activity_summary), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) },
			)
		}
		activity.error?.let { e ->
			item(key = "error") {
				Section(stringResource(R.string.activity_error_title), Icons.Default.Warning, accent = Metric.colors.warn) {
					Text(e.text(resources()), style = MaterialTheme.typography.bodyMedium)
					TextButton(onClick = { vm.refreshActivity() }) { Text(stringResource(R.string.retry)) }
				}
			}
		}
		when {
			entries == null && activity.loading -> item(key = "loading") {
				Box(Modifier.fillMaxWidth().padding(Space.xxl), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
			}
			entries == null -> {}
			entries.isEmpty() -> item(key = "empty") {
				Section(stringResource(R.string.activity_empty), ElIcons.Clock, accent = MaterialTheme.colorScheme.secondary) {
					Text(stringResource(R.string.activity_empty_body), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
				}
			}
			else -> items(entries) { e -> LogRow(e, now) }
		}
	}
}

/** One request: what, the phone and when beneath it, and how it ended on a tag; said as one line. */
@Composable
private fun LogRow(e: LogEntry, nowMs: Long) {
	val res = resources()
	val ago = agoWords(((nowMs - e.t) / MS_PER_SECOND).coerceAtLeast(0)).text(res)
	val result = logResult(e.result)
	val resultText = stringResource(result.words.id, *result.words.args.toTypedArray())
	val c = Metric.colors
	val color = when {
		result.ok -> c.good
		result.refused -> c.warn
		else -> c.hot
	}
	val parts = listOfNotNull(e.what, e.phone.ifBlank { null }, ago, resultText, e.message?.ifBlank { null })
	Surface(
		Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = parts.joinToString(res.getString(R.string.a11y_list_separator)) },
		shape = RoundedCornerShape(Radius.xl),
		color = MaterialTheme.colorScheme.surfaceContainerLow,
	) {
		Row(Modifier.padding(horizontal = Space.mPlus, vertical = Space.m), verticalAlignment = Alignment.CenterVertically) {
			LetterAvatar(e.phone.ifBlank { "?" }, Sizes.avatarS)
			Spacer(Modifier.width(Space.m))
			Column(Modifier.weight(1f)) {
				Text(e.what, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
				Text(
					listOfNotNull(e.phone.ifBlank { null }, ago).joinToString(" · "),
					style = MaterialTheme.typography.bodySmall,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
					maxLines = 1,
					overflow = TextOverflow.Ellipsis,
				)
				e.message?.takeIf { it.isNotBlank() }?.let {
					Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
				}
			}
			Spacer(Modifier.width(Space.sPlus))
			Pill(resultText, color, dot = true)
		}
	}
}

private const val MS_PER_SECOND = 1000L
