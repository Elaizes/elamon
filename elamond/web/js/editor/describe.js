// What the service says the forms may offer (`GET /api/v1/admin/editor`), asked the questions the
// editor has: which keys a widget takes, how wide it is, where a dashboard comes from, what a
// reference may name. Pure (no DOM), tested under node. Nothing here lists types, keys or values
// of its own: all of it is the description's.
import { clone, WIDGETS } from './draft.js';

/**
 * A key a form offers, or (without `key`) the shape of a list's, map's or `any`'s items.
 * @typedef {{ key?: string, kind: string, label?: string, hint?: string, required?: boolean, default?: any,
 *   min?: number, max?: number, choices?: string[], to?: string, of?: EditorKey | EditorKey[], record?: string,
 *   [more: string]: any }} EditorKey
 *
 * @typedef {{ type: string, label?: string, span?: number, keys: EditorKey[], example?: Record<string, any> }} EditorType
 *
 * @typedef {{ file: string, line?: number | null }} Whence
 *
 * @typedef {{ id: string, title?: string, def: Record<string, any>, from?: Whence[], editor?: boolean, own?: boolean }} EditorDashboard
 *
 * @typedef {{ id: string, name?: string, def: Record<string, any>, from?: Whence[], editor?: boolean, clients?: string[] }} EditorProfile
 *
 * @typedef {{ file: string, hash: string | null, writable: boolean, why: string | null, profile: string | null,
 *   dashboard: { keys: EditorKey[] }, common: EditorKey[], types: EditorType[], records: Record<string, EditorKey[]>,
 *   values: { address: string, now?: string | null, label?: string }[], collections: { name: string, fields?: string[], label?: string }[],
 *   actions: { id: string, name?: string, kind?: string }[], links: { id: string, name?: string }[],
 *   sources: { id: string, kind?: string }[], dashboards: EditorDashboard[], profiles: EditorProfile[] }} Editor
 *
 * An option of a picker: what it writes, what it shows, and a line under it.
 * @typedef {{ value: string, label?: string, detail?: string }} Option
 */

/** The description with every list there, whatever an older or newer service left out. @param {any} d @returns {Editor} */
export function normalise(d) {
	const o = d && typeof d === 'object' ? d : {};
	const arr = (/** @type {any} */ x) => (Array.isArray(x) ? x : []);
	return {
		...o,
		file: String(o.file || ''),
		hash: o.hash ?? null,
		writable: o.writable !== false,
		why: o.why ?? null,
		profile: o.profile ?? null,
		dashboard: { keys: arr(o.dashboard && o.dashboard.keys) },
		common: arr(o.common),
		types: arr(o.types),
		records: o.records && typeof o.records === 'object' ? o.records : {},
		values: arr(o.values),
		collections: arr(o.collections),
		actions: arr(o.actions),
		links: arr(o.links),
		sources: arr(o.sources),
		dashboards: arr(o.dashboards),
		profiles: arr(o.profiles),
	};
}

/** A widget type's description, or null for one the service didn't describe. @param {Editor} d @param {unknown} type */
export function typeOf(d, type) {
	return d.types.find((t) => t.type === type) || null;
}

/**
 * The keys a widget takes: its type's, then those of every widget (`common`) its type doesn't
 * have. @param {Editor} d @param {Record<string, any>} widget
 * @returns {{ own: EditorKey[], common: EditorKey[] }}
 */
export function keysOf(d, widget) {
	const t = typeOf(d, widget.type);
	const own = t ? t.keys.filter((k) => k && k.key) : [];
	const names = new Set(own.map((k) => k.key));
	return { own, common: d.common.filter((k) => k && k.key && !names.has(k.key)) };
}

/**
 * The keys of a widget no description names, in the order it has them. `type` is the protocol's
 * own (what the widget is), offered on its own, never as one of these.
 * @param {Editor} d @param {Record<string, any>} widget
 */
export function unknownKeys(d, widget) {
	const { own, common } = keysOf(d, widget);
	const known = new Set([...own, ...common].map((k) => k.key));
	known.add('type');
	return Object.keys(widget).filter((k) => !known.has(k));
}

/** The key of `kind` `widgets` a type holds its widgets in (a group's), or null. @param {Editor} d @param {Record<string, any>} widget */
export function holdsWidgets(d, widget) {
	const t = typeOf(d, widget && widget.type);
	const k = t ? t.keys.find((x) => x.kind === 'widgets') : null;
	return k ? /** @type {string} */ (k.key) : null;
}

