// Pairing, the computer's tabs and its dashboards, drawn from `ui` and the stream; every widget
// type in widgets/. The dashboard editor and the admin panel's preview draw with `dashboardView`
// too, so a draft looks as it will.
import { h } from './dom.js';
import { S } from './strings.js';

/** Every widget type this page draws, as `ui`, the stream and a preview ask for them: keep it to the types widgets/ has. */
export const DRAWS = 'value,gauge,meter,status,graph,list,bars,table,button,toggle,select,slider,text,group,media,timeline,log';

/**
 * The pairing screen. @param {HTMLElement} root @param {import('./types.js').Ctx} ctx
 * @param {string | null} code a code from the page's address (`#pair=123456`), to fill in
 */
export function mountPair(root, ctx, code) {
	root.append(h('p', null, 'Pairing: to be written. ', code || ''));
	return () => {};
}

/**
 * The computer: its header, its tabs and the dashboard on screen, live from the stream.
 * @param {HTMLElement} root @param {import('./types.js').Ctx} ctx @param {string | null} tab
 * @returns {() => void} what tears it down
 */
export function mountMachine(root, ctx, tab) {
	root.append(h('p', null, S.core.loading));
	return () => {};
}

/**
 * One dashboard of `ui`, drawn; `update` takes each `data` for it. Used by the machine view, the
 * admin panel's preview and the dashboard editor.
 * @param {import('./types.js').Ui} ui @param {string} dashboard @param {import('./types.js').ViewHooks} hooks
 * @returns {import('./types.js').DashboardView}
 */
export function dashboardView(ui, dashboard, hooks) {
	const el = h('section', { class: 'dashboard' }, h('p', null, dashboard));
	return { el, update() {}, destroy() {} };
}
