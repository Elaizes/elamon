package io.github.elaizes.elamon.tiles

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.messageFor
import io.github.elaizes.elamon.Reach
import io.github.elaizes.elamon.Reading
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.reach
import io.github.elaizes.elamon.readDashboard
import io.github.elaizes.elamon.ui.iconRes
import io.github.elaizes.elamon.ui.tapOf
import io.github.elaizes.elamon.ui.text
import io.github.elaizes.elamon.ui.toggledOn
import io.github.elaizes.elamon.uiOf

/**
 * A Quick Settings tile: the [slot]th button or toggle of the dashboard the profile's `tiles` names,
 * of the computer chosen in the settings (or the one paired). Named and drawn as the widget, a
 * toggle on while its value is its `on`; a tap runs it, the phone unlocked first where it is locked,
 * and what the computer says to ask before it asked in the app's own way ([TileActionActivity]).
 * Without such a widget, it says so and does nothing.
 */
abstract class ElamonTile : TileService() {
	/** Which of the tiles this is, from 0. */
	abstract val slot: Int

	private var scope: CoroutineScope? = null

	/** What a tap runs, as last drawn: the computer and the widget; null for nothing. */
	private var shown: Pair<Machine, TileSlot>? = null

	/**
	 * The computer's own `ui`, read as the tile began listening: what a tap runs by, so that it asks
	 * before the action as the profile says now (an unlock or a confirmation added since included),
	 * not as the `ui` kept from before said. Null until it is read.
	 */
	private var fresh: UiDoc? = null

	override fun onStartListening() {
		super.onStartListening()
		val s = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
		scope?.cancel()
		scope = s
		fresh = null
		s.launch {
			val machine = tilesMachine(this@ElamonTile)
			// At once, as last read; then as the computer says now, toggles and profile alike.
			var ui = machine?.let { UiCache.get(this@ElamonTile).ui(it.id) }
			draw(machine, ui)
			if (machine == null) return@launch
			// An action that starts or ends meanwhile (this tile's, or another's on the same toggle) is shown.
			s.launch { TileMemory.changes.collect { draw(machine, ui) } }
			ui = TileReading.read(this@ElamonTile, machine)?.ui ?: return@launch
			fresh = ui
			draw(machine, ui)
		}
	}

	override fun onStopListening() {
		scope?.cancel()
		scope = null
		super.onStopListening()
	}

	override fun onDestroy() {
		scope?.cancel()
		super.onDestroy()
	}

	override fun onClick() {
		super.onClick()
		val (machine, slot) = shown ?: return
		val widget = slot.widget.id
		// A locked phone is unlocked first, whatever the action asks after.
		if (isLocked) unlockAndRun { act(machine, widget) } else act(machine, widget)
	}

	/**
	 * Runs the tile's [widget] of [machine] by the computer's own `ui`: the one read as the tile began
	 * listening, or, tapped before that came, one read now; never by the `ui` kept from before, whose
	 * `confirm` and `unlock` may be out of date. Out of reach, nothing runs, and the tile says so.
	 */
	private fun act(machine: Machine, widget: String) {
		fresh?.let { return tap(machine, widget, it) }
		(scope ?: MainScope()).launch {
			val ui = TileReading.read(this@ElamonTile, machine)?.ui
			if (ui == null) {
				TileMemory.notes[tileKey(machine.id, widget)] = TileNote(messageFor(ElamondException(ElamondException.Kind.Unreachable)), System.currentTimeMillis(), failed = true)
				TileMemory.changed()
				return@launch
			}
			fresh = ui
			draw(machine, ui)
			tap(machine, widget, ui)
		}
	}

	/** The tap on [widget], as [ui] has it in this tile's place; none where it no longer is, or can't run now. */
	private fun tap(machine: Machine, widget: String, ui: UiDoc) {
		val slot = tileSlots(ui).getOrNull(this.slot)?.takeIf { it.widget.id == widget } ?: return
		val key = tileKey(machine.id, widget)
		if (!ui.control.enabled || key in TileMemory.disabled) return
		val on = TileMemory.on[key] == true
		val tap = tapOf(slot.widget, slot.action, on)
		if (asksFirst(slot, tap.params)) {
			open(TileActionActivity.intent(this, machine.id, slot.widget.id, on))
			return
		}
		runForTile(this, machine, slot.widget.id, slot.action, tap.params, null, turnsTo = if (slot.toggle) !on else null)
	}