/** Whether a widget is one the canvas opens out (it holds widgets in `widgets`). @param {Editor} d @param {Record<string, any>} widget */
export function isGroup(d, widget) {
	return holdsWidgets(d, widget) === WIDGETS;
}

/** The common key `name`, if the service describes it. @param {Editor} d @param {string} name */
export function commonKey(d, name) {
	return d.common.find((k) => k.key === name) || null;
}

/** The widths a widget may take: `span`'s `min` and `max`, 1–4 when the description doesn't say. @param {Editor} d */
export function spanRange(d) {
	const k = commonKey(d, 'span');
	const min = k && typeof k.min === 'number' ? k.min : 1;
	const max = k && typeof k.max === 'number' ? k.max : 4;
	return { min, max };
}

/** How wide a widget is: its own `span`, else its type's, else `span`'s default. @param {Editor} d @param {Record<string, any>} widget */
export function spanOf(d, widget) {
	const { min, max } = spanRange(d);
	const t = typeOf(d, widget.type);
	const k = commonKey(d, 'span');
	const raw = typeof widget.span === 'number' ? widget.span : t && typeof t.span === 'number' ? t.span : k && typeof k.default === 'number' ? k.default : max;
	return Math.max(min, Math.min(max, Math.round(raw)));
}

/** A type's example, a copy to put in a draft (`{ type }` for one without). @param {Editor} d @param {string} type */
export function exampleOf(d, type) {
	const t = typeOf(d, type);
	return t && t.example && typeof t.example === 'object' ? { type, ...clone(t.example) } : { type };
}

/** What to call a widget: its label, else its type's, else its type. @param {Editor} d @param {Record<string, any>} widget */
export function nameOf(d, widget) {
	if (typeof widget.label === 'string' && widget.label.trim()) return widget.label;
	const t = typeOf(d, widget.type);
	return (t && t.label) || String(widget.type ?? '?');
}

/** A type's label, or the type. @param {Editor} d @param {unknown} type */
export function typeLabel(d, type) {
	const t = typeOf(d, type);
	return (t && t.label) || String(type ?? '?');
}

/**
 * What a `ref` may name, by its `to`: actions, dashboards, collections, links or sources.
 * @param {Editor} d @param {string | undefined} to @returns {Option[]}
 */
export function refOptions(d, to) {
	switch (to) {
		case 'action':
			return d.actions.map((a) => ({ value: a.id, label: a.name || a.id, detail: a.kind }));
		case 'dashboard':
			return d.dashboards.map((x) => ({ value: x.id, label: x.title || (x.def && x.def.title) || x.id }));
		case 'collection':
			return d.collections.map((c) => ({ value: c.name, label: c.label || c.name, detail: (c.fields || []).join(', ') }));
		case 'link':
			return d.links.map((l) => ({ value: l.id, label: l.name || l.id }));
		case 'source':
			return d.sources.map((s) => ({ value: s.id, label: s.id, detail: s.kind }));
		default:
			return [];
	}
}

/** The values there are, as a picker offers them: each address with its reading now. @param {Editor} d @returns {Option[]} */
export function valueOptions(d) {
	return d.values.map((v) => ({ value: v.address, label: v.label || v.address, detail: v.now == null ? '' : String(v.now) }));
}

/**
 * Options matching a search, best first: those starting with it, then those holding it anywhere.
 * @param {Option[]} options @param {string} query @param {number} [limit]
 */
export function search(options, query, limit = 60) {
	const q = query.trim().toLowerCase();
	if (!q) return options.slice(0, limit);
	/** @type {Option[]} */
	const first = [];
	/** @type {Option[]} */
	const then = [];
	for (const o of options) {
		const v = o.value.toLowerCase();
		const l = (o.label || '').toLowerCase();
		if (v.startsWith(q) || l.startsWith(q)) first.push(o);
		else if (v.includes(q) || l.includes(q) || (o.detail || '').toLowerCase().includes(q)) then.push(o);
	}
	return [...first, ...then].slice(0, limit);
}

/** The shapes an `any` key takes. @param {EditorKey} key @returns {EditorKey[]} */
export function anyOf(key) {
	return Array.isArray(key.of) ? key.of.filter((x) => x && typeof x === 'object') : key.of && typeof key.of === 'object' ? [key.of] : [];
}

