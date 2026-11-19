package io.github.elaizes.elamon.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.text.format.DateUtils
import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
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
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import java.text.DateFormat
import io.github.elaizes.elamon.MainActivity
import io.github.elaizes.elamon.Prefs
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Scale
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.ThemeMode
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.ui.MetricColors
import io.github.elaizes.elamon.ui.appColorScheme
import io.github.elaizes.elamon.ui.color
import io.github.elaizes.elamon.ui.levelOf
import io.github.elaizes.elamon.ui.metricColors
import io.github.elaizes.elamon.with
import androidx.glance.color.ColorProvider as DayNight

/**
 * The home-screen widget: the dashboard the profile's `widget` names, of the computer chosen for it
 * (or the one paired), as label and value lines; when it was read, and what it is when it can't be.
 * A tap reads it at once; its name opens the computer in the app.
 */
class ElamonWidget : GlanceAppWidget() {
	/** Drawn for the size it is: as many lines as fit. */
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

	/** What was read before, shown while the computer can't be reached: faded. */
	val old = scheme { it.onSurfaceVariant.copy(alpha = OLD_ALPHA) }

	/** A line's value by its level, as its tile colours it: green or the text's own while normal, turning amber, then red. */
	fun level(line: WidgetLine): ColorProvider {
		val band = levelOf(line.level, line.blend) ?: return text
		fun pick(s: ColorScheme, m: MetricColors) = band.color(if (line.green) m.good else s.onSurface, m.warn, m.hot)
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

/** The widget's padding, and a line's height at 100%, for how many fit. */
private val Pad = 12.dp
private const val LINE_DP = 20f
private const val HEADER_DP = 30f
private const val TEXT_SP = 14f
private const val SMALL_SP = 12f
private const val TITLE_SP = 15f
private val Corner = 16.dp

@Composable
private fun Content(machine: Machine?, shown: WidgetShown, reading: Boolean, c: WidgetColors, scale: Float) {
	val ctx = LocalContext.current
	val open = actionStartActivity(Intent(ctx, MainActivity::class.java).apply { machine?.let { putExtra(MainActivity.EXTRA_MACHINE, it.id) } })
	val tap = machine?.let { actionRunCallback<RefreshWidget>(actionParametersOf(MACHINE to it.id)) } ?: open
	val body = TextStyle(color = c.text, fontSize = (TEXT_SP * scale).sp)
	val small = TextStyle(color = c.quiet, fontSize = (SMALL_SP * scale).sp)
	Column(
		GlanceModifier.fillMaxSize().appWidgetBackground().cornerRadius(Corner).background(c.background).padding(Pad).clickable(tap),
	) {
		// Its name and when it was read; the name opens it in the app.
		Row(GlanceModifier.fillMaxWidth().clickable(open), verticalAlignment = Alignment.CenterVertically) {
			Text(
				machine?.name ?: ctx.getString(R.string.app_name),
				GlanceModifier.defaultWeight(),
				TextStyle(color = c.text, fontSize = (TITLE_SP * scale).sp, fontWeight = FontWeight.Bold),
				maxLines = 1,
			)
			note(ctx, shown, reading)?.let {
				Spacer(GlanceModifier.width(8.dp))
				Text(it, style = small, maxLines = 1)
			}
		}
		Spacer(GlanceModifier.height(6.dp))
		val words = message(ctx, machine, shown, reading)
		if (words != null) {
			Text(words, style = body.copy(color = c.quiet))
			return@Column
		}
		val old = shown.fetch != Fetch.Ok
		val fit = linesThatFit(LocalSize.current.height.value, scale)
		for (line in shown.lines.take(fit)) Line(line, old, c, scale)
	}
}

@Composable
private fun Line(line: WidgetLine, old: Boolean, c: WidgetColors, scale: Float) {
	val size = (TEXT_SP * scale).sp
	if (line.words) {
		Text(line.text, GlanceModifier.fillMaxWidth().padding(vertical = 1.dp), TextStyle(color = if (old) c.old else c.level(line), fontSize = size), maxLines = 2)
		return
	}
	Row(GlanceModifier.fillMaxWidth().padding(vertical = 1.dp), verticalAlignment = Alignment.CenterVertically) {
		Text(line.label, GlanceModifier.defaultWeight(), TextStyle(color = if (old) c.old else c.quiet, fontSize = size), maxLines = 1)
		Spacer(GlanceModifier.width(8.dp))
		Text(line.text, style = TextStyle(color = if (old) c.old else c.level(line), fontSize = size, fontWeight = FontWeight.Medium), maxLines = 1)
	}
}

/** How many lines fit in a widget [heightDp] tall at the app's [scale]: one at least. */
internal fun linesThatFit(heightDp: Float, scale: Float): Int =
	((heightDp - 2 * Pad.value - HEADER_DP * scale) / (LINE_DP * scale)).toInt().coerceAtLeast(1)

/** What the widget says beside the computer's name: when it was read, or what keeps it from it. */
private fun note(ctx: Context, shown: WidgetShown, reading: Boolean): String? {
	val at = shown.readAt.takeIf { it > 0 }?.let { DateUtils.formatSameDayTime(it, System.currentTimeMillis(), DateFormat.SHORT, DateFormat.SHORT).toString() }
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
