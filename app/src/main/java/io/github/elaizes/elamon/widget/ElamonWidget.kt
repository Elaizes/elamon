package io.github.elaizes.elamon.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.text.format.DateUtils
import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.appWidgetBackground
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextAlign
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import java.text.DateFormat
import kotlin.math.roundToInt
import io.github.elaizes.elamon.MainActivity
import io.github.elaizes.elamon.Prefs
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Scale
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.ThemeMode
import io.github.elaizes.elamon.data.Grid
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.ui.MetricColors
import io.github.elaizes.elamon.ui.Radius
import io.github.elaizes.elamon.ui.Sizes
import io.github.elaizes.elamon.ui.Space
import io.github.elaizes.elamon.ui.accentOf
import io.github.elaizes.elamon.ui.appColorScheme
import io.github.elaizes.elamon.ui.color
import io.github.elaizes.elamon.ui.levelOf
import io.github.elaizes.elamon.ui.metricColors
import io.github.elaizes.elamon.with
import androidx.glance.color.ColorProvider as DayNight

/**
 * The home-screen widget: the dashboard the profile's `widget` names, of the computer chosen for it
 * (or the one paired): its gauges as small rings, the rest as label and value lines; when it was
 * read, and what it is when it can't be. A tap reads it at once; its name opens the computer in the
 * app.
 */
class ElamonWidget : GlanceAppWidget() {
	/** Drawn for the size it is: as many rings and lines as fit. */
	override val sizeMode: SizeMode = SizeMode.Exact

	override suspend fun provideGlance(context: Context, id: GlanceId) {
		val appWidgetId = GlanceAppWidgetManager(context).getAppWidgetId(id)
		val store = MachineStore.get(context)
		val widgets = WidgetStore.get(context)
		val cache = UiCache.get(context)
		val settings = Settings.get(context)
		provideContent {
			val paired by store.machines.collectAsState()
			val shown by widgets.shown.collectAsState()
			val reading by widgets.reading.collectAsState()
			val profiles by cache.profiles.collectAsState()
			val look by settings.look.collectAsState()
			val chosen by settings.widgets.collectAsState()
			val machine = remember(paired, chosen) { shownMachine(settings.widgetMachine(appWidgetId), paired) }
			// As the app looks with this computer: its profile's theme, or the phone's own over it.
			val prefs = (machine?.let { profiles[it.id] }?.let(Prefs::of) ?: Prefs()).with(look.theme)
			val colors = remember(prefs) { WidgetColors.of(context, prefs) }
			Content(machine, machine?.let { shown[it.id] } ?: WidgetShown(), machine?.id in reading, colors, Scale.factor(look.scale))
		}
	}
}

/** Puts the widget on the home screen's list, and keeps its readings' work as the widgets on it need. */
class ElamonWidgetReceiver : GlanceAppWidgetReceiver() {
	override val glanceAppWidget: GlanceAppWidget = ElamonWidget()

	override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
		super.onUpdate(context, appWidgetManager, appWidgetIds)
		WidgetWork.sync(context)
		// Added, or the phone restarted: what is older than its pace is read at once.
		WidgetWork.catchUp(context)
	}

	override fun onDeleted(context: Context, appWidgetIds: IntArray) {
		super.onDeleted(context, appWidgetIds)
		Settings.get(context).forgetWidgets(appWidgetIds)
		WidgetWork.sync(context)
	}
}

/** A tap on the widget: its computer read at once. */
class RefreshWidget : ActionCallback {
	override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
		val machine = parameters[MACHINE] ?: return
		WidgetWork.now(context, machine)
	}
}

/** The computer a widget's action is for. */
private val MACHINE = ActionParameters.Key<String>("machine")

