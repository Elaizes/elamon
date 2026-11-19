// Light or dark: this browser's choice, else the profile's `theme`, else the system's.
import * as state from './state.js';

/** @type {import('./types.js').Ui | null} */
let last = null;

/** @param {import('./types.js').Ui | null} ui */
export function applyTheme(ui) {
	last = ui;
	const own = state.load('theme', 'auto');
	const theme = own !== 'auto' ? own : ui ? ui.profile.theme : 'system';
	const html = document.documentElement;
	if (theme === 'light' || theme === 'dark') html.dataset.theme = theme;
	else delete html.dataset.theme;
	html.dataset.black = ui && ui.profile.pure_black ? 'true' : 'false';
	html.dataset.still = ui && ui.profile.animations === false ? 'true' : 'false';
}

/** This browser's choice now: `auto` (the profile's), `light` or `dark`. @returns {'auto' | 'light' | 'dark'} */
export function chosen() {
	return state.load('theme', 'auto');
}

/** Cycles this browser's choice: the profile's, light, dark. */
export function cycleTheme() {
	const order = ['auto', 'light', 'dark'];
	state.save('theme', order[(order.indexOf(chosen()) + 1) % order.length]);
	applyTheme(last);
}
