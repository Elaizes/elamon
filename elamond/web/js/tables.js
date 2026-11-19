// What a table's rows need besides drawing (the app's Dashboards.kt, Tables and Params): search,
// a local order until the service sends the one asked for, its bars, its row actions and links,
// and the values of an action's parameters. Pure.

/** "top": a quarter above the column's largest. */
const TOP = 'top';
const TOP_HEADROOM = 1.25;
/** A whole "{field}", filled from a row. */
const FIELD = /^\{([a-z0-9_]+)\}$/;
/** What a link's address says about the computer itself: the browser reaches it at another. */
const LOCAL_HOSTS = new Set(['{host}', 'localhost', '127.0.0.1', '[::1]']);
const SCHEME = /^[A-Za-z][A-Za-z0-9+.-]*:\/\//;

/**
 * @typedef {{ key: string, level?: string, blend?: number, f?: Record<string, any> }} Row
 * @typedef {{ field: string, label?: string, format?: string, unit?: string, bar?: { max?: any }, grow?: boolean }} Column
 */

/** A value as text, for searching and ordering words. @param {unknown} v */
function text(v) {
	if (v == null) return null;
	if (typeof v === 'string') return v;
	if (typeof v === 'number' || typeof v === 'boolean') return String(v);
	return null;
}

/** @param {unknown} v */
function num(v) {
	return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

/** The text columns a table's search looks in: those that hold words, not figures. @param {Column[]} columns */
export function textColumns(columns) {
	return columns.filter((c) => c.format == null || c.format === 'text');
}

/** The fields a table's search looks in: those the service names, or its text columns. @param {{ search_fields?: string[], columns?: Column[], [key: string]: any }} w */
export function searchFields(w) {
	return w.search_fields || textColumns(w.columns || []).map((c) => c.field);
}

/** `rows` where one of `fields`, as text, holds `query`, any case; all of them for a blank one. @param {Row[]} rows @param {string[]} fields @param {string} query */
export function searchIn(rows, fields, query) {
	const q = query.trim().toLowerCase();
	if (!q) return rows;
	return rows.filter((r) => fields.some((f) => (text(r.f?.[f]) ?? '').toLowerCase().includes(q)));
}

/**
 * `rows` by `field`, largest first with `desc`: numbers as numbers, words as words, a row without
 * the field last. Stable. @param {Row[]} rows @param {string} field @param {boolean} desc
 */
export function sortRows(rows, field, desc) {
	const present = rows.filter((r) => r.f?.[field] != null);
	const absent = rows.filter((r) => r.f?.[field] == null);
	const numeric = present.every((r) => num(r.f?.[field]) != null);
	const sorted = present
		.map((r, i) => ({ r, i }))
		.sort((a, b) => {
			let c;
			if (numeric) c = /** @type {number} */ (num(a.r.f?.[field])) - /** @type {number} */ (num(b.r.f?.[field]));
			else c = (text(a.r.f?.[field]) ?? '').toLowerCase().localeCompare((text(b.r.f?.[field]) ?? '').toLowerCase());
			if (desc) c = -c;
			return c || a.i - b.i;
		})
		.map((x) => x.r);
	return [...sorted, ...absent];
}

/**
 * How much of the bar of `column` is filled in each of `rows`, 0 to 1; null where it can't be told.
 * Full at a number, at a field of the row, or at "top", a quarter above the column's largest.
 * @param {Row[]} rows @param {Column} column @returns {(number | null)[]}
 */
export function barFractions(rows, column) {
	/** @type {number | null | undefined} */
	let top;
	const largest = () => {
		if (top === undefined) {
			const vs = rows.map((r) => num(r.f?.[column.field])).filter((v) => v != null);
			top = vs.length ? Math.max(.../** @type {number[]} */ (vs)) * TOP_HEADROOM : null;
		}
		return top;
	};
	return rows.map((r) => {
		const v = num(r.f?.[column.field]);
		const max = column.bar ? column.bar.max : undefined;
		if (v == null || max == null) return null;
		let full = num(max);
		if (full == null && typeof max === 'string') full = max === TOP ? largest() : num(r.f?.[max]);
		if (full == null || full <= 0) return null;
		return Math.min(Math.max(v / full, 0), 1);
	});
}

/**
 * `template`'s values with each that is exactly "{field}" taken from `row` as the value it is
 * there (a number stays a number); one whose field the row lacks is left out.
 * @param {Record<string, unknown> | null | undefined} template @param {Record<string, unknown>} row @returns {Record<string, unknown>}
 */
export function fillParams(template, row) {
	/** @type {Record<string, unknown>} */
	const out = {};
	for (const [k, v] of Object.entries(template || {})) {
		const m = typeof v === 'string' ? FIELD.exec(v) : null;
		if (!m) {
			out[k] = v;
			continue;
		}
		const value = row[m[1]];
		if (value != null) out[k] = value;
	}
	return out;
}

/**
 * What a row offers of a table's row actions: the dashboard one opens, `<open>.<the row's key>`,
 * while `ui` has it; or the action one runs, while `ui` has that. One with neither isn't offered.
 * @param {{ dashboards?: { id: string }[], actions?: { id: string }[], [key: string]: any }} ui @param {{ row_actions?: any[], [key: string]: any }} w @param {Row} row
 * @returns {{ rowAction: any, opens: string | null, runs: any }[]}
 */
export function rowButtons(ui, w, row) {
	const out = [];
	for (const ra of w.row_actions || []) {
		const target = ra.open ? `${ra.open}.${row.key}` : null;
		const opens = target && (ui.dashboards || []).some((d) => d.id === target) ? target : null;
		const runs = ra.open ? null : (ui.actions || []).find((a) => a.id === ra.action) || null;
		if (opens || runs) out.push({ rowAction: ra, opens, runs });
	}
	return out;
}

/**
 * A link's address as the browser opens it: "{host}", "localhost" and the loopback address in the
 * host's place become `host`, the name this page reaches the computer by; "{host}" elsewhere
 * becomes it too. A bare host:port gets http://. (The app's linkUrl.)
 * @param {string} url @param {string} host @returns {string}
 */
export function linkUrl(url, host) {
	const h = host.includes(':') && !host.startsWith('[') ? `[${host}]` : host;
	const u = SCHEME.test(url.trim()) ? url.trim() : `http://${url.trim()}`;
	const start = u.indexOf('://') + 3;
	let end = u.slice(start).search(/[/?#]/);
	end = end < 0 ? u.length : start + end;
	const authority = u.slice(start, end);
	const at = authority.lastIndexOf('@');
	const userInfo = authority.slice(0, at + 1);
	const hostPort = authority.slice(at + 1);
	let split;
	if (hostPort.startsWith('[')) {
		const close = hostPort.indexOf(']');
		split = close < 0 ? hostPort.length : close + 1;
	} else {
		const colon = hostPort.lastIndexOf(':');
		split = colon < 0 ? hostPort.length : colon;
	}
	const name = hostPort.slice(0, split);
	const newName = LOCAL_HOSTS.has(name.toLowerCase()) ? h : name;
	const rest = u.slice(end).replaceAll('{host}', h);
	return u.slice(0, start) + userInfo + newName + hostPort.slice(split) + rest;
}

/** Whether two values are the same value: a number as a number, whatever its writing. @param {unknown} a @param {unknown} b */
export function sameValue(a, b) {
	if (typeof a === 'number' && typeof b === 'number') return a === b;
	if (typeof a === 'boolean' && typeof b === 'boolean') return a === b;
	return typeof a === typeof b && a === b;
}