/** What a widget's line says, in the colours of the app's theme (day and night where it follows the phone). */
private class WidgetColors(
	private val mode: ThemeMode,
	private val light: ColorScheme,
	private val dark: ColorScheme,
	private val lightMetric: MetricColors,
	private val darkMetric: MetricColors,
) {
	private fun of(day: Color, night: Color): ColorProvider = when (mode) {
		ThemeMode.System -> DayNight(day = day, night = night)
		ThemeMode.Light -> ColorProvider(day)
		ThemeMode.Dark -> ColorProvider(night)
	}

	private fun scheme(pick: (ColorScheme) -> Color) = of(pick(light), pick(dark))

	val background = scheme { it.surfaceContainerLow }
	val text = scheme { it.onSurface }
	val quiet = scheme { it.onSurfaceVariant }

	/** A ring's empty part, as the app's. */
	val track = scheme { it.surfaceContainerHighest }

	/** What was read before, shown while the computer can't be reached: faded. */
	val old = scheme { it.onSurfaceVariant.copy(alpha = OLD_ALPHA) }

	/** A line's value by its level, as its tile colours it: green or the text's own while normal, turning amber, then red. */
	fun level(line: WidgetLine): ColorProvider {
		val band = levelOf(line.level, line.blend) ?: return text
		fun pick(s: ColorScheme, m: MetricColors) = band.color(if (line.green) m.good else s.onSurface, m.warn, m.hot)
		return of(pick(light, lightMetric), pick(dark, darkMetric))
	}

	/** A gauge's ring by its level, as its tile colours it: the gauge's own colour while normal, turning amber, then red. */
	fun ring(line: WidgetLine): ColorProvider {
		fun pick(s: ColorScheme, m: MetricColors): Color {
			val own = accentOf(line.color, s, m)
			return levelOf(line.level, line.blend)?.color(own, m.warn, m.hot) ?: own
		}
		return of(pick(light, lightMetric), pick(dark, darkMetric))
	}

	companion object {
		private const val OLD_ALPHA = 0.6f

		fun of(context: Context, prefs: Prefs) = WidgetColors(
			prefs.theme,
			appColorScheme(context, prefs, dark = false),
			appColorScheme(context, prefs, dark = true),
			metricColors(false),
			metricColors(true),
		)
	}
}

/** A line's height and the name's at 100%, for how many fit; the sizes of their text. */
private const val LINE_DP = 20f
private const val HEADER_DP = 30f
private const val TEXT_SP = 14f
private const val SMALL_SP = 12f
private const val TITLE_SP = 15f

/** Narrower than this at 100%, a widget has little room for its name. */
private val NARROW = 180.dp

/** The widget's corner: the home screen's own where it has one (Android 12), or a card's. */
private fun GlanceModifier.widgetCorner(): GlanceModifier =
	if (Build.VERSION.SDK_INT >= 31) cornerRadius(android.R.dimen.system_app_widget_background_radius) else cornerRadius(Radius.card)

@Composable
private fun Content(machine: Machine?, shown: WidgetShown, reading: Boolean, c: WidgetColors, scale: Float) {
	val ctx = LocalContext.current
	val open = actionStartActivity(Intent(ctx, MainActivity::class.java).apply { machine?.let { putExtra(MainActivity.EXTRA_MACHINE, it.id) } })
	val tap = machine?.let { actionRunCallback<RefreshWidget>(actionParametersOf(MACHINE to it.id)) } ?: open
	val body = TextStyle(color = c.text, fontSize = (TEXT_SP * scale).sp)
	val small = TextStyle(color = c.quiet, fontSize = (SMALL_SP * scale).sp)
	Column(
		GlanceModifier.fillMaxSize().appWidgetBackground().widgetCorner().background(c.background).padding(Space.m).clickable(tap),
	) {
		// Its name and when it was read; the name opens it in the app.
		Row(GlanceModifier.fillMaxWidth().clickable(open), verticalAlignment = Alignment.CenterVertically) {
			Text(
				machine?.name ?: ctx.getString(R.string.app_name),
				GlanceModifier.defaultWeight(),
				TextStyle(color = c.text, fontSize = (TITLE_SP * scale).sp, fontWeight = FontWeight.Bold),
				maxLines = 1,
			)
			note(ctx, shown, reading, narrow = LocalSize.current.width < NARROW * scale)?.let {
				Spacer(GlanceModifier.width(Space.s))
				Text(it, style = small, maxLines = 1)
			}
		}
		Spacer(GlanceModifier.height(Space.s))
		val words = message(ctx, machine, shown, reading)
		if (words != null) {
			Text(words, style = body.copy(color = c.quiet))
			return@Column
		}
		val old = shown.fetch != Fetch.Ok
		val size = LocalSize.current
		val layout = widgetLayout(shown.lines, size.width.value, size.height.value, scale)
		val gap = (RING_GAP_DP * scale).dp
		// Rings, with the lines under them, sit in the middle of the room they have; lines alone start
		// under the name, as before rings.
		Column(
			GlanceModifier.fillMaxWidth().defaultWeight(),
			verticalAlignment = if (layout.rings.isEmpty()) Alignment.Top else Alignment.CenterVertically,
		) {
			layout.rings.forEachIndexed { i, row ->
				if (i > 0) Spacer(GlanceModifier.height(gap))
				// Each in the middle of an even share of the width.
				Row(GlanceModifier.fillMaxWidth()) {
					for (line in row) {
						Box(GlanceModifier.defaultWeight(), contentAlignment = Alignment.Center) { Ring(line, layout.ringDp, old, c) }
					}
				}
			}
			if (layout.rings.isNotEmpty() && layout.lines.isNotEmpty()) Spacer(GlanceModifier.height(gap))
			for (line in layout.lines) Line(line, old, c, scale)
		}
	}
}

