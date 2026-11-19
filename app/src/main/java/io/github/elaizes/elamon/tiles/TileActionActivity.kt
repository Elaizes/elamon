package io.github.elaizes.elamon.tiles

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.withStarted
import kotlinx.coroutines.launch
import io.github.elaizes.elamon.ActionsState
import io.github.elaizes.elamon.Prefs
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.ui.ActionLauncher
import io.github.elaizes.elamon.ui.ElamonTheme
import io.github.elaizes.elamon.ui.FormRequest
import io.github.elaizes.elamon.ui.Guard
import io.github.elaizes.elamon.ui.GuardDialog
import io.github.elaizes.elamon.ui.GuardState
import io.github.elaizes.elamon.ui.ParamForm
import io.github.elaizes.elamon.ui.forgetUnlock
import io.github.elaizes.elamon.ui.scaledConfiguration
import io.github.elaizes.elamon.ui.tapOf
import io.github.elaizes.elamon.with

/** A tile's asking, kept through a rotation: whether it has begun, and the action's form. */
class TileAsk : ViewModel() {
	var launched = false
	val form = mutableStateOf<FormRequest?>(null)
}

/**
 * What a tile's action asks before it runs, over whatever is on screen: its values a widget leaves
 * open, then the confirmation, the typed name or command, or unlocking for it, all as the app asks
 * them ([Guard], with the profile's grace). Once answered, the action runs on for the tile
 * ([runForTile]), and this goes; cancelled, nothing runs. In a task of its own, out of the recent
 * apps: the app's own screens are left as they were.
 */
class TileActionActivity : FragmentActivity() {
	private val guardState: GuardState by viewModels()
	private val ask: TileAsk by viewModels()

	override fun attachBaseContext(base: Context) {
		super.attachBaseContext(base)
		// At the app's own scale, as its screens.
		scaledConfiguration(base.resources.configuration, Settings.get(base).look.value.scale)?.let(::applyOverrideConfiguration)
	}

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		// Made in onCreate, as the unlock prompt must be, to hear its answer after a rotation too.
		val guard = Guard(this, guardState)
		val machineId = intent?.getStringExtra(EXTRA_MACHINE)
		val widgetId = intent?.getStringExtra(EXTRA_WIDGET)
		val on = intent?.getBooleanExtra(EXTRA_ON, false) == true
		lifecycleScope.launch {
			val machine = machineId?.let { MachineStore.get(this@TileActionActivity).get(it) }
			val ui = machine?.let { UiCache.get(this@TileActionActivity).ui(it.id) }
			val slot = ui?.let { tileSlots(it).find { s -> s.widget.id == widgetId } }
			if (machine == null || ui == null || slot == null) return@launch finish()
			val launcher = launcher(guard, machine, ui, slot, on)
			// The prompt is shown once the activity is up.
			withStarted {
				if (!ask.launched) {
					ask.launched = true
					val tap = tapOf(slot.widget, slot.action, on)
					launcher.launch(slot.action, tap.params, tap.phrase, key = slot.widget.id)
				}
			}
			setContent {
				val profiles by UiCache.get(this@TileActionActivity).profiles.collectAsStateWithLifecycle()
				val look by Settings.get(this@TileActionActivity).look.collectAsStateWithLifecycle()
				ElamonTheme((profiles[machine.id]?.let(Prefs::of) ?: Prefs()).with(look.theme)) {
					GuardDialog(guard)
					ask.form.value?.let { req -> ParamForm(req, onDismiss = { ask.form.value = null }) { params -> launcher.submit(req, params) } }
				}
				// Why unlocking failed (locked out, no biometrics), said over whatever is on screen as this goes.
				LaunchedEffect(guard) { guard.messages.collect { Toast.makeText(applicationContext, it, Toast.LENGTH_LONG).show() } }
				// Nothing asked any more: run, or cancelled.
				val idle = guardState.confirming == null && guardState.unlocking == null && ask.form.value == null
				LaunchedEffect(idle) { if (idle) finish() }
			}
		}
	}

	/**
	 * Runs [slot]'s action of [machine] as the app runs any, once asked, for the tile to show how it
	 * ended: a toggle [on] when tapped, turned off, and the other way.
	 */
	private fun launcher(guard: Guard, machine: Machine, ui: UiDoc, slot: TileSlot, on: Boolean) = ActionLauncher(
		guard,
		run = { action, params, _, typed -> runForTile(applicationContext, machine, slot.widget.id, action, params, typed, turnsTo = if (slot.toggle) !on else null) },
		ui = ui,
		computer = machine.name,
		res = resources,
		tap = {},
		form = ask.form,
		running = ActionsState(),
	)

	override fun onStop() {
		super.onStop()
		// Left: the next action asks again, as when the app is left.
		if (!isChangingConfigurations) forgetUnlock()
	}

	companion object {
		private const val EXTRA_MACHINE = "machine"
		private const val EXTRA_WIDGET = "widget"
		private const val EXTRA_ON = "on"

		/** What asks before the tile's widget [widget] of [machine] runs, a toggle [on] now or not. */
		fun intent(context: Context, machine: String, widget: String, on: Boolean): Intent =
			Intent(context, TileActionActivity::class.java)
				.putExtra(EXTRA_MACHINE, machine)
				.putExtra(EXTRA_WIDGET, widget)
				.putExtra(EXTRA_ON, on)
				// Its own task, afresh: never the app's, nor one left from an earlier tap.
				.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
	}
}
