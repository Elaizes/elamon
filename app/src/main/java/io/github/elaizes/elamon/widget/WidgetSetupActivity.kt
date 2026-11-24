package io.github.elaizes.elamon.widget

import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.getValue
import androidx.compose.ui.res.stringResource
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import io.github.elaizes.elamon.Prefs
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.ui.DialogButton
import io.github.elaizes.elamon.ui.ElIcons
import io.github.elaizes.elamon.ui.ElamonTheme
import io.github.elaizes.elamon.ui.GlassDialog
import io.github.elaizes.elamon.ui.ListRow
import io.github.elaizes.elamon.with

/**
 * Which computer a home-screen widget shows, asked as it is added (and, from Android 12, when the
 * home screen offers to change it): with several paired, a list of them; with one or none, nothing
 * is asked and the widget is placed at once.
 */
class WidgetSetupActivity : ComponentActivity() {
	private var widget = AppWidgetManager.INVALID_APPWIDGET_ID

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		widget = intent?.extras?.getInt(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID) ?: AppWidgetManager.INVALID_APPWIDGET_ID
		// Left without a choice, the home screen takes the widget away again.
		setResult(RESULT_CANCELED, done())
		if (widget == AppWidgetManager.INVALID_APPWIDGET_ID) return finish()
		val store = MachineStore.get(this)
		val machines = store.machines.value
		if (machines.size <= 1) return chosen(machines.firstOrNull()?.id)
		val settings = Settings.get(this)
		val now = settings.widgetMachine(widget)
		setContent {
			val profiles by UiCache.get(this).profiles.collectAsStateWithLifecycle()
			val look by settings.look.collectAsStateWithLifecycle()
			val paired by store.machines.collectAsStateWithLifecycle()
			ElamonTheme(Prefs().with(look.theme)) {
				GlassDialog(
					onDismiss = ::finish,
					title = stringResource(R.string.widget_choose_title),
					icon = ElIcons.Laptop,
					text = {
						Column {
							for (m in paired) {
								val profile = profiles[m.id]
								ListRow(
									m.name,
									summary = if (m.id == now) stringResource(R.string.widget_choose_now) else profile?.let { stringResource(R.string.widget_choose_profile, it.name.ifEmpty { it.id }) },
									icon = ElIcons.Laptop,
									onClick = { chosen(m.id) },
								)
							}
						}
					},
					confirm = DialogButton(stringResource(R.string.cancel), onClick = ::finish),
				)
			}
		}
	}

	/** The widget shows [machine] (none, with nothing paired yet: the first paired later), read at once. */
	private fun chosen(machine: String?) {
		val ctx = applicationContext
		if (machine != null) Settings.get(ctx).setWidgetMachine(widget, machine)
		setResult(RESULT_OK, done())
		WidgetWork.sync(ctx)
		machine?.let { WidgetWork.now(ctx, it) }
		lifecycleScope.launch {
			runCatching { ElamonWidget().update(ctx, GlanceAppWidgetManager(ctx).getGlanceIdBy(widget)) }
			finish()
		}
	}

	private fun done() = Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widget)
}