	/** What asks before the action, over whatever is on screen, the panel closed. */
	// Android 14 takes only a PendingIntent; before it, only an Intent.
	@SuppressLint("StartActivityAndCollapseDeprecated")
	private fun open(intent: Intent) {
		if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
			startActivityAndCollapse(PendingIntent.getActivity(this, slot, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
		} else {
			@Suppress("DEPRECATION")
			startActivityAndCollapse(intent)
		}
	}

	/** The tile as [ui] of [machine] has it, or as a tile with nothing to do. */
	private fun draw(machine: Machine?, ui: UiDoc?) {
		val tile = qsTile ?: return
		val s = ui?.let { tileSlots(it).getOrNull(slot) }
		shown = if (machine != null && s != null) machine to s else null
		val subtitle: String
		if (machine == null || ui == null || s == null) {
			tile.label = getString(R.string.tile_label, slot + 1)
			tile.icon = Icon.createWithResource(this, R.drawable.ic_control)
			tile.state = Tile.STATE_UNAVAILABLE
			subtitle = getString(R.string.tile_no_action)
		} else {
			val key = tileKey(machine.id, s.widget.id)
			val note = TileMemory.note(key, System.currentTimeMillis())
			val on = s.toggle && TileMemory.on[key] == true
			// Control off, or the widget disabled where the computer says so (a monitor's picture while it is off).
			val usable = ui.control.enabled && key !in TileMemory.disabled
			tile.label = s.label
			tile.icon = Icon.createWithResource(this, iconRes(s.icon))
			tile.state = when {
				!usable -> Tile.STATE_UNAVAILABLE
				on -> Tile.STATE_ACTIVE
				else -> Tile.STATE_INACTIVE
			}
			subtitle = when {
				key in TileMemory.running -> getString(R.string.tile_running)
				// How it ended isn't shown on a locked phone.
				note != null && !isLocked -> note.text.text(resources)
				!ui.control.enabled -> getString(R.string.control_off)
				else -> machine.name
			}
			if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && s.toggle) {
				tile.stateDescription = getString(if (on) R.string.tile_on else R.string.tile_off)
			}
		}
		if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) tile.subtitle = subtitle
		tile.updateTile()
	}
}

class Tile1 : ElamonTile() {
	override val slot = 0
}

class Tile2 : ElamonTile() {
	override val slot = 1
}

class Tile3 : ElamonTile() {
	override val slot = 2
}

class Tile4 : ElamonTile() {
	override val slot = 3
}

/**
 * One short reading for the tiles as the panel opens: the computer's own `ui` (a snapshot doesn't
 * say whether the one kept is out of date) and a snapshot of the tiles' dashboard, for the
 * toggles' states and whether each widget is enabled. The four tiles share it, and that it failed.
 */
object TileReading {
	private val lock = Mutex()

	/** The last reading: of which computer, when (the phone's clock), and what it gave (null: none). */
	private var last: Triple<String, Long, Reading?>? = null

	/** A reading as fresh as this is shared rather than asked for again. */
	private const val SHARED_MS = 5_000L

	/** The next tile asks again: an action ran, which may change what the computer says. */
	fun forget() {
		last = null
	}

	suspend fun read(context: Context, machine: Machine): Reading? = lock.withLock {
		val now = System.currentTimeMillis()
		last?.takeIf { (id, at, _) -> id == machine.id && now - at in 0 until SHARED_MS }?.let { return it.third }
		val r = ask(context, machine)
		r?.line?.widgets?.let { states ->
			for (s in tileSlots(r.ui)) {
				val key = tileKey(machine.id, s.widget.id)
				val state = states[s.widget.id] ?: continue
				// Not what a reading from before an action of its own says.
				if (!TileMemory.readable(key, now)) continue
				if (s.toggle) TileMemory.on[key] = toggledOn(s.widget, state)
				if (state.enabled == false) TileMemory.disabled += key else TileMemory.disabled -= key
			}
		}
		last = Triple(machine.id, now, r)
		r
	}

	private suspend fun ask(context: Context, machine: Machine): Reading? {
		val store = MachineStore.get(context)
		val cache = UiCache.get(context)
		// Taken before asking: forgotten meanwhile, the computer's answer isn't kept.
		val ticket = cache.ticket(machine.id)
		return try {
			val client = (reach(machine, store) as? Reach.Reached)?.client ?: return null
			readDashboard(
				ui = { fresh -> uiOf(machine, client, cache, store, fresh, ticket) },
				pick = ::tilesDashboard,
				snapshot = { client.snapshot(it, history = false) },
				freshWithout = true,
				// The computer's own: what the tiles run asks as the profile says now.
				fresh = true,
			)
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			null
		}
	}
}
