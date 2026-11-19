// The page's words for what the service answers instead of doing something (docs/protocol.md,
// "Errors"), as the app's Errors.kt has them: a failed command in the computer's own words, the
// rest by code.
import { ApiError } from './api.js';
import { S, fill } from './strings.js';

const P = S.panels;

/** @type {Record<string, string>} */
const CODES = {
	unauthorized: P.errUnauthorized,
	control_disabled: P.errControlDisabled,
	not_permitted: P.errNotPermitted,
	wrong_code: P.pairWrongCode,
	no_pairing: P.pairNoPairing,
	pair_declined: P.pairDeclined,
	too_many_phones: P.pairTooMany,
	bad_origin: P.errBadOrigin,
	not_found: P.errNotFound,
	no_such_process: P.errNoSuchProcess,
	no_such_action: P.errNoSuchAction,
	no_such_dashboard: P.errNoSuchDashboard,
	no_such_widget: P.errNoSuchWidget,
	no_such_stream: P.errNoSuchStream,
	no_art: P.errNoArt,
	unknown_signal: P.errUnknownSignal,
	bad_sort: P.errBadSort,
	storage: P.errStorage,
	not_allowed: P.errNotAllowed,
	confirm_required: P.errConfirmRequired,
	process_changed: P.errProcessChanged,
	rekey_refused: P.errRekeyRefused,
};

/** The page's words for an error code, or null for one it has none for. @param {string} code */
export function codeWords(code) {
	return Object.hasOwn(CODES, code) ? CODES[code] : null;
}

/** What a `failed` says, where it is one of the service's own words rather than the system's. */
/** @type {Record<string, string>} */
const FAILED = {
	not_applied: P.errNotApplied,
	waiting_for_a_screen: P.errWaitingForAScreen,
	timed_out: P.timedOut,
};

/** Control characters and Unicode format characters out of what the computer said. @param {string} s */
function clean(s) {
	return s.replace(/[\p{Cc}\p{Cf}]+/gu, ' ').trim();
}

/** What to tell about a failed request. @param {unknown} e @returns {string} */
export function errorText(e) {
	if (e instanceof ApiError) {
		const said = e.detail ? clean(e.detail) : '';
		if (e.code === 'failed') return said ? FAILED[said] || said : P.errFailed;
		if (e.code === 'bad_param') return fill(P.errBadParam, said || '?');
		if (e.code === 'confirm_required' && said) return fill(P.errConfirmRequiredText, said);
		if (Object.hasOwn(CODES, e.code)) return CODES[e.code];
		return fill(P.errCode, e.code);
	}
	if (e instanceof TypeError) return P.errUnreachable;
	if (e instanceof DOMException && e.name === 'AbortError') return P.pairTimedOut;
	return P.errUnknown;
}
