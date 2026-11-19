// A log's lines as the page keeps them (the app's LogLines.kt): those the stream sent since its last
// reset, after pages of older ones from `/api/v1/log`; "… n more" where the stream left some out.
// Each change makes a new value. Pure.

/** Lines a log keeps when its widget doesn't say. */
export const LOG_LINES_DEFAULT = 200;
/** Lines a log keeps at most, older pages included. */
export const LOG_LINES_MAX = 2000;
/** A search's length at most, as the service takes it. */
export const LOG_SEARCH_MAX = 100;
/** The least level a log shows when its widget doesn't say. */
export const DEFAULT_LOG_LEVEL = 'info';

/** The levels a log offers to filter by, least first. */
const LOG_CHIPS = ['debug', 'info', 'notice', 'warning', 'error'];

/** Counts every log's resets, so a page asked for before the last one is told apart as late. */
let generations = 0;

/**
 * @typedef {{ c: number, t: number, level?: string, unit?: string, id?: string, text?: string }} LogLine
 * @typedef {{ key: number, line: LogLine } | { key: number, more: number }} LogItem
 * @typedef {{ items: LogItem[], keep: number, loaded: number, cursor: number | null, atStart: boolean,
 *   note: string | null, generation: number, loadingOlder: boolean, olderFailed: boolean, nextKey: number }} LogLines
 */

/** No lines yet. @param {number} [keep] @returns {LogLines} */
export function empty(keep = LOG_LINES_DEFAULT) {
	return { items: [], keep, loaded: 0, cursor: null, atStart: false, note: null, generation: 0, loadingOlder: false, olderFailed: false, nextKey: 0 };
}

/** @param {LogLines} l */
export function lineCount(l) {
	return l.items.reduce((n, i) => n + ('line' in i ? 1 : 0), 0);
}

/** The oldest line held. @param {LogLines} l @returns {LogLine | null} */
export function oldest(l) {
	for (const i of l.items) if ('line' in i) return i.line;
	return null;
}

/** `before` for the next older page: where the last one ended, or the oldest line held. @param {LogLines} l */
export function before(l) {
	if (l.cursor != null) return l.cursor;
	const o = oldest(l);
	return o ? o.c : null;
}

/** Whether there may be older lines to load, and room for them. @param {LogLines} l */
export function canLoadOlder(l) {
	return !l.atStart && lineCount(l) < LOG_LINES_MAX;
}

/**
 * With the state `s` the stream sent, for a widget that keeps `keep` lines: its lines in place of
 * these on a reset, else after them, "… n more" before them where it left some out. A note stays
 * until a reset comes without one.
 * @param {LogLines} l @param {{ lines?: LogLine[], reset?: boolean, more?: number, note?: string, [key: string]: any }} s @param {number} keep @returns {LogLines}
 */
export function take(l, s, keep) {
	const base = s.reset ? { ...empty(keep), note: s.note ?? null, generation: ++generations, nextKey: l.nextKey } : { ...l, keep, note: s.note ?? l.note };
	let key = base.nextKey;
	const items = base.items.slice();
	const more = s.more || 0;
	if (more > 0) {
		const last = items[items.length - 1];
		if (last && 'more' in last) items[items.length - 1] = { key: last.key, more: last.more + more };
		else items.push({ key: key++, more });
	}
	for (const line of s.lines || []) items.push({ key: key++, line });
	return trimmed({ ...base, items, nextKey: key });
}

/** Asking for an older page. @param {LogLines} l @returns {LogLines} */
export function asking(l) {
	return { ...l, loadingOlder: true, olderFailed: false };
}

/** The older page didn't come. @param {LogLines} l @returns {LogLines} */
export function failed(l) {
	return { ...l, loadingOlder: false, olderFailed: true };
}

/** There are no older lines to give (a service before 0.8). @param {LogLines} l @returns {LogLines} */
export function noOlder(l) {
	return { ...l, loadingOlder: false, olderFailed: false, atStart: true };
}

/**
 * With the older `page` before these: its lines older than the oldest held, as many as there is
 * room for, the newest of them kept.
 * @param {LogLines} l @param {{ lines?: LogLine[], before?: number | null }} page @returns {LogLines}
 */
export function older(l, page) {
	const first = oldest(l);
	const fresh = (page.lines || []).filter((x) => first == null || x.c < first.c);
	const room = Math.max(LOG_LINES_MAX - lineCount(l), 0);
	const kept = room === 0 ? [] : fresh.slice(-room);
	let key = l.nextKey;
	const added = kept.map((line) => ({ key: key++, line }));
	let i = 0;
	while (i < l.items.length && 'more' in l.items[i]) i++;
	return {
		...l,
		items: [...added, ...l.items.slice(i)],
		loaded: l.loaded + kept.length,
		cursor: page.before ?? null,
		atStart: page.before == null,
		loadingOlder: false,
		olderFailed: false,
		nextKey: key,
	};
}

/** Without the oldest lines past what is kept; the next older page then starts at the oldest left. @param {LogLines} l @returns {LogLines} */
function trimmed(l) {
	let drop = lineCount(l) - Math.min(LOG_LINES_MAX, l.keep + l.loaded);
	if (drop <= 0) return l;
	let i = 0;
	while (drop > 0) {
		if ('line' in l.items[i]) drop--;
		i++;
	}
	while (i < l.items.length && 'more' in l.items[i]) i++;
	return { ...l, items: l.items.slice(i), cursor: null, atStart: false };
}

/** How a line's level looks: critical and error alike, debug dimmed. @param {string | undefined} level */
export function tone(level) {
	switch (level) {
		case 'critical':
		case 'error':
			return 'error';
		case 'warning':
			return 'warning';
		case 'notice':
			return 'notice';
		case 'debug':
			return 'dimmed';
		default:
			return 'normal';
	}
}

/** The levels a log offers to filter by, least first: critical only for a log that starts there. @param {string} own */
export function levels(own) {
	return own === 'critical' ? [...LOG_CHIPS, own] : LOG_CHIPS;
}

/**
 * A log's filter as the stream is told it: null for its own (its level, no search), else the
 * level and search. @param {string} own @param {string} level @param {string} search
 * @returns {{ level: string, search?: string } | null}
 */
export function filterFor(own, level, search) {
	const q = [...search.trim()].slice(0, LOG_SEARCH_MAX).join('');
	if (level === own && q === '') return null;
	return q ? { level, search: q } : { level };
}