/**
 * Whether a value is of a kind (as far as a client can tell: an address and a string look alike).
 * @param {EditorKey} k @param {unknown} v
 */
export function fits(k, v) {
	switch (k.kind) {
		case 'number':
			return typeof v === 'number';
		case 'int':
			return typeof v === 'number' && Number.isInteger(v);
		case 'bool':
			return typeof v === 'boolean';
		case 'scalar':
			return typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean';
		case 'string':
		case 'text':
		case 'id':
		case 'address':
		case 'ref':
		case 'format':
			return typeof v === 'string';
		case 'enum':
			return typeof v === 'string' && (!k.choices || k.choices.includes(v));
		case 'list':
			return Array.isArray(v);
		case 'map':
		case 'record':
			return !!v && typeof v === 'object' && !Array.isArray(v);
		case 'widgets':
			return Array.isArray(v);
		default:
			return false;
	}
}

/** Which of an `any` key's shapes a value has: the first it fits, -1 for none (or no value). @param {EditorKey} key @param {unknown} v */
export function whichOf(key, v) {
	if (v === undefined) return -1;
	return anyOf(key).findIndex((k) => fits(k, v));
}

/**
 * Where a dashboard or profile comes from, lowest first: the built-in defaults (no file), a file
 * and line, the Nix store, the editor's own file.
 * @param {Editor} d @param {{ from?: Whence[] }} entry
 * @returns {{ kind: 'defaults' | 'nix' | 'editor' | 'file', file: string, line: number | null }[]}
 */
export function origins(d, entry) {
	const from = Array.isArray(entry.from) ? entry.from : [];
	if (!from.length) return [{ kind: 'defaults', file: '', line: null }];
	return from.map((w) => {
		const file = String(w.file || '');
		const line = typeof w.line === 'number' ? w.line : null;
		if (!file) return { kind: 'defaults', file, line };
		if (file === d.file) return { kind: 'editor', file, line };
		if (file.startsWith('/nix/store/')) return { kind: 'nix', file, line };
		return { kind: 'file', file, line };
	});
}

/**
 * The dashboard keys a file below the editor's sets, which the editor can change but not take out
 * (the files merge key by key). `sure` is false when the editor's file sets the dashboard too: the
 * description doesn't say which of its keys come from below, so each might.
 * @param {Editor} d @param {EditorDashboard | null} entry
 * @returns {{ keys: Set<string>, where: string, sure: boolean }}
 */
export function keysFromBelow(d, entry) {
	if (!entry || entry.own) return { keys: new Set(), where: '', sure: true };
	const below = origins(d, entry).filter((o) => o.kind !== 'editor');
	if (!below.length) return { keys: new Set(), where: '', sure: true };
	const keys = new Set(Object.keys(entry.def || {}).filter((k) => k !== WIDGETS));
	const o = below[below.length - 1];
	const where = o.kind === 'defaults' ? '' : o.line != null ? `${o.file}:${o.line}` : o.file;
	return { keys, where, sure: !entry.editor };
}

/**
 * For the drawn dashboard: which of its widgets (resolved, as the preview's `ui` has them) each
 * widget of the draft's list became. A widget's drawn id is `<prefix><its id or place>`, with
 * `.<item>` after it for each item of a `for_each`; one the role doesn't see, or whose `when`
 * isn't there, became none.
 * @param {{ id: string, widgets?: any[] }[]} drawn the drawn list (a dashboard's or a group's)
 * @param {string} prefix the drawn dashboard's or group's id and `/`
 * @param {Record<string, any>[]} list the draft's list there
 * @returns {any[][]} one list per widget of the draft's
 */
export function matchDrawn(drawn, prefix, list) {
	/** @type {Map<string, number>} */
	const named = new Map();
	/** @type {Map<string, number>} */
	const placed = new Map();
	list.forEach((w, i) => {
		if (w && typeof w.id === 'string' && w.id) named.set(w.id, i);
		else {
			placed.set(String(i), i);
			placed.set(`w${i}`, i);
		}
	});
	/** @type {any[][]} */
	const out = list.map(() => []);
	for (const x of drawn || []) {
		if (!x || typeof x.id !== 'string' || !x.id.startsWith(prefix)) continue;
		const head = x.id.slice(prefix.length).split('/')[0].split('.')[0];
		const i = named.has(head) ? named.get(head) : placed.get(head);
		if (i !== undefined) out[i].push(x);
	}
	return out;
}
