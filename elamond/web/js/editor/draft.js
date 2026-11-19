// The editor's drafts: a dashboard and a profile as the service will write them, changed by the
// forms and the canvas. Pure (no DOM), tested under node. A draft keeps every key it was given,
// those no form knows included: what is sent back holds them all.
import { COLUMNS, SPAN_MIN } from '../consts.js';

/**
 * A widget of a draft: the settings' own table, any keys.
 * @typedef {Record<string, any>} DraftWidget
 *
 * A dashboard as the editor writes it: `title`, `icon`, `role`, `for_each`, `widgets`, and any key
 * it came with.
 * @typedef {{ widgets?: DraftWidget[], [key: string]: any }} DraftDashboard
 *
 * A profile's part the editor writes: `dashboards`, `header`, `android.widget/tiles/card`.
 * @typedef {{ dashboards?: string[], header?: string, android?: Record<string, any>, [key: string]: any }} DraftProfile
 *
 * Where a widget is: its index among the dashboard's widgets, then among each group's.
 * @typedef {number[]} WidgetPath
 */

/**
 * A draft and what it started from (as it was, and as JSON to compare), to tell what changed.
 * @template T
 * @typedef {{ value: T, base: string, start: T }} Draft
 */

/** The key a dashboard and a group hold their widgets in (the protocol's, as in mistakes' paths). */
export const WIDGETS = 'widgets';

/** A deep copy of JSON data. @template T @param {T} v @returns {T} */
export function clone(v) {
	return v === undefined ? v : JSON.parse(JSON.stringify(v));
}

/** JSON with each table's keys sorted, to compare two drafts whatever order their keys came in. @param {unknown} v */
export function stable(v) {
	return JSON.stringify(v, (k, x) => {
		if (x && typeof x === 'object' && !Array.isArray(x)) {
			/** @type {Record<string, unknown>} */
			const out = {};
			for (const key of Object.keys(x).sort()) out[key] = x[key];
			return out;
		}
		return x;
	});
}

/** @template T @param {T} value @returns {Draft<T>} */
export function draft(value) {
	return { value: clone(value), base: stable(value), start: clone(value) };
}

/** Whether a draft differs from what it started from. @param {Draft<any>} d */
export function isDirty(d) {
	return stable(d.value) !== d.base;
}

/** Takes the draft as it is now as what it started from (after a save). @param {Draft<any>} d */
export function rebase(d) {
	d.base = stable(d.value);
	d.start = clone(d.value);
}

/**
 * What of a table changed from what it started as, as a profile's write sends it: each key whose
 * value changed, `null` for one taken out (back to what the other files say), and of a table in it
 * (a profile's `android`), the keys of that which did. A key that didn't change is left out: the
 * editor's file keeps it as it has it. Never the values it only shows from the files.
 * @param {Record<string, any> | null | undefined} start @param {Record<string, any> | null | undefined} now
 * @returns {Record<string, any>}
 */
export function changes(start, now) {
	const table = (/** @type {unknown} */ v) => !!v && typeof v === 'object' && !Array.isArray(v);
	const a0 = start || {};
	const b0 = now || {};
	/** @type {Record<string, any>} */
	const out = {};
	for (const k of new Set([...Object.keys(a0), ...Object.keys(b0)])) {
		const a = a0[k];
		const b = b0[k];
		if (stable(a) === stable(b)) continue;
		if (table(a) && (table(b) || b === undefined)) {
			const inner = changes(a, b);
			if (Object.keys(inner).length) out[k] = inner;
		} else out[k] = b === undefined ? null : clone(b);
	}
	return out;
}

/** Sets a key, or takes it out with `undefined`. @param {Record<string, any>} obj @param {string} key @param {unknown} value */
export function setKey(obj, key, value) {
	if (value === undefined) delete obj[key];
	else obj[key] = value;
}

// ---------------------------------------------------------------- widgets in a dashboard

