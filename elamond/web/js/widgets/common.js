// What the widgets share: their cards and tiles, colours and levels, captions, folding, and what a
// view hands each of them (`Env`). A widget module exports `create(w, env)`, which returns an
// `Instance`: its element, and `update` for each data line (its state, and the line for its time
// and history).
import { h, setText, setStyle } from '../dom.js';
import { icon } from '../icons.js';
import { S, fill } from '../strings.js';
import * as F from '../format.js';
import * as state from '../state.js';

/**
 * @typedef {import('../types.js').Widget} Widget
 * @typedef {import('../types.js').WidgetState} WidgetState
 * @typedef {import('../types.js').Data} Data
 * @typedef {import('../types.js').Action} Action
 * @typedef {import('../types.js').Ui} Ui
 *
 * @typedef {{ el: HTMLElement, update: (state: WidgetState | undefined, data: Data) => void, destroy?: () => void,
 *   layout?: (columns: number) => void, clear?: () => void, shown?: () => boolean }} Instance
 *
 * What a view hands its widgets.
 * @typedef {{
 *   ui: Ui, dashboard: string, isTab: boolean,
 *   live: boolean,
 *   action: (id: string | null | undefined) => Action | null,
 *   controlOn: boolean,
 *   intervalMs: number, historyMs: number,
 *   memory: Map<string, any>,
 *   asked: { sort: Record<string, string>, log: Record<string, { level: string, search?: string }> },
 *   make: (w: Widget, env: Env) => Instance,
 *   inGroup?: boolean,
 *   canOpen: (dashboard: string | null | undefined) => boolean,
 *   open: (dashboard: string) => void,
 *   run: (action: Action, params: Record<string, unknown>, w: Widget, phrase?: string) => Promise<any>,
 *   change: (body: Record<string, unknown>) => void,
 * }} Env
 */

/** The colours the settings may name (docs/config.md, "Colours"), each a CSS variable of the theme. */
const COLOURS = new Set(['cpu', 'memory', 'swap', 'gpu', 'rx', 'tx', 'read', 'write', 'battery', 'good', 'warn', 'hot', 'primary', 'secondary', 'tertiary']);

/** A colour the settings name, as CSS; the primary for none, or one the page doesn't know. @param {string | null | undefined} name */
export function accent(name) {
	return name && COLOURS.has(name) ? `var(--${name})` : 'var(--primary)';
}

/** @param {HTMLElement | SVGElement} el @param {string | null | undefined} name */
export function setAccent(el, name) {
	setStyle(el, { '--accent': accent(name) });
}

const LEVELS = ['normal', 'warning', 'critical'];

/**
 * The service's level and blend on `el`: `lv-normal` (turning amber by `--blend`), `lv-warning`
 * (turning red), `lv-critical`; none for no level, or one the page doesn't know. What a normal
 * level looks like is the element's `--lv-good`.
 * @param {HTMLElement | SVGElement} el @param {string | null | undefined} level @param {number | null | undefined} [blend]
 */
export function setLevel(el, level, blend) {
	for (const l of LEVELS) el.classList.toggle(`lv-${l}`, l === level);
	const b = typeof blend === 'number' && blend > 0 ? Math.min(blend, 1) : null;
	setStyle(el, { '--blend': level && b != null ? b : null });
}

/** A widget's line under it: its own text, or the state's where it holds values. @param {Widget} w @param {WidgetState | undefined} s */
export function captionOf(w, s) {
	if (typeof w.caption === 'string' && w.caption) return w.caption;
	return s && typeof s.caption === 'string' && s.caption ? s.caption : null;
}

/** The figure of a state: the word the service gave, the value by format, or a dash. @param {Widget | { format?: string, unit?: string }} w @param {WidgetState | { v?: any, text?: string } | undefined} s */
export function figureText(w, s) {
	if (s && typeof s.text === 'string') return s.text;
	return F.format(s ? s.v : null, w.format, w.unit) ?? F.DASH;
}

