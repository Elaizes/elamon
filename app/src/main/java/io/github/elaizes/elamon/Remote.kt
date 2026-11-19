package io.github.elaizes.elamon

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import io.github.elaizes.elamon.data.APP_API
import io.github.elaizes.elamon.data.Action
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.reachPaired

/**
 * A paired computer reached for an exchange away from its own screen: the cards on the home screen,
 * the home-screen widget, the Quick Settings tiles. No stream, no connection kept: a request or two,
 * and done.
 */
sealed interface Reach {
	/** It answers, as itself, over the certificate pinned for it; [client] carries this phone's token. */
	data class Reached(val client: ElamondClient) : Reach

	/** None of its addresses answers as it (another certificate there counts as this too). */
	data object Offline : Reach

	/** The phone can't read its own token for it any more. */
	data object Unpaired : Reach

	/** Its service speaks another API. */
	data object Outdated : Reach
}

/**
 * Reaches [machine] on whichever of its addresses answers, with its token as the home screen reads
 * it: the key store's key isn't bound to unlocking the phone (data/Keystore.kt), so this works
 * from the background too, once the phone has been unlocked after it started.
 */
suspend fun reach(machine: Machine, store: MachineStore): Reach {
	// Opened by the key store off the main thread, the first time.
	val token = withContext(Dispatchers.Default) { store.token(machine) } ?: return Reach.Unpaired
	val reached = try {
		reachPaired(machine)
	} catch (e: ElamondException) {
		// Another certificate: the computer's own screen tells of it; here it is out of reach.
		null
	} ?: return Reach.Offline
	if (reached.second.api != APP_API) return Reach.Outdated
	return Reach.Reached(ElamondClient(reached.first.host, machine.port, machine.fingerprint, token))
}

/**
 * The `ui` of [machine]: the one kept on the phone, or with [fresh] (or none kept) the computer's
 * own, then kept. Where the computer says it is goes with it, as the home screen takes it.
 * [ticket], taken before asking: forgotten meanwhile, the computer's answer isn't kept.
 */
suspend fun uiOf(machine: Machine, client: ElamondClient, cache: UiCache, store: MachineStore, fresh: Boolean, ticket: Long): UiDoc {
	val pin = machine.fingerprint
	// The one kept came over this computer's certificate and to its token too: where it said it is
	// then is taken as well, for a phone that never opens the computer's screen.
	if (!fresh) cache.ui(machine.id)?.let {
		store.takeEndpoints(machine.id, pin, it.service.endpoints)
		return it
	}
	return client.ui().also {
		cache.putUi(machine.id, it, ticket)
		store.takeEndpoints(machine.id, pin, it.service.endpoints)
	}
}

/** A dashboard's reading, and the `ui` it was read by; no [line] where the `ui` names none to read. */
data class Reading(val ui: UiDoc, val line: DataLine?)

/**
 * A reading of the dashboard [pick] names in a computer's ui, with [snapshot]: by the ui kept ([ui]
 * with false), or, where that is out of date, once more by the computer's own (true). Out of date: the
 * dashboard refused (gone from what the computer shows this phone: renamed, or the role or profile
 * changed), a reading with a widget the kept ui hasn't (the dashboards changed), or, with
 * [freshWithout], none named in it at all (the profile may name one since). What fails is thrown.
 */
suspend fun readDashboard(
	ui: suspend (fresh: Boolean) -> UiDoc,
	pick: (UiDoc) -> String?,
	snapshot: suspend (String) -> DataLine,
	freshWithout: Boolean = false,
): Reading {
	var doc = ui(false)
	var again = true
	while (true) {
		val id = pick(doc)
		if (id == null) {
			if (!freshWithout || !again) return Reading(doc, null)
		} else {
			try {
				val line = snapshot(id)
				// A reading has the states of the dashboard's widgets and the header's.
				val known = allWidgets(listOfNotNull(doc.dashboard(id), doc.header?.let(doc::dashboard)).flatMap { it.widgets }).map { it.id }
				if (!again || known.containsAll(line.widgets.keys)) return Reading(doc, line)
			} catch (e: ElamondException) {
				val stale = e.status in HTTP_CLIENT_ERRORS && e.kind != ElamondException.Kind.Unauthorized
				if (!stale || !again) throw e
			}
		}
		again = false
		doc = ui(true)
	}
}

private val HTTP_CLIENT_ERRORS = 400..499

/**
 * Runs [action] of [machine] with [params] (and [typed], the text a choice asks to be typed), once
 * whatever asks before it has been answered: from the home screen's card, or a Quick Settings tile.
 * Its answer, or the failure, thrown, as [messageFor] tells it.
 */
suspend fun runAction(machine: Machine, store: MachineStore, action: Action, params: JsonObject, typed: String?): ActionResult =
	when (val r = reach(machine, store)) {
		is Reach.Reached -> r.client.action(action.id, params, typed)
		Reach.Unpaired -> throw ElamondException(ElamondException.Kind.Unauthorized)
		Reach.Outdated -> throw ElamondException(ElamondException.Kind.Protocol)
		Reach.Offline -> throw ElamondException(ElamondException.Kind.Unreachable)
	}

/**
 * Work outside any screen that outlives the one that started it: an action a tile runs goes on
 * after the tile stops listening, as an action from a computer's screen goes on after Back.
 */
object Outside {
	val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
}

/**
 * What to say once [action] of the card or a tile ran and answered [r]: how a command ended, as its
 * button shows it, else what a built-in action says when it is done ([done]); null where nothing needs saying.
 */
fun afterRun(action: Action, params: JsonObject, r: ActionResult): Message? = when {
	!printsOutput(action, r) -> done(action, params)
	r.timedOut -> Message.Res(R.string.control_timed_out)
	r.exitCode != null && r.exitCode != 0 -> Message.Res(R.string.control_exit_code, listOf(r.exitCode.toString()))
	else -> Message.Res(R.string.done_ran, listOf(action.name))
}
