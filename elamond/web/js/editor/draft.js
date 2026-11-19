// The editor's drafts: a dashboard and a profile as the service will write them, changed by the
// forms and the canvas. Pure (no DOM), tested under node. A draft keeps every key it was given,
// those no form knows included: what is sent back holds them all.

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
 * A draft and what it started from, to tell whether it changed.
 * @template T
 * @typedef {{ value: T, base: string }} Draft
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
	return { value: clone(value), base: stable(value) };
}

/** Whether a draft differs from what it started from. @param {Draft<any>} d */
export function isDirty(d) {
	return stable(d.value) !== d.base;
}

/** Takes the draft as it is now as what it started from (after a save). @param {Draft<any>} d */
export function rebase(d) {
	d.base = stable(d.value);
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

/** Clamps a width to `min`–`max` and sets it. @param {DraftWidget} widget @param {number} span @param {number} [min] @param {number} [max] */
export function setSpan(widget, span, min = 1, max = 4) {
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

// ---------------------------------------------------------------- a profile's tabs

/** @param {DraftProfile} p @param {string} id @param {number} [index] */
export function addTab(p, id, index) {
	const tabs = Array.isArray(p.dashboards) ? p.dashboards : (p.dashboards = []);
	if (tabs.includes(id)) return false;
	tabs.splice(index == null ? tabs.length : Math.max(0, Math.min(index, tabs.length)), 0, id);
	return true;
}

/** @param {DraftProfile} p @param {number} index */
export function removeTab(p, index) {
	if (!Array.isArray(p.dashboards)) return;
	p.dashboards.splice(index, 1);
}

/** Moves a tab from one place to another (`to` counted after it is taken out). @param {DraftProfile} p @param {number} from @param {number} to */
export function moveTab(p, from, to) {
	const tabs = p.dashboards;
	if (!Array.isArray(tabs) || from < 0 || from >= tabs.length) return;
	const [id] = tabs.splice(from, 1);
	tabs.splice(Math.max(0, Math.min(to, tabs.length)), 0, id);
}

/** Sets one of `android`'s keys (`widget`, `tiles`, `card`). @param {DraftProfile} p @param {string} key @param {string} value */
export function setAndroid(p, key, value) {
	if (!p.android || typeof p.android !== 'object') p.android = {};
	p.android[key] = value;
}

// ---------------------------------------------------------------- the grid

/**
 * The widths a grid of `cols` columns draws widgets of these spans at, as the app does: on two
 * columns each span is halved rounding up, on one every widget is the whole width; a widget that
 * doesn't fit beside the one before starts a row, and the last of a row takes the columns left.
 * @param {number[]} spans in columns of four @param {number} cols 4, 2 or 1 @returns {number[]}
 */
export function flow(spans, cols) {
	const s = spans.map((x) => {
		const n = Math.max(1, Math.min(4, Math.round(Number(x) || 1)));
		return cols >= 4 ? n : cols === 2 ? Math.ceil(n / 2) : 1;
	});
	const out = s.slice();
	let used = 0;
	let last = -1;
	for (let i = 0; i < s.length; i++) {
		if (used + s[i] > cols && last >= 0) {
			out[last] += cols - used;
			used = 0;
		}
		used += s[i];
		last = i;
		if (used >= cols) {
			used = 0;
			last = -1;
		}
	}
	if (last >= 0) out[last] += cols - used;
	return out;
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

/** Whether a string is an id as the settings take one: 1–32 of a-z, 0-9, `_`, `-`. @param {unknown} s */
export function isId(s) {
	return typeof s === 'string' && /^[a-z0-9_-]{1,32}$/.test(s);
}
