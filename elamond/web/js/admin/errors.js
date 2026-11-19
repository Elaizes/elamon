// A refusal in words: the service's error code as a sentence (strings.js), with its own words
// (`message`) where it gave some that the sentence doesn't already say.
import { S, fill } from '../strings.js';

/** @typedef {{ status: number, code: string, detail?: string | null, body?: any }} Refusal */

/** Whether `e` is an answer of the service's that wasn't 2xx (an ApiError, or one shaped as it). @param {unknown} e @returns {e is Refusal} */
export function isRefusal(e) {
	return !!e && typeof e === 'object' && typeof (/** @type {any} */ (e).code) === 'string' && typeof (/** @type {any} */ (e).status) === 'number';
}

/** Whether `e` is a request given up on (the section closed): nothing to say. @param {unknown} e */
export function isAbort(e) {
	return !!e && typeof e === 'object' && /** @type {any} */ (e).name === 'AbortError';
}

/** The mistakes a `bad_settings` refusal carries. @param {unknown} e @returns {any[]} */
export function mistakesOf(e) {
	if (!isRefusal(e) || !e.body || !Array.isArray(e.body.mistakes)) return [];
	return e.body.mistakes;
}

/**
 * What went wrong, as a sentence.
 * @param {unknown} e an ApiError, a TypeError (the computer can't be reached) or anything else
 * @returns {string}
 */
export function errorSentence(e) {
	const E = S.admin.errors;
	if (!isRefusal(e)) return e instanceof TypeError ? E.unreachable : E.unknown;
	const detail = typeof e.detail === 'string' && e.detail.trim() ? e.detail.trim() : null;
	/** @type {string} */
	let sentence;
	/** Whether the sentence already holds the service's words. */
	let said = false;
	switch (e.code) {
		case 'writes_off':
			sentence = detail === 'held_off' ? E.writes_off_held : E.writes_off;
			said = detail === 'held_off';
			break;
		case 'read_only':
			sentence = detail ? fill(E.read_only_why, detail) : E.read_only;
			said = true;
			break;
		case 'bad_param':
			sentence = detail ? fill(E.bad_param_named, detail) : E.bad_param;
			said = true;
			break;
		case 'failed':
			sentence = detail ? fill(E.failed_why, detail) : E.failed;
			said = true;
			break;
		case 'bad_settings': {
			const n = mistakesOf(e).length;
			sentence = n === 1 ? E.bad_settings_one : n > 1 ? fill(E.bad_settings_many, n) : E.bad_settings;
			break;
		}
		default: {
			const known = /** @type {Record<string, string>} */ (E.codes)[e.code];
			if (known) sentence = known;
			else if (e.code === 'http_413') sentence = E.too_large;
			else if (/^http_5\d\d$/.test(e.code)) sentence = fill(E.server, e.status);
			else sentence = fill(E.other, e.code);
		}
	}
	return detail && !said ? fill(E.withWords, sentence, detail) : sentence;
}
