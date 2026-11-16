package io.github.elaizes.elamon.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import kotlinx.coroutines.delay
import io.github.elaizes.elamon.LogItem
import io.github.elaizes.elamon.LogLines
import io.github.elaizes.elamon.LogTone
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.DEFAULT_LOG_LEVEL
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.LogLine
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.takeCodePoints
import io.github.elaizes.elamon.logClock
import io.github.elaizes.elamon.logCopy
import io.github.elaizes.elamon.logLevels
import io.github.elaizes.elamon.logTone

/**
 * The computer's journal as it comes, in a card: its note, chips for the least level and a search
 * field (both filter it on the computer), then its lines, newest at the bottom, scrolling on their
 * own within [Sizes.logMax]. Older lines load at the top.
 */
@Composable
fun LogWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val log = scope.machine.logs[w.id]
	val own = w.level ?: DEFAULT_LOG_LEVEL
	val filter = scope.logs[w.id]
	val level = filter?.level ?: own
	WidgetSection(scope, w, accent(w.color), modifier) {
		log?.note?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = Metric.colors.warn) }
		Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
			for (l in logLevels(own)) {
				FilterChip(
					selected = l == level,
					onClick = { if (l != level) scope.vm.filterLog(w, level = l) },
					label = { Text(levelLabel(l)) },
					shape = PillShape,
				)
			}
		}
		if (w.search != false) LogSearch(scope, w, filter?.search.orEmpty())
		if (log == null) {
			Text(stringResource(R.string.loading), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		} else {
			LogList(scope, w, log)
		}
	}
}

/** The search field: told to the computer once typing stops a moment, as each search starts the lines anew. */
@Composable
private fun LogSearch(scope: DashScope, w: Widget, sent: String) {
	var query by rememberSaveable(w.id) { mutableStateOf(sent) }
	LaunchedEffect(query) {
		if (query.trim() == sent) return@LaunchedEffect
		delay(Limits.LOG_SEARCH_SETTLE_MS)
		scope.vm.filterLog(w, search = query)
	}
	OutlinedTextField(
		value = query,
		onValueChange = { query = it.takeCodePoints(Limits.LOG_SEARCH_MAX) },
		modifier = Modifier.fillMaxWidth(),
		singleLine = true,
		shape = RoundedCornerShape(Radius.field),
		placeholder = { Text(stringResource(R.string.log_search)) },
		leadingIcon = { Icon(Icons.Default.Search, null) },
		trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Default.Clear, stringResource(R.string.processes_clear)) } },
		colors = OutlinedTextFieldDefaults.colors(
			unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
			focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
			unfocusedBorderColor = MaterialTheme.colorScheme.surfaceContainerLow,
		),
	)
}

/**
 * The lines, in a list of their own. It follows the newest while it is at the bottom; scrolled up,
 * it stays where it is, and a chip takes it back down. A tap shows a line whole, a long press copies it.
 */
