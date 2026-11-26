// The page's theme: light, dark or black, as html's `data-theme`. This browser's choice when it made
// one (the theme menu); else the profile's `theme` and `pure_black`, its `system` the browser's own.
// The theme shown is kept as well, so the next load starts in it (boot.js) before `ui` comes.
import * as state from './state.js';

/** @typedef {'light' | 'dark' | 'black'} Theme */

/** The themes, in the menu's order. @type {Theme[]} */
export const THEMES = ['light', 'dark', 'black'];

/** The icon of each. @type {Record<Theme, string>} */
export const THEME_ICONS = { light: 'sun', dark: 'moon', black: 'contrast' };

const systemDark = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null;
/** @type {import('./types.js').Ui | null} */
let last = null;
let listening = false;

/** Whether `v` names a theme. @param {unknown} v @returns {v is Theme} */
export function isTheme(v) {
	return typeof v === 'string' && /** @type {string[]} */ (THEMES).includes(v);
}

/** This browser's choice; null when it made none (what 0.11.2 kept, `auto`, is none). @returns {Theme | null} */
export function chosen() {
	const c = state.load('theme', null);
	return isTheme(c) ? c : null;
}

/**
 * The theme to show: the choice, else the profile's (`system` the browser's, pure black making a
 * dark theme black). Pure.
 * @param {Theme | null} choice @param {{ theme?: string, pure_black?: boolean } | null | undefined} profile @param {boolean} darkSystem
 * @returns {Theme}
 */
export function themeOf(choice, profile, darkSystem) {
	if (choice) return choice;
	const t = profile && (profile.theme === 'light' || profile.theme === 'dark') ? profile.theme : darkSystem ? 'dark' : 'light';
	return t === 'dark' && profile && profile.pure_black ? 'black' : t;
}

/** The theme shown now. @returns {Theme} */
export function shown() {
	const t = document.documentElement.dataset.theme;
	return isTheme(t) ? t : 'light';
}

/** Shows the theme for `ui` (null: no profile yet, or none, as on pairing). @param {import('./types.js').Ui | null} ui */
export function applyTheme(ui) {
	last = ui;
	const html = document.documentElement;
	const theme = themeOf(chosen(), ui ? ui.profile : null, !!systemDark && systemDark.matches);
	html.dataset.theme = theme;
	html.dataset.still = ui && ui.profile.animations === false ? 'true' : 'false';
	state.save('shown', theme);
	// The browser's bar in the page's background.
	const meta = document.querySelector('meta[name="theme-color"]');
	if (meta) meta.setAttribute('content', getComputedStyle(html).getPropertyValue('--bg').trim());
	if (systemDark && !listening) {
		listening = true;
		systemDark.addEventListener('change', () => applyTheme(last));
	}
}

/** This browser's choice, from the theme menu. @param {Theme} theme */
export function choose(theme) {
	state.save('theme', theme);
	applyTheme(last);
}
