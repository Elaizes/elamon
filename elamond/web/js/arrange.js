// The page's layout, as this browser picks it in the Layout menu: how wide the page is, how many
// columns the dashboards have, how dense the cards are and how far the page stands from the
// window's edge. Kept like the theme (state.js), one key with only what was picked; each shows as
// an attribute on html (`data-width`, `data-columns`, `data-density`, `data-inset`), which boot.js
// sets before the first frame and style.css turns into its tokens. Whether the side bar is folded
// to its icons is kept beside it (`data-side`). Nothing of this is the computer's.
import * as state from './state.js';

/**
 * @typedef {{ width: 'full' | 'fixed', columns: import('./layout.js').ColumnsPick, density: 'compact' | 'normal' | 'roomy', inset: 'none' | 'small' | 'normal' | 'large' }} Layout
 */

/** Each choice's values, in the menu's order. @type {{ [K in keyof Layout]: Layout[K][] }} */
export const PICKS = {
	width: ['full', 'fixed'],
	columns: ['auto', '4', '8'],
	density: ['compact', 'normal', 'roomy'],
	inset: ['none', 'small', 'normal', 'large'],
};

/** What a choice is until one is picked: the whole window; 4 columns, 8 on a wide one; 0.11.3's density and edge. @type {Layout} */
export const DEFAULTS = { width: 'full', columns: 'auto', density: 'normal', inset: 'normal' };

/** The keys, in the menu's order. @type {(keyof Layout)[]} */
export const KEYS = /** @type {any} */ (Object.keys(PICKS));

/**
 * The layout from what was kept: each choice that is one of its values, the rest as they are
 * until picked. Anything else kept (an old or broken value, not an object) counts as nothing. Pure.
 * @param {unknown} saved @returns {Layout}
 */
export function layoutOf(saved) {
	const s = saved && typeof saved === 'object' && !Array.isArray(saved) ? /** @type {Record<string, unknown>} */ (saved) : {};
	/** @type {any} */
	const out = {};
	for (const k of KEYS) out[k] = /** @type {unknown[]} */ (PICKS[k]).includes(s[k]) ? s[k] : DEFAULTS[k];
	return out;
}

/** Only what was picked, of what was kept. @returns {Partial<Layout>} */
function picked() {
	const kept = state.load('layout', null);
	const all = layoutOf(kept);
	/** @type {any} */
	const out = {};
	if (kept && typeof kept === 'object') for (const k of KEYS) if (kept[k] === all[k]) out[k] = all[k];
	return out;
}

/** The layout shown now. @returns {Layout} */
export function current() {
	return layoutOf(state.load('layout', null));
}

/** @type {Set<(layout: Layout) => void>} */
const listeners = new Set();

/** Hears a pick (the dashboards lay out their columns again). @param {(layout: Layout) => void} cb @returns {() => void} */
export function onLayout(cb) {
	listeners.add(cb);
	return () => listeners.delete(cb);
}

/** Sets html's attributes for the layout kept. @returns {Layout} */
export function applyLayout() {
	const l = current();
	const html = document.documentElement;
	for (const k of KEYS) html.dataset[k] = l[k];
	for (const f of listeners) f(l);
	return l;
}

/** A pick from the menu. @template {keyof Layout} K @param {K} key @param {Layout[K]} value */
export function choose(key, value) {
	state.save('layout', { ...picked(), [key]: value });
	applyLayout();
}

/** Back to what is there until picked. */
export function reset() {
	state.save('layout', undefined);
	applyLayout();
}

/** Whether the side bar is folded to its icons. */
export function folded() {
	return state.load('side', null) === 'rail';
}

/** Folds the side bar to its icons, or opens it out. @param {boolean} rail */
export function fold(rail) {
	state.save('side', rail ? 'rail' : undefined);
	document.documentElement.dataset.side = rail ? 'rail' : 'open';
}
