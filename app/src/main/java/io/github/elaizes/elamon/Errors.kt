package io.github.elaizes.elamon

import kotlinx.coroutines.CancellationException
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ElamondException.Kind
import io.github.elaizes.elamon.data.cleanName

/** The app's words for each error code the service can answer with. */
private val CODE_MESSAGES = mapOf(
	"unauthorized" to R.string.err_unauthorized,
	"control_disabled" to R.string.err_control_disabled,
	"not_permitted" to R.string.err_not_permitted,
	"wrong_code" to R.string.err_wrong_code,
	"no_pairing" to R.string.err_no_pairing,
	"pair_declined" to R.string.err_pair_declined,
	"not_found" to R.string.err_not_found,
	"no_such_process" to R.string.err_no_such_process,
	"no_such_action" to R.string.err_no_such_action,
	"no_such_dashboard" to R.string.err_no_such_dashboard,
	"no_such_widget" to R.string.err_no_such_widget,
	"no_such_stream" to R.string.err_no_such_stream,
	"no_art" to R.string.err_no_art,
	"unknown_signal" to R.string.err_unknown_signal,
	"bad_sort" to R.string.err_bad_sort,
	"storage" to R.string.err_storage,
	"not_allowed" to R.string.err_not_allowed,
	// A station's command that is typed in, sent without its text: its `message` is the text.
	"confirm_required" to R.string.err_confirm_required,
	"process_changed" to R.string.err_process_changed,
	// Pairing past the phones the computer takes; a key of a phone's own that a token gave already,
	// which only Elamon 0.6.1 asks for (the service's list of codes still has it).
	"too_many_phones" to R.string.err_too_many_phones,
	"rekey_refused" to R.string.err_rekey_refused,
)

/**
 * The app's words for what a `failed` says, where it is one of the service's own codes rather than
 * the system's text: a monitor that didn't change in time, or that waits for a screen to wake.
 */
private val FAILED_MESSAGES = mapOf(
	"not_applied" to R.string.err_not_applied,
	"waiting_for_a_screen" to R.string.err_waiting_for_a_screen,
)

/** What to tell about a failed request: the computer's own words for a failed command, else one of the app's strings. */
fun messageFor(e: Throwable): Message {
	if (e is CancellationException) return Message.Res(R.string.err_unknown)
	val a = e as? ElamondException ?: return Message.Res(R.string.err_unknown)
	if (a.code == "failed") {
		val said = a.message?.takeIf { it.isNotBlank() } ?: return Message.Res(R.string.err_failed)
		return FAILED_MESSAGES[said]?.let { Message.Res(it) } ?: Message.Text(said)
	}
	// The parameter the computer didn't take, by its name.
	if (a.code == "bad_param") return Message.Res(R.string.err_bad_param, listOf(a.detail?.takeIf { it.isNotBlank() } ?: "?"))
	// What is to be typed, where the computer says it: a command, or the last monitor's name.
	if (a.code == "confirm_required") a.detail?.let(::cleanName)?.takeIf { it.isNotEmpty() }?.let { return Message.Res(R.string.err_confirm_required_text, listOf(it)) }
	a.code?.let { code -> CODE_MESSAGES[code]?.let { return Message.Res(it) } }
	return when (a.kind) {
		Kind.Unreachable -> Message.Res(R.string.err_unreachable)
		Kind.Unauthorized -> Message.Res(R.string.err_unauthorized)
		Kind.CertificateChanged -> Message.Res(R.string.err_certificate_changed)
		Kind.WrongCode -> Message.Res(R.string.err_wrong_code)
		Kind.NoPairing -> Message.Res(R.string.err_no_pairing)
		Kind.PairDeclined -> Message.Res(R.string.err_pair_declined)
		Kind.NotElamond -> Message.Res(R.string.err_not_elamond)
		Kind.ControlDisabled -> Message.Res(R.string.err_control_disabled)
		Kind.NotPermitted -> Message.Res(R.string.err_not_permitted)
		Kind.NotAllowed -> Message.Res(R.string.err_not_allowed)
		Kind.NotFound -> Message.Res(R.string.err_not_found)
		Kind.Failed -> Message.Res(R.string.err_failed)
		// A code from a newer service is still more use than nothing.
		Kind.Forbidden, Kind.BadRequest, Kind.Conflict -> a.code?.let { Message.Res(R.string.err_code, listOf(it)) } ?: Message.Res(R.string.err_refused)
		Kind.Protocol -> a.code?.let { Message.Res(R.string.err_code, listOf(it)) } ?: Message.Res(R.string.err_protocol)
	}
}