/** The value of a state as a number; null when missing or not one. @param {WidgetState | undefined} s */
export function numberOf(s) {
	return s && typeof s.v === 'number' && Number.isFinite(s.v) ? s.v : null;
}

/** How full a gauge or meter is: its value between its min and its max (the state's, where it changes). @param {Widget} w @param {WidgetState | undefined} s */
export function fraction(w, s) {
	const v = numberOf(s);
	if (v == null) return 0;
	const min = typeof w.min === 'number' ? w.min : 0;
	const max = s && typeof s.max === 'number' ? s.max : typeof w.max === 'number' ? w.max : w.format === 'percent' ? 100 : null;
	if (max == null || max <= min) return 0;
	return Math.min(Math.max((v - min) / (max - min), 0), 1);
}

/** Whether a widget is foldable: a group or table with `collapsed`. @param {Widget} w */
export function foldable(w) {
	return typeof w.collapsed === 'boolean';
}

/** Whether `w` is folded: as it started, until folded or opened here, then as left. @param {Widget} w */
export function isFolded(w) {
	if (!foldable(w)) return false;
	const kept = state.load('folded', {});
	return typeof kept[w.id] === 'boolean' ? kept[w.id] : !!w.collapsed;
}

/** @param {Widget} w @param {boolean} folded */
function keepFolded(w, folded) {
	const kept = state.load('folded', {});
	kept[w.id] = folded;
	state.save('folded', kept);
}

/**
 * A card for `w`: its icon and label as the title, its body under it. `tile` cards (a figure, a
 * ring, a bar) open their dashboard with a tap anywhere; others have a button by the title for it,
 * as they hold controls of their own. A foldable one has a button that folds its body away.
 * @param {Widget} w @param {Env} env
 * @param {{ tile?: boolean, icon?: string, title?: string, trailing?: Node, cls?: string, onFold?: (folded: boolean) => void }} [opts]
 * @returns {{ el: HTMLElement, head: HTMLElement, body: HTMLElement, trailing: HTMLElement }}
 */
export function card(w, env, opts = {}) {
	const title = opts.title ?? w.label ?? '';
	const titleEl = h('h3', { class: 'w-title' }, title);
	const trailing = h('div', { class: 'w-trailing' });
	if (opts.trailing) trailing.append(opts.trailing);
	const head = h('div', { class: 'w-head' }, icon(w.icon ?? opts.icon ?? null, 'w-icon'), titleEl, trailing);
	const body = h('div', { class: 'w-body' });
	const el = h('section', { class: `card w w-${w.type}${opts.tile ? ' tile' : ''}${opts.cls ? ` ${opts.cls}` : ''}`, 'data-widget': w.id }, head, body);
	if (title) el.setAttribute('aria-label', title);
	else titleEl.remove();
	setAccent(el, w.color);
	if (w.open && env.canOpen(w.open)) {
		const target = env.ui.dashboards.find((d) => d.id === w.open);
		const label = fill(S.panels.opens, target?.title || w.open);
		if (opts.tile) {
			el.classList.add('opens');
			el.tabIndex = 0;
			el.setAttribute('role', 'link');
			el.setAttribute('aria-label', title ? `${title}. ${label}` : label);
			el.addEventListener('click', () => env.open(/** @type {string} */ (w.open)));
			el.addEventListener('keydown', (e) => {
				if (e.key === 'Enter' || e.key === ' ') {
					e.preventDefault();
					env.open(/** @type {string} */ (w.open));
				}
			});
		} else {
			trailing.append(h('button', { class: 'icon-button quiet', type: 'button', 'aria-label': label, title: label, onclick: () => env.open(/** @type {string} */ (w.open)) }, icon('chevron')));
		}
	}
	if (foldable(w)) {
		let folded = isFolded(w);
		const button = h('button', { class: 'icon-button quiet w-fold', type: 'button' });
		const show = () => {
			el.classList.toggle('folded', folded);
			button.setAttribute('aria-expanded', String(!folded));
			button.setAttribute('aria-label', folded ? S.panels.unfold : S.panels.fold);
			button.title = folded ? S.panels.unfold : S.panels.fold;
			button.replaceChildren(icon(folded ? 'expand_more' : 'expand_less'));
		};
		button.addEventListener('click', () => {
			folded = !folded;
			keepFolded(w, folded);
			show();
			opts.onFold?.(folded);
		});
		trailing.append(button);
		show();
	}
	return { el, head, body, trailing };
}