@Composable
private fun LogList(scope: DashScope, w: Widget, log: LogLines) {
	val ctx = LocalContext.current
	val haptics = LocalHapticFeedback.current
	val prefs = LocalPrefs.current
	val list = rememberLazyListState()
	var follow by remember { mutableStateOf(true) }
	// A finger moved the list since it last followed: where it comes to rest says whether it follows again.
	var moved by remember { mutableStateOf(false) }
	val followAgain = {
		follow = true
		moved = false
	}
	val open = remember { mutableStateMapOf<Long, Boolean>() }
	val older = log.canLoadOlder || log.loadingOlder
	// The list's last item: the newest line, after the row that loads older ones.
	val last = (if (older) 1 else 0) + log.items.size - 1
	val newest = log.items.lastOrNull()?.key
	// A finger on the list stops the following; once the list rests at the bottom, it follows again.
	// Its own scrolls to the newest don't count: a line that came meanwhile would stop it.
	LaunchedEffect(list) {
		list.interactionSource.interactions.collect {
			if (it is DragInteraction.Start) {
				follow = false
				moved = true
			}
		}
	}
	LaunchedEffect(list) {
		snapshotFlow { list.isScrollInProgress }.collect { scrolling -> if (!scrolling && moved && !list.canScrollForward) followAgain() }
	}
	// Lines that start anew (another filter, the stream back) are followed from their newest.
	LaunchedEffect(log.generation) { followAgain() }
	LaunchedEffect(newest, follow) {
		if (follow && newest != null) list.scrollToItem(last)
	}
	Box {
		LazyColumn(Modifier.fillMaxWidth().heightIn(max = Sizes.logMax), state = list, verticalArrangement = Arrangement.spacedBy(Space.xxs)) {
			// Older lines are there to be read: the newest no longer pull the list down.
			if (older) {
				item(key = OLDER) {
					OlderRow(log) {
						follow = false
						scope.vm.loadOlder(w)
					}
				}
			}
			if (log.items.isEmpty()) {
				item(key = EMPTY) { Text(stringResource(R.string.log_empty), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
			}
			items(log.items, key = { it.key }) { item ->
				when (item) {
					is LogItem.More -> Text(
						pluralStringResource(R.plurals.log_more, item.count, item.count),
						style = MaterialTheme.typography.labelSmall,
						color = MaterialTheme.colorScheme.onSurfaceVariant,
						modifier = Modifier.padding(start = Space.l, top = Space.xxs, bottom = Space.xxs),
					)
					is LogItem.Line -> LogLineRow(
						item.line,
						expanded = open[item.key] == true,
						onTap = { if (open.remove(item.key) == null) open[item.key] = true },
						onLongPress = {
							if (prefs.haptics) haptics.performHapticFeedback(HapticFeedbackType.LongPress)
							copy(ctx, logCopy(item.line))
						},
					)
				}
			}
		}
		if (!follow && newest != null) {
			FilledTonalButton(
				onClick = followAgain,
				modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = Space.s),
				contentPadding = PaddingValues(horizontal = Space.l),
			) {
				Icon(Icons.Default.KeyboardArrowDown, null, Modifier.size(Sizes.iconM))
				Spacer(Modifier.width(Space.xs))
				Text(stringResource(R.string.log_newest))
			}
		}
	}
}

/** Over the lines: the button that loads older ones, a spinner while they come, and why they didn't. */
@Composable
private fun OlderRow(log: LogLines, onLoad: () -> Unit) {
	Row(Modifier.fillMaxWidth().heightIn(min = Sizes.touchTarget), horizontalArrangement = Arrangement.spacedBy(Space.s, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
		if (log.loadingOlder) {
			CircularProgressIndicator(Modifier.size(Sizes.iconM), strokeWidth = Sizes.spinnerStroke)
			return@Row
		}
		if (log.olderFailed) Text(stringResource(R.string.log_older_failed), style = MaterialTheme.typography.bodySmall, color = Metric.colors.warn)
		TextButton(onClick = onLoad) { Text(stringResource(if (log.olderFailed) R.string.retry else R.string.log_older)) }
	}
}

/** A line on one line: its level's mark, its time, where it came from, and its text; [expanded], all of it. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LogLineRow(line: LogLine, expanded: Boolean, onTap: () -> Unit, onLongPress: () -> Unit) {
	val tone = logTone(line.level)
	val style = MaterialTheme.typography.bodySmall
	val variant = MaterialTheme.colorScheme.onSurfaceVariant
	val level = levelLabel(line.level)
	// The mark sits by the first line of text, however many it takes.
	val rowHeight = style.lineHeight.takeIf { it.isSp }?.let { with(LocalDensity.current) { it.toDp() } } ?: Sizes.iconS
	val source = line.unit ?: line.id
	val text = buildAnnotatedString {
		if (source != null) {
			withStyle(SpanStyle(color = variant, fontWeight = FontWeight.Medium)) { append(source) }
			append(' ')
		}
		append(line.text)
	}
	Row(
		Modifier.fillMaxWidth()
			.combinedClickable(onLongClickLabel = stringResource(R.string.copy), onLongClick = onLongPress, onClick = onTap)
			.padding(vertical = Space.xxs),
	) {
		Box(Modifier.height(rowHeight), contentAlignment = Alignment.Center) {
			Box(Modifier.size(Sizes.pillDot).clip(CircleShape).background(toneColor(tone)).semantics { contentDescription = level })
		}
		Spacer(Modifier.width(Space.xsPlus))
		Text(logClock(line.t), style = style, fontFamily = FontFamily.Monospace, color = variant)
		Spacer(Modifier.width(Space.s))
		Text(
			text,
			style = style,
			color = if (tone == LogTone.Dimmed) variant else MaterialTheme.colorScheme.onSurface,
			maxLines = if (expanded) Int.MAX_VALUE else 1,
			overflow = TextOverflow.Ellipsis,
			modifier = Modifier.weight(1f),
		)
	}
}

/** A level's mark: critical and error red, warning amber, notice the primary, info plain, debug faint. */
@Composable
private fun toneColor(tone: LogTone): Color = when (tone) {
	LogTone.Error -> Metric.colors.hot
	LogTone.Warning -> Metric.colors.warn
	LogTone.Notice -> MaterialTheme.colorScheme.primary
	LogTone.Normal -> MaterialTheme.colorScheme.onSurfaceVariant
	LogTone.Dimmed -> MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = Alpha.DISABLED)
}

@Composable
private fun levelLabel(level: String): String = stringResource(
	when (level) {
		"debug" -> R.string.log_level_debug
		"notice" -> R.string.log_level_notice
		"warning" -> R.string.log_level_warning
		"error" -> R.string.log_level_error
		"critical" -> R.string.log_level_critical
		else -> R.string.log_level_info
	},
)

/** Keys of the list's rows that aren't lines (theirs are numbers). */
private const val OLDER = "older"
private const val EMPTY = "empty"