/** A gauge as a small ring, as on its tile: its value inside, its label under that. */
@Composable
private fun Ring(line: WidgetLine, dp: Float, old: Boolean, c: WidgetColors) {
	val density = LocalContext.current.resources.displayMetrics.density
	val px = (dp * density).roundToInt().coerceIn(1, RING_MAX_PX)
	val fill = remember(line.ring, px) { ringFill(line.ring ?: 0f, px) }
	Box(GlanceModifier.size(dp.dp), contentAlignment = Alignment.Center) {
		Image(ImageProvider(R.drawable.widget_ring_track), null, GlanceModifier.fillMaxSize(), colorFilter = ColorFilter.tint(c.track))
		Image(ImageProvider(fill), null, GlanceModifier.fillMaxSize(), colorFilter = ColorFilter.tint(if (old) c.old else c.ring(line)))
		Column(horizontalAlignment = Alignment.CenterHorizontally) {
			// Each no wider than the ring is there: a label longer still ends in "…", not over the ring.
			RingText(line.text, dp, RING_VALUE, RING_VALUE_ROOM, RING_VALUE_MIN, TextStyle(color = if (old) c.old else c.text, fontWeight = FontWeight.Bold))
			RingText(line.label, dp, RING_LABEL, RING_LABEL_ROOM, RING_LABEL_MIN, TextStyle(color = if (old) c.old else c.quiet))
		}
	}
}

@Composable
private fun RingText(text: String, dp: Float, share: Float, room: Float, min: Float, style: TextStyle) {
	// As large as its ring, not as the phone's text.
	val size = ringText(text, dp, share, room, min) / LocalContext.current.resources.configuration.fontScale
	// On a line as tall whatever its size: rings side by side have theirs at one height.
	Box(GlanceModifier.width((dp * room).dp).height((dp * share * RING_LINE).dp), contentAlignment = Alignment.Center) {
		Text(text, GlanceModifier.fillMaxWidth(), style.copy(fontSize = size.sp, textAlign = TextAlign.Center), maxLines = 1)
	}
}

@Composable
private fun Line(line: WidgetLine, old: Boolean, c: WidgetColors, scale: Float) {
	val size = (TEXT_SP * scale).sp
	if (line.words) {
		Text(line.text, GlanceModifier.fillMaxWidth().padding(vertical = Sizes.widgetLine), TextStyle(color = if (old) c.old else c.level(line), fontSize = size), maxLines = 2)
		return
	}
	Row(GlanceModifier.fillMaxWidth().padding(vertical = Sizes.widgetLine), verticalAlignment = Alignment.CenterVertically) {
		Text(line.label, GlanceModifier.defaultWeight(), TextStyle(color = if (old) c.old else c.quiet, fontSize = size), maxLines = 1)
		Spacer(GlanceModifier.width(Space.s))
		Text(line.text, style = TextStyle(color = if (old) c.old else c.level(line), fontSize = size, fontWeight = FontWeight.Medium), maxLines = 1)
	}
}

/** How many lines fit in a widget [heightDp] tall at the app's [scale]: one at least. */
internal fun linesThatFit(heightDp: Float, scale: Float): Int =
	((heightDp - 2 * Space.m.value - HEADER_DP * scale) / (LINE_DP * scale)).toInt().coerceAtLeast(1)

/** A ring's least and most size at 100%, and the room between rings, in dp. */
internal const val RING_MIN_DP = 48f
internal const val RING_MAX_DP = 84f
internal const val RING_GAP_DP = 8f

/** A ring's picture is drawn no larger than this: what a widget sends the home screen is kept small. */
private const val RING_MAX_PX = 256

/** A ring's value and label, of its width, and a line of them, of their size. */
private const val RING_VALUE = 0.24f
private const val RING_LABEL = 0.16f
private const val RING_LINE = 1.4f

/** How much of a ring's width is between its sides where its value is, and where its label is, nearer its open end. */
internal const val RING_VALUE_ROOM = 0.68f
internal const val RING_LABEL_ROOM = 0.56f

/** How wide a letter is in its size, and the least size, in dp, a ring's value and label get: a value is whole, small as it may be. */
internal const val LETTER = 0.62f
internal const val RING_VALUE_MIN = 5f
internal const val RING_LABEL_MIN = 7f

/** The size, in dp, of [text] in a ring [dp] across: its [share] of the ring, smaller where that would be wider than its [room], down to [min]. */
internal fun ringText(text: String, dp: Float, share: Float, room: Float, min: Float): Float =
	minOf(dp * share, dp * room / (LETTER * text.length.coerceAtLeast(1))).coerceAtLeast(min)

/** What a widget shows of its lines: its gauges as [rings], rows of them [ringDp] across, then the [lines] of the rest. */
internal data class WidgetLayout(val rings: List<List<WidgetLine>>, val ringDp: Float, val lines: List<WidgetLine>)

