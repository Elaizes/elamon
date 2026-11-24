package io.github.elaizes.elamon.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import io.github.elaizes.elamon.Conn
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.Player
import io.github.elaizes.elamon.data.Volume
import io.github.elaizes.elamon.data.Widget

/**
 * The players, with covers and seeking, and the volume, as 0.5's Control tab had them: the
 * buttons run the `media.*` and `volume.set` actions, as far as the computer offers them.
 */
@Composable
fun MediaWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val players = s?.players.orEmpty()
	var chosen by remember { mutableStateOf<String?>(null) }
	val player = players.find { it.id == chosen } ?: players.firstOrNull()
	val padding = cardPadding()
	CardSurface(modifier) {
		Column(Modifier.padding(padding), verticalArrangement = Arrangement.spacedBy(Space.m)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				IconBadge(icon(w.icon ?: "music"), if (w.color != null) accent(w.color) else MaterialTheme.colorScheme.tertiary)
				Spacer(Modifier.width(Space.m))
				Text(player?.name ?: w.label ?: stringResource(R.string.control_media), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
			}
			if (players.size > 1) {
				ChipRow(padding) {
					for (p in players) FilterChip(selected = p.id == player?.id, onClick = { chosen = p.id }, label = { Text(p.name) }, shape = PillShape)
				}
			}
			when {
				player != null -> NowPlaying(scope, player)
				s != null -> Text(stringResource(R.string.control_nothing_playing), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
			s?.volume?.let { VolumeRow(scope, it) }
		}
	}
}

@Composable
private fun NowPlaying(scope: DashScope, p: Player) {
	val a = scope.actions
	val playPause = a.action(PLAY_PAUSE)
	val next = a.action(NEXT)
	val previous = a.action(PREVIOUS)
	val seek = a.action(SEEK)
	val artPx = with(LocalDensity.current) { Sizes.art.roundToPx() }
	var art by remember(p.id, p.title) { mutableStateOf<Bitmap?>(null) }
	LaunchedEffect(p.id, p.title, p.hasArt) { art = if (p.hasArt) scope.vm.art(p.id, artPx) else null }
	val playing = p.status == "Playing"
	// The service reports once a line, maybe seconds apart; in between the position moves on by itself.
	val moving = playing && scope.machine.conn == Conn.Live
	var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
	LaunchedEffect(moving) {
		while (moving) {
			now = System.currentTimeMillis()
			delay(Limits.MEDIA_TICK_MS)
		}
	}
	val player = buildJsonObject { put("player", p.id) }
	Row(verticalAlignment = Alignment.CenterVertically) {
		Box(
			Modifier.size(Sizes.art).clip(InnerShape).background(
				Brush.verticalGradient(listOf(MaterialTheme.colorScheme.tertiaryContainer, MaterialTheme.colorScheme.primaryContainer))
			),
			contentAlignment = Alignment.Center,
		) {
			val bitmap = art
			if (bitmap != null) Image(bitmap.asImageBitmap(), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
			else Icon(ElIcons.Music, null, tint = MaterialTheme.colorScheme.onTertiaryContainer, modifier = Modifier.size(Sizes.iconXl))
		}
		Spacer(Modifier.width(Space.m))
		Column(Modifier.weight(1f)) {
			Text(p.title ?: stringResource(if (p.status == "Stopped") R.string.control_nothing_playing else R.string.control_unknown_track), style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
			listOfNotNull(p.artist, p.album).joinToString(" · ").takeIf { it.isNotEmpty() }?.let {
				Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
			}
		}
	}
	val length = p.length
	if (length != null && length > 0) {
		// A new line may be newer than the last tick: it is where the position starts again.
		val pos = playbackPosition(p.position, length, scope.machine.receivedAt, maxOf(now, scope.machine.receivedAt), moving)
		var dragging by remember { mutableStateOf<Float?>(null) }
		Column {
			Slider(
				value = dragging ?: (pos / length).toFloat(),
				onValueChange = { dragging = it },
				onValueChangeFinished = {
					val f = dragging
					dragging = null
					if (f != null && seek != null) {
						a.launch(seek, buildJsonObject { put("player", p.id); put("position", Params0.seconds(f * length)) }, seek.name)
					}
				},
				enabled = p.canSeek && a.enabled(seek, null),
			)
			Row {
				Text(Fmt.clock(pos), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
				Spacer(Modifier.weight(1f))
				Text(Fmt.clock(length), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
	if (playPause != null || next != null || previous != null) {
		Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly, verticalAlignment = Alignment.CenterVertically) {
			IconButton(onClick = { previous?.let { a.launch(it, player, it.name) } }, enabled = p.canPrevious && a.enabled(previous, null)) {
				Icon(ElIcons.Previous, stringResource(R.string.control_previous))
			}
			FilledIconButton(
				onClick = { playPause?.let { a.launch(it, player, it.name) } },
				enabled = (p.canPlay || p.canPause) && a.enabled(playPause, null),
				modifier = Modifier.size(Sizes.playButton),
			) {
				Icon(if (playing) ElIcons.Pause else ElIcons.Play, stringResource(if (playing) R.string.control_pause else R.string.control_play), modifier = Modifier.size(Sizes.iconXl))
			}
			IconButton(onClick = { next?.let { a.launch(it, player, it.name) } }, enabled = p.canNext && a.enabled(next, null)) {
				Icon(ElIcons.Next, stringResource(R.string.control_next))
			}
		}
	}
}

/** Seconds as the service takes a position: a number, to the tenth. */
private object Params0 {
	fun seconds(v: Double): JsonPrimitive = JsonPrimitive(Math.round(v * TENTHS) / TENTHS)
	private const val TENTHS = 10.0
}

@Composable
private fun VolumeRow(scope: DashScope, v: Volume) {
	val a = scope.actions
	val set = a.action(VOLUME)
	val canSet = a.enabled(set, null)
	var dragging by remember { mutableStateOf<Float?>(null) }
	val level = (v.level / Thresholds.PERCENT).toFloat().coerceIn(0f, 1f)
	Row(verticalAlignment = Alignment.CenterVertically) {
		// The icon, not its button, where the rest of the card starts, as far from the slider as its figure.
		IconButton(onClick = { set?.let { a.launch(it, buildJsonObject { put("muted", !v.muted) }, it.name) } }, enabled = canSet, modifier = Modifier.flush(Sizes.buttonEmpty)) {
			Icon(
				if (v.muted) ElIcons.Mute else ElIcons.Volume,
				stringResource(if (v.muted) R.string.control_unmute else R.string.control_mute),
				tint = if (v.muted) Metric.colors.hot else MaterialTheme.colorScheme.onSurface,
			)
		}
		Spacer(Modifier.width(Space.m))
		Slider(
			value = dragging ?: level,
			onValueChange = { dragging = it },
			onValueChangeFinished = {
				val f = dragging
				dragging = null
				if (f != null && set != null) a.launch(set, buildJsonObject { put("level", Math.round(f * Thresholds.PERCENT)) }, set.name)
			},
			enabled = canSet,
			modifier = Modifier.weight(1f),
		)
		// As wide as its longest, so the slider doesn't change with it.
		val style = MaterialTheme.typography.labelLarge
		val measurer = rememberTextMeasurer()
		val widest = with(LocalDensity.current) { remember(measurer, style) { measurer.measure(Fmt.percent(Thresholds.PERCENT.toDouble()), style).size.width }.toDp() }
		Text(Fmt.percent(((dragging ?: level) * Thresholds.PERCENT).toDouble()), style = style, textAlign = TextAlign.End, maxLines = 1, modifier = Modifier.padding(start = Space.m).widthIn(min = widest))
	}
}

private const val PLAY_PAUSE = "media.play_pause"
private const val NEXT = "media.next"
private const val PREVIOUS = "media.previous"
private const val SEEK = "media.seek"
private const val VOLUME = "volume.set"
