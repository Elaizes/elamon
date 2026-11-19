// The admin panel: for the roles the service opens its admin API to (`admin` in `features`). Each
// section has its address, `#/admin/<section>/…`, and is drawn afresh when the address changes.
import { h } from '../dom.js';
import { S, fill } from '../strings.js';
import { makeEnv } from './ui.js';
import { mountOverview } from './overview.js';
import { mountSettings } from './settings.js';
import { mountClients } from './clients.js';
import { mountPairing } from './pairing.js';
import { mountControl } from './control.js';
import { mountFiles } from './files.js';
import { mountActivity, mountEvents, mountSources } from './logs.js';

/** @typedef {(main: HTMLElement, env: import('./types.js').Env, rest: string[]) => (() => void)} Mount */

/** The sections, in the order the panel lists them. @type {[keyof typeof S.admin.nav, Mount][]} */
const SECTIONS = [
	['overview', mountOverview],
	['settings', mountSettings],
	['clients', mountClients],
	['pairing', mountPairing],
	['control', mountControl],
	['files', mountFiles],
	['activity', mountActivity],
	['events', mountEvents],
	['sources', mountSources],
];

/**
 * @param {HTMLElement} root @param {import('../types.js').Ctx} ctx @param {string[]} path what follows `#/admin/`
 * @returns {() => void} what tears it down
 */
export function mountAdmin(root, ctx, path) {
	const env = makeEnv(ctx);
	const name = path.length ? path[0] : 'overview';
	const section = SECTIONS.find(([id]) => id === name);
	const nav = h(
		'nav',
		{ class: 'adm-nav', 'aria-label': S.admin.nav.label },
		...SECTIONS.map(([id]) => h('a', { href: `#/admin/${id}`, 'aria-current': id === name ? 'page' : null }, S.admin.nav[id])),
	);
	const main = h('main', { class: 'adm-main' });
	root.append(
		h(
			'div',
			{ class: 'adm' },
			h('header', { class: 'topbar' }, h('a', { class: 'button quiet adm-back', href: '#/' }, fill(S.admin.marks.back, S.core.back)), h('span', { class: 'name' }, fill(S.admin.title, ctx.hello.name))),
			nav,
			main,
		),
	);
	document.title = fill(S.admin.docTitle, section ? S.admin.nav[section[0]] : S.admin.nav.label, ctx.hello.name);
	/** @type {() => void} */
	let destroy = () => {};
	if (section) destroy = section[1](main, env, path.slice(1));
	else main.append(h('p', { class: 'note' }, S.core.notFound));
	// The section's own link in view, on a narrow screen where the list scrolls sideways.
	const current = nav.querySelector('[aria-current="page"]');
	if (current) current.scrollIntoView({ block: 'nearest', inline: 'center' });
	return () => {
		env.destroy();
		destroy();
	};
}
