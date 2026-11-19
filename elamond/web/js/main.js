// The page: asks the service who it is, whether this browser is paired, and draws what the address
// asks for: pairing, the computer, the admin panel or the dashboard editor.
import { api, ApiError } from './api.js';
import { h, clear } from './dom.js';
import { S, fill } from './strings.js';
import { applyTheme } from './theme.js';
import { mountPair, mountMachine, DRAWS } from './panels.js';
import { mountAdmin } from './admin/index.js';
import { mountEditor } from './editor/index.js';

/** The API this page speaks. */
const API = 2;

const root = /** @type {HTMLElement} */ (document.getElementById('app'));
/** @type {import('./types.js').Ui | null} */
let ui = null;
/** @type {import('./types.js').Hello | null} */
let hello = null;
/** @type {() => void} */
let teardown = () => {};

/** @type {import('./types.js').Ctx} */
const ctx = {
	get hello() {
		return /** @type {import('./types.js').Hello} */ (hello);
	},
	ui: () => ui,
	reloadUi,
	go: (hash) => {
		if (location.hash === hash) route();
		else location.hash = hash;
	},
	toast,
	has: (feature) => !!ui && ui.service.features.includes(feature),
};

/** The `ui` of this browser, or null when it isn't paired (401). */
async function reloadUi() {
	try {
		ui = await api(`/api/v1/ui?widgets=${DRAWS}`);
	} catch (e) {
		if (e instanceof ApiError && e.status === 401) ui = null;
		else throw e;
	}
	applyTheme(ui);
	return ui;
}

/** A short note at the bottom of the page. @param {string} text @param {'normal' | 'warning' | 'critical'} [level] */
function toast(text, level = 'normal') {
	let box = document.getElementById('toasts');
	if (!box) {
		box = h('div', { id: 'toasts', role: 'status', 'aria-live': 'polite' });
		document.body.append(box);
	}
	const t = h('div', { class: `toast ${level}` }, text);
	box.append(t);
	setTimeout(() => t.remove(), level === 'normal' ? 4000 : 8000);
}

/** Draws what the address asks for. */
async function route() {
	teardown();
	teardown = () => {};
	clear(root);
	const hash = location.hash;
	// A code in the address is taken and dropped from it, so it stays out of the history.
	const code = /^#pair=(\d{6})$/.exec(hash);
	if (code) history.replaceState(null, '', location.pathname);
	try {
		if (!hello) hello = await api('/api/v1/hello');
		if (hello && hello.api !== API) {
			root.append(h('p', { class: 'note critical' }, fill(S.core.tooOld, API, hello.api)));
			return;
		}
		await reloadUi();
	} catch (e) {
		root.append(h('p', { class: 'note critical' }, S.core.unreachable), h('button', { onclick: () => route() }, S.core.retry));
		return;
	}
	document.title = hello ? `${hello.name} · ${S.core.title}` : S.core.title;
	if (!ui) {
		teardown = mountPair(root, ctx, code ? code[1] : null);
		return;
	}
	const parts = (code ? '' : hash).replace(/^#\/?/, '').split('/').filter(Boolean).map(decodeURIComponent);
	if (parts[0] === 'admin' && ctx.has('admin')) teardown = mountAdmin(root, ctx, parts.slice(1));
	else if (parts[0] === 'edit' && ctx.has('editor')) teardown = mountEditor(root, ctx, parts[1] ?? null);
	else teardown = mountMachine(root, ctx, parts[0] === 'tab' ? (parts[1] ?? null) : null);
}

window.addEventListener('hashchange', () => route());
route();