/** A line under a widget, from its first text on: kept (empty) while a reading is missing, so nothing below moves. @returns {{ el: HTMLElement, set: (text: string | null) => void }} */
export function captionLine() {
	const el = h('p', { class: 'w-caption muted' });
	el.hidden = true;
	let had = false;
	return {
		el,
		set(text) {
			if (text) had = true;
			el.hidden = !had;
			setText(el, text || (had ? '\u00a0' : ''));
		},
	};
}

/**
 * A figure large: the number with its unit set smaller, or the word the service gave instead,
 * or a dash. @returns {{ el: HTMLElement, set: (w: Widget | { format?: string, unit?: string }, s: WidgetState | undefined) => void }}
 */
export function figure(large = true) {
	const num = h('span', { class: 'fig-num' });
	const unit = h('span', { class: 'fig-unit' });
	const el = h('div', { class: large ? 'fig large' : 'fig' }, num, unit);
	return {
		el,
		set(w, s) {
			el.classList.toggle('word', !!(s && typeof s.text === 'string'));
			if (s && typeof s.text === 'string') {
				setText(num, s.text);
				setText(unit, '');
				return;
			}
			const n = numberOf(s);
			if (n != null) {
				const [a, b] = F.parts(n, w.format, w.unit);
				setText(num, a);
				setText(unit, b ? (b === '%' || b === '°C' || b === '°F' ? b : ` ${b}`) : '');
				return;
			}
			setText(num, F.format(s ? s.v : null, w.format, w.unit) ?? F.DASH);
			setText(unit, '');
		},
	};
}

/** A notice in a card: amber for a warning, red when critical. @param {string | null | undefined} title @param {string} text @param {string} level */
export function notice(title, text, level) {
	const el = h('section', { class: `card w notice ${level === 'critical' ? 'critical' : 'warning'}`, role: level === 'critical' ? 'alert' : 'status' },
		h('div', { class: 'w-head' }, icon('warning', 'w-icon'), h('h3', { class: 'w-title' }, title || S.panels.notice)),
		h('p', { class: 'notice-text' }, text));
	return el;
}

/** Chips to pick one of, as a radio group the arrow keys move through. @param {string} label @param {{ value: string, text: string }[]} items @param {string} selected @param {(value: string) => void} onPick */
export function chips(label, items, selected, onPick) {
	const el = h('div', { class: 'chips', role: 'radiogroup', 'aria-label': label });
	/** @type {HTMLButtonElement[]} */
	const buttons = [];
	const select = (/** @type {string} */ value) => {
		for (const b of buttons) {
			const on = b.dataset.value === value;
			b.setAttribute('aria-checked', String(on));
			b.tabIndex = on ? 0 : -1;
		}
	};
	items.forEach((item, i) => {
		const b = h('button', { class: 'chip', type: 'button', role: 'radio', 'data-value': item.value }, item.text);
		b.addEventListener('click', () => {
			if (b.getAttribute('aria-checked') === 'true') return;
			select(item.value);
			onPick(item.value);
		});
		b.addEventListener('keydown', (e) => {
			const d = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
			if (!d) return;
			e.preventDefault();
			const next = buttons[(i + d + buttons.length) % buttons.length];
			next.focus();
			next.click();
		});
		buttons.push(b);
		el.append(b);
	});
	select(selected);
	return { el, select };
}

/** Whether a widget's state says it shows nothing to act on (a control the service turned off). @param {WidgetState | undefined} s */
export function stateEnabled(s) {
	return !(s && s.enabled === false);
}
