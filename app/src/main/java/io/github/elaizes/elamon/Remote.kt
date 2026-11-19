package io.github.elaizes.elamon

import android.os.SystemClock
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
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
 * Refusals after which the `ui` is read again: what this phone may do there changed since it was
 * read (its role, control, an action gone, or a monitor that has become the last one showing a
 * picture, whose name is now to be typed).
 */
val UI_CHANGED = setOf("not_allowed", "control_disabled", "no_such_action", "confirm_required")

/** How long after the tap an action outside the app may still be sent: a process frozen meanwhile doesn't send it late. */
const val OUTSIDE_DEADLINE_MS = 20_000L

/**
 * Runs [action] of [machine] with [params] (and [typed], the text a choice asks to be typed), once
 * whatever asks before it has been answered: from the home screen's card, or a Quick Settings tile.
 * Not sent once [deadline] (elapsed realtime) has passed: a request left waiting for an address,
 * the app's process frozen in the background meanwhile, isn't sent minutes after the tap. Refused
 * as [UI_CHANGED] says, the `ui` is read again for the next try. Its answer, or the failure, thrown,
 * as [messageFor] tells it.
 */
suspend fun runAction(
	machine: Machine,
	store: MachineStore,
	cache: UiCache,
	action: Action,
	params: JsonObject,
	typed: String?,
	deadline: Long = SystemClock.elapsedRealtime() + OUTSIDE_DEADLINE_MS,
): ActionResult {
	val ticket = cache.ticket(machine.id)
	val client = when (val r = withTimeoutOrNull(OUTSIDE_DEADLINE_MS) { reach(machine, store) }) {
		is Reach.Reached -> r.client
		Reach.Unpaired -> throw ElamondException(ElamondException.Kind.Unauthorized)
		Reach.Outdated -> throw ElamondException(ElamondException.Kind.Protocol)
		Reach.Offline, null -> throw ElamondException(ElamondException.Kind.Unreachable)
	}
	if (SystemClock.elapsedRealtime() > deadline) throw ElamondException(ElamondException.Kind.Unreachable)
	return try {
		client.action(action.id, params, typed)
	} catch (e: ElamondException) {
		if (e.code in UI_CHANGED) orNull { uiOf(machine, client, cache, store, fresh = true, ticket) }
		throw e
	}
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