/**
 * The list of widgets a path's parent holds: the dashboard's for `[]`, a group's for its path.
 * With `make`, a group without a list is given one.
 * @param {DraftDashboard} dash @param {WidgetPath} parent @param {boolean} [make]
 * @returns {DraftWidget[] | null}
 */
export function listAt(dash, parent, make = false) {
	/** @type {Record<string, any>} */
	let holder = dash;
	for (const i of parent) {
		const list = holder[WIDGETS];
		if (!Array.isArray(list) || !list[i] || typeof list[i] !== 'object') return null;
		holder = list[i];
	}
	if (!Array.isArray(holder[WIDGETS])) {
		if (!make) return null;
		holder[WIDGETS] = [];
	}
	return holder[WIDGETS];
}

/** The widget at a path, or null. @param {DraftDashboard} dash @param {WidgetPath} path @returns {DraftWidget | null} */
export function widgetAt(dash, path) {
	if (!path.length) return null;
	const list = listAt(dash, path.slice(0, -1));
	const w = list ? list[path[path.length - 1]] : undefined;
	return w && typeof w === 'object' ? w : null;
}

/** Whether `inner` is `outer` or inside it. @param {WidgetPath} outer @param {WidgetPath} inner */
export function within(outer, inner) {
	return outer.length <= inner.length && outer.every((x, i) => inner[i] === x);
}

/** @param {WidgetPath} a @param {WidgetPath} b */
export function samePath(a, b) {
	return a.length === b.length && a.every((x, i) => b[i] === x);
}

/**
 * Puts a widget into a list: the dashboard's (`parent` `[]`) or a group's, at `index` (the end when
 * past it). @returns {WidgetPath} where it is
 * @param {DraftDashboard} dash @param {WidgetPath} parent @param {number} index @param {DraftWidget} widget
 */
export function addWidget(dash, parent, index, widget) {
	const list = listAt(dash, parent, true);
	if (!list) throw new Error('no such group');
	const at = Math.max(0, Math.min(index, list.length));
	list.splice(at, 0, widget);
	return [...parent, at];
}

/** Takes a widget out. @param {DraftDashboard} dash @param {WidgetPath} path @returns {DraftWidget | null} the widget taken out */
export function removeWidget(dash, path) {
	const list = listAt(dash, path.slice(0, -1));
	const i = path[path.length - 1];
	if (!list || i < 0 || i >= list.length) return null;
	return list.splice(i, 1)[0];
}

/**
 * Moves a widget to `index` of the list at `toParent` (`index` counted in that list as it is before
 * the move). A group can't go into itself or one of its own. @returns {WidgetPath | null} where it
 * is now, or null when it can't go there
 * @param {DraftDashboard} dash @param {WidgetPath} from @param {WidgetPath} toParent @param {number} index
 */
export function moveWidget(dash, from, toParent, index) {
	if (!widgetAt(dash, from) || within(from, toParent)) return null;
	const target = listAt(dash, toParent, true);
	if (!target) return null;
	const fromParent = from.slice(0, -1);
	const fromIndex = from[from.length - 1];
	let at = Math.max(0, Math.min(index, target.length));
	// Taking it out first shifts what follows it in its own list, and the path of a list after it.
	const parent = [...toParent];
	if (samePath(fromParent, toParent) && fromIndex < at) at -= 1;
	if (parent.length > fromParent.length && within(fromParent, parent.slice(0, fromParent.length)) && parent[fromParent.length] > fromIndex) {
		parent[fromParent.length] -= 1;
	}
	const w = removeWidget(dash, from);
	if (!w) return null;
	const list = listAt(dash, parent, true);
	if (!list) return null;
	at = Math.min(at, list.length);
	list.splice(at, 0, w);
	return [...parent, at];
}

