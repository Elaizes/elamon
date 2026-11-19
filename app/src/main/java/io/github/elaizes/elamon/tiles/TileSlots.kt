package io.github.elaizes.elamon.tiles

import android.content.ComponentName
import android.content.Context
import android.service.quicksettings.TileService
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import io.github.elaizes.elamon.Message
import io.github.elaizes.elamon.Outside
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.afterRun
import io.github.elaizes.elamon.allWidgets
import io.github.elaizes.elamon.data.Action
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.Params
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.messageFor
import io.github.elaizes.elamon.runAction
import io.github.elaizes.elamon.widget.shownMachine

/** How many Quick Settings tiles the app offers: as many as the manifest declares. */
const val TILES = 4

/** What a Quick Settings tile runs: a `button` or `toggle` of the profile's `tiles` dashboard, and its action. */
data class TileSlot(val widget: Widget, val action: Action) {
	/** Named as the widget, else as its action. */
	val label: String get() = widget.label?.ifEmpty { null } ?: action.name

	/** Drawn as the widget, else as its action. */
	val icon: String? get() = widget.icon ?: action.icon

	val toggle: Boolean get() = widget.type == "toggle"
}

/** The dashboard the tiles take their actions from: the profile's `tiles`, where the ui has it. */
fun tilesDashboard(ui: UiDoc): String? = ui.profile.android.tiles.takeIf { it.isNotEmpty() && ui.dashboard(it) != null }

/**
 * The tiles' actions in [ui]: the buttons and toggles of the profile's `tiles` dashboard, in order,
 * those in its groups too, each whose action this phone may run there; the first [TILES] of them.
 * A button that only opens a page has no action for a tile.
 */
fun tileSlots(ui: UiDoc): List<TileSlot> {
	val dashboard = tilesDashboard(ui)?.let(ui::dashboard) ?: return emptyList()
	return allWidgets(dashboard.widgets)
		.filter { (it.type == "button" || (it.type == "toggle" && it.param != null)) && it.action != null }
		.mapNotNull { w -> ui.action(w.action)?.let { TileSlot(w, it) } }
		.take(TILES)
}

/**
 * Whether a tap on [slot] asks before it runs, in a window over whatever is on screen: what the
 * computer says to confirm or to unlock for, a choice typed in, or values its widget leaves open.
 * The asking itself is the app's ([io.github.elaizes.elamon.ui.Guard]), with its grace.
 */
fun asksFirst(slot: TileSlot, params: JsonObject): Boolean =
	slot.action.confirm || slot.action.unlock || Params.typedText(slot.action, params) != null || Params.asked(slot.action, params).isNotEmpty()

/** The computer the tiles act on: the one chosen in the settings while it is paired, else the first paired. */
fun tilesMachine(context: Context): Machine? = shownMachine(Settings.get(context).tilesMachine(), MachineStore.get(context).machines.value)

/** What a tile shows of what it ran last, for a while. */
data class TileNote(val text: Message, val at: Long, val failed: Boolean)

/**
 * What the tiles know between their listening: each toggle's state as last read, and how each
 * slot's action ended. For as long as the app runs; read again when a tile starts listening.
 */
object TileMemory {
	/** A toggle's state, by its widget's id. */
	val on = ConcurrentHashMap<String, Boolean>()

	/** How the last run of each slot ended, by its widget's id. */
	val notes = ConcurrentHashMap<String, TileNote>()

	/** The slots' widgets running now. */
	val running: MutableSet<String> = ConcurrentHashMap.newKeySet()

	/** How long what a tile ran stays said under its name. */
	const val NOTE_MS = 10 * 60_000L

	/** Counts the changes to what the tiles show, for a tile that listens to draw itself again. */
	val changes = MutableStateFlow(0)

	fun changed() = changes.update { it + 1 }

	fun note(widget: String, now: Long): TileNote? = notes[widget]?.takeIf { now - it.at < NOTE_MS }
}

/** The tiles' services, slot by slot. */
private val SERVICES = listOf(Tile1::class.java, Tile2::class.java, Tile3::class.java, Tile4::class.java)

/** Asks every tile to draw itself again: their computer, or what its profile names, changed. */
fun refreshTiles(context: Context) {
	for (s in SERVICES) runCatching { TileService.requestListeningState(context, ComponentName(context, s)) }
}

/**
 * Runs [action] of [machine], a tile's ([widget]), once whatever asks before it has been answered:
 * outside any screen, so it goes on after the panel closes. How it ended is the tile's to show; a
 * toggle's, done, [turnsTo] on or off.
 */
fun runForTile(context: Context, machine: Machine, widget: String, action: Action, params: JsonObject, typed: String?, turnsTo: Boolean? = null) {
	val ctx = context.applicationContext
	if (!TileMemory.running.add(widget)) return
	TileMemory.changed()
	refreshTiles(ctx)
	Outside.scope.launch {
		val note = try {
			val r = runAction(machine, MachineStore.get(ctx), action, params, typed)
			turnsTo?.let { TileMemory.on[widget] = it }
			TileNote(afterRun(action, params, r) ?: Message.Res(io.github.elaizes.elamon.R.string.control_done), System.currentTimeMillis(), failed = false)
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			TileNote(messageFor(e), System.currentTimeMillis(), failed = true)
		}
		TileMemory.notes[widget] = note
		TileMemory.running.remove(widget)
		TileMemory.changed()
		refreshTiles(ctx)
	}
}
