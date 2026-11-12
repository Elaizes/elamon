package io.github.elaizes.elamon

import kotlinx.coroutines.CancellationException
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ElamondException.Kind

/** The app's words for each error code the service can answer with. */
private val CODE_MESSAGES = mapOf(
	"unauthorized" to R.string.err_unauthorized,
	"control_disabled" to R.string.err_control_disabled,
	"not_permitted" to R.string.err_not_permitted,
	"protected_process" to R.string.err_protected_process,
	"wrong_code" to R.string.err_wrong_code,
	"no_pairing" to R.string.err_no_pairing,
	"pair_declined" to R.string.err_pair_declined,
	"not_found" to R.string.err_not_found,
	"no_such_process" to R.string.err_no_such_process,
	"no_such_action" to R.string.err_no_such_action,
	"no_such_dashboard" to R.string.err_no_such_dashboard,
	"no_such_stream" to R.string.err_no_such_stream,
	"no_art" to R.string.err_no_art,
	"unknown_signal" to R.string.err_unknown_signal,
	"bad_sort" to R.string.err_bad_sort,
	"storage" to R.string.err_storage,
	"not_allowed" to R.string.err_not_allowed,
	"too_many_requests" to R.string.err_too_many_requests,
	"process_changed" to R.string.err_process_changed,
	// Pairing, or a key of the phone's own, past the phones the computer takes; a token that gave one already.
	"too_many_phones" to R.string.err_too_many_phones,
	"rekey_refused" to R.string.err_rekey_refused,
	// Codes of the links editor elamond 0.5 had; errors.json lists them until the service's 0.6 list replaces it.
	"no_such_link" to R.string.err_no_such_link,
	"empty_command" to R.string.err_empty_command,
	"no_unit" to R.string.err_no_unit,
	"bad_name" to R.string.err_bad_name,
	"bad_url" to R.string.err_bad_url,
	"bad_unit" to R.string.err_bad_unit,
	"bad_scope" to R.string.err_bad_scope,
)

/** What to tell about a failed request: the computer's own words for a failed command, else one of the app's strings. */
fun messageFor(e: Throwable): Message {
	if (e is CancellationException) return Message.Res(R.string.err_unknown)
	val a = e as? ElamondException ?: return Message.Res(R.string.err_unknown)
	if (a.code == "failed") return a.message?.takeIf { it.isNotBlank() }?.let(Message::Text) ?: Message.Res(R.string.err_failed)
	// The parameter the computer didn't take, by its name.
	if (a.code == "bad_param") return Message.Res(R.string.err_bad_param, listOf(a.detail?.takeIf { it.isNotBlank() } ?: "?"))
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
		Kind.TooManyRequests -> Message.Res(R.string.err_too_many_requests)
		Kind.NotFound -> Message.Res(R.string.err_not_found)
		Kind.Failed -> Message.Res(R.string.err_failed)
		// A code from a newer service is still more use than nothing.
		Kind.Forbidden, Kind.BadRequest, Kind.Conflict -> a.code?.let { Message.Res(R.string.err_code, listOf(it)) } ?: Message.Res(R.string.err_refused)
		Kind.Protocol -> a.code?.let { Message.Res(R.string.err_code, listOf(it)) } ?: Message.Res(R.string.err_protocol)
	}
}