/**
 * Moves a widget one place up (`-1`) or down (`1`): within its list, and out of its group past
 * either end. @returns {WidgetPath} where it is now (the same when it can't go)
 * @param {DraftDashboard} dash @param {WidgetPath} path @param {-1 | 1} delta
 */
export function nudge(dash, path, delta) {
	const parent = path.slice(0, -1);
	const list = listAt(dash, parent);
	const i = path[path.length - 1];
	if (!list) return path;
	const j = i + delta;
	if (j >= 0 && j < list.length) return moveWidget(dash, path, parent, delta > 0 ? j + 1 : j) || path;
	if (!parent.length) return path;
	// Past the end of its group: out of it, just before or after the group.
	const groupParent = parent.slice(0, -1);
	const g = parent[parent.length - 1];
	return moveWidget(dash, path, groupParent, delta > 0 ? g + 1 : g) || path;
}

/** Clamps a width to `min`–`max` (the grid's, unless the description says) and sets it. @param {DraftWidget} widget @param {number} span @param {number} [min] @param {number} [max] */
export function setSpan(widget, span, min = SPAN_MIN, max = COLUMNS) {
	widget.span = Math.max(min, Math.min(max, Math.round(span)));
	return widget.span;
}

/**
 * Every widget's path, depth first, groups before what they hold. @param {DraftDashboard} dash
 * @returns {WidgetPath[]}
 */
export function allPaths(dash) {
	/** @type {WidgetPath[]} */
	const out = [];
	/** @param {DraftWidget[]} list @param {WidgetPath} at */
	const walk = (list, at) => {
		list.forEach((w, i) => {
			const p = [...at, i];
			out.push(p);
			if (w && Array.isArray(w[WIDGETS])) walk(w[WIDGETS], p);
		});
	};
	walk(Array.isArray(dash[WIDGETS]) ? dash[WIDGETS] : [], []);
	return out;
}

// ---------------------------------------------------------------- writing

/**
 * Writes one after another, each on the hash of the editor's file the one before it left (the
 * service answers each with the file's `hash` now), stopping at the first refused. What is had
 * after: the hash to write on next (the last written, or `base` when none went), the generation,
 * how many went, and what refused the next, if one did.
 * @param {((base: string | null) => Promise<any>)[]} steps @param {string | null} base
 * @returns {Promise<{ hash: string | null, generation: number | null, done: number, error: unknown }>}
 */
export async function writeInTurn(steps, base) {
	let hash = base;
	/** @type {number | null} */
	let generation = null;
	let done = 0;
	for (const step of steps) {
		try {
			const r = await step(hash);
			if (r && 'hash' in r) hash = r.hash ?? null;
			if (r && typeof r.generation === 'number') generation = r.generation;
			done++;
		} catch (error) {
			return { hash, generation, done, error };
		}
	}
	return { hash, generation, done, error: null };
}

// ---------------------------------------------------------------- a profile's tabs

/** @param {DraftProfile} p @param {string} id @param {number} [index] */
export function addTab(p, id, index) {
	const tabs = Array.isArray(p.dashboards) ? p.dashboards : (p.dashboards = []);
	if (tabs.includes(id)) return false;
	tabs.splice(index == null ? tabs.length : Math.max(0, Math.min(index, tabs.length)), 0, id);
	return true;
}

// ---------------------------------------------------------------- mistakes

/**
 * A settings path in parts: `dashboards.mine.widgets[1].max` → `dashboards`, `mine`, `widgets`, 1,
 * `max`. A quoted part (`map."On battery"`) is one part.
 * @param {string} path @returns {(string | number)[]}
 */