/**
 * What fits of [lines] in a widget [widthDp] by [heightDp] at the app's [scale]: its gauges as rings
 * at the top, in rows as even as they go (four in two rows of two rather than three and one), each
 * as large as the room allows up to [RING_MAX_DP]; then the other lines, in order, as many as fit
 * below. Where not even one ring fits, the gauges are lines like the rest.
 */
internal fun widgetLayout(lines: List<WidgetLine>, widthDp: Float, heightDp: Float, scale: Float): WidgetLayout {
	val gauges = lines.filter { it.ring != null }
	val width = widthDp - 2 * Space.m.value
	val height = heightDp - 2 * Space.m.value - HEADER_DP * scale
	val least = RING_MIN_DP * scale
	val gap = RING_GAP_DP * scale
	if (gauges.isEmpty() || width < least || height < least) return WidgetLayout(emptyList(), 0f, lines.take(linesThatFit(heightDp, scale)))
	val across = ((width + gap) / (least + gap)).toInt().coerceIn(1, gauges.size)
	val rows = minOf((gauges.size + across - 1) / across, ((height + gap) / (least + gap)).toInt())
	val shown = minOf(gauges.size, rows * across)
	val perRow = (shown + rows - 1) / rows
	val ring = minOf(RING_MAX_DP * scale, (width - (perRow - 1) * gap) / perRow, (height - (rows - 1) * gap) / rows)
	val left = height - rows * ring - rows * gap
	val rest = lines.filter { it.ring == null }
	return WidgetLayout(Grid.even(shown, perRow).map { row -> row.cells.map { (i, _) -> gauges[i] } }, ring, rest.take((left / (LINE_DP * scale)).toInt().coerceAtLeast(0)))
}

/** What the widget says beside the computer's name: when it was read, or what keeps it from it. */
private fun note(ctx: Context, shown: WidgetShown, reading: Boolean, narrow: Boolean): String? {
	val now = System.currentTimeMillis()
	val at = shown.readAt.takeIf { it > 0 }?.let {
		// In a narrow one the name has the room "AM" would take.
		if (narrow && DateUtils.isToday(it)) android.text.format.DateFormat.format(if (android.text.format.DateFormat.is24HourFormat(ctx)) "HH:mm" else "h:mm", it).toString()
		else DateUtils.formatSameDayTime(it, now, DateFormat.SHORT, DateFormat.SHORT).toString()
	}
	return when (widgetNote(shown, reading)) {
		Note.Reading -> ctx.getString(R.string.widget_reading)
		Note.At -> at
		Note.Offline -> at?.let { ctx.getString(R.string.widget_offline_at, it) } ?: ctx.getString(R.string.offline)
		Note.Failed -> at?.let { ctx.getString(R.string.widget_failed_at, it) } ?: ctx.getString(R.string.widget_failed)
		Note.Unpaired -> ctx.getString(R.string.conn_unauthorized)
		Note.Outdated -> ctx.getString(R.string.home_outdated)
		Note.None -> null
	}
}

/** What stands in place of the lines, where there are none to show; null where there are. */
private fun message(ctx: Context, machine: Machine?, shown: WidgetShown, reading: Boolean): String? = when {
	machine == null -> ctx.getString(R.string.widget_no_computer)
	shown.fetch == Fetch.NoDashboard -> ctx.getString(R.string.widget_no_dashboard, shown.profile ?: PROFILE_PLACEHOLDER)
	shown.lines.isNotEmpty() -> null
	reading || shown.fetch == Fetch.None -> ctx.getString(R.string.widget_first_reading)
	shown.fetch == Fetch.Ok -> ctx.getString(R.string.widget_nothing, shown.title ?: "")
	shown.fetch == Fetch.Unpaired -> ctx.getString(R.string.widget_unpaired)
	shown.fetch == Fetch.Outdated -> ctx.getString(R.string.widget_outdated)
	else -> ctx.getString(R.string.widget_no_connection, machine.name)
}

/** Where the profile's table is named while the computer hasn't said which profile it gives this phone. */
private const val PROFILE_PLACEHOLDER = "<id>"

/** What the note beside the name tells. */
internal enum class Note { Reading, At, Offline, Failed, Unpaired, Outdated, None }

/** The note for [shown], being read now or not ([reading]). */
internal fun widgetNote(shown: WidgetShown, reading: Boolean): Note = when {
	reading -> Note.Reading
	else -> when (shown.fetch) {
		Fetch.Ok -> Note.At
		Fetch.Offline -> Note.Offline
		Fetch.Failed -> Note.Failed
		Fetch.Unpaired -> Note.Unpaired
		Fetch.Outdated -> Note.Outdated
		Fetch.None, Fetch.NoDashboard -> Note.None
	}
}