export function pathParts(path) {
	/** @type {(string | number)[]} */
	const out = [];
	let i = 0;
	let cur = '';
	const push = () => {
		if (cur) out.push(cur);
		cur = '';
	};
	while (i < path.length) {
		const c = path[i];
		if (c === '"') {
			const end = path.indexOf('"', i + 1);
			cur += path.slice(i + 1, end < 0 ? path.length : end);
			i = end < 0 ? path.length : end + 1;
		} else if (c === '.') {
			push();
			i++;
		} else if (c === '[') {
			push();
			const end = path.indexOf(']', i);
			const inner = path.slice(i + 1, end < 0 ? path.length : end);
			out.push(/^\d+$/.test(inner) ? Number(inner) : inner.replace(/^"|"$/g, ''));
			i = end < 0 ? path.length : end + 1;
		} else {
			cur += c;
			i++;
		}
	}
	push();
	return out;
}

/**
 * What a mistake's `path` names: the dashboard or profile, the widget in it (a path through the
 * groups), the key, and the rest past the key (`columns[0].field`).
 * @param {string} path
 * @returns {{ section: string, id: string | null, widget: WidgetPath | null, key: string | null, rest: string }}
 */
export function locate(path) {
	const parts = pathParts(path || '');
	const section = String(parts[0] ?? '');
	const id = parts.length > 1 ? String(parts[1]) : null;
	let i = 2;
	/** @type {WidgetPath} */
	const widget = [];
	if (section === 'dashboards') {
		while (parts[i] === WIDGETS && typeof parts[i + 1] === 'number') {
			widget.push(/** @type {number} */ (parts[i + 1]));
			i += 2;
		}
	}
	const key = i < parts.length ? String(parts[i]) : null;
	const rest = parts
		.slice(i + 1)
		.map((p) => (typeof p === 'number' ? `[${p}]` : `.${p}`))
		.join('')
		.replace(/^\./, '');
	return { section, id, widget: widget.length ? widget : null, key, rest };
}

/** A widget path as a string, to key maps by. @param {WidgetPath} path */
export function pathKey(path) {
	return path.join('/');
}

/**
 * The mistakes of one dashboard and one profile, by where they go: each widget's (by `pathKey`, then
 * key; `''` for the widget as a whole), the dashboard's own keys, the profile's, and the rest.
 * @param {{ path?: string | null, message?: string, file?: string, line?: number | null }[]} mistakes
 * @param {string | null} dashboard @param {string | null} profile
 */
export function sortMistakes(mistakes, dashboard, profile) {
	/** @type {Map<string, Map<string, string[]>>} */
	const widgets = new Map();
	/** @type {Map<string, string[]>} */
	const dash = new Map();
	/** @type {Map<string, string[]>} */
	const prof = new Map();
	/** @type {string[]} */
	const other = [];
	/** @param {Map<string, string[]>} m @param {string} k @param {string} text */
	const put = (m, k, text) => {
		const list = m.get(k) || [];
		list.push(text);
		m.set(k, list);
	};
	for (const m of mistakes || []) {
		const message = String(m.message || '');
		const at = locate(m.path || '');
		const text = at.rest ? `${at.rest}: ${message}` : message;
		if (at.section === 'dashboards' && at.id === dashboard && dashboard != null) {
			if (at.widget) {
				const k = pathKey(at.widget);
				if (!widgets.has(k)) widgets.set(k, new Map());
				put(/** @type {Map<string, string[]>} */ (widgets.get(k)), at.key || '', text);
			} else put(dash, at.key || '', text);
		} else if (at.section === 'profiles' && at.id === profile && profile != null) {
			// `android.widget` is a key of the form; `dashboards[2]` is the tabs'.
			const head = /^[^.[]+/.exec(at.rest);
			put(prof, head && at.key ? `${at.key}.${head[0]}` : at.key || '', head ? message : text);
		} else other.push(m.path ? `${m.path}: ${message}` : message);
	}
	return { widgets, dash, prof, other };
}

/** Whether a string is an id as the settings take one: 1–32 of a-z, 0-9, `_`, `-`, starting with a letter or a digit. @param {unknown} s */
export function isId(s) {
	return typeof s === 'string' && /^[a-z0-9][a-z0-9_-]{0,31}$/.test(s);
}
