// The admin panel: for the roles the service opens its admin API to (`admin` in `features`). Each
// section has its address, `#/admin/<section>/…`, and is drawn afresh when the address changes; the
// sections are listed by the page's frame (shell.js): in its side bar, or a row over the section.
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
import { mountFrame } from '../shell.js';

/** @typedef {(main: HTMLElement, env: import('./types.js').Env, rest: string[]) => (() => void)} Mount */

/** Each section's page; their order and icons are the frame's (shell.js, ADMIN_SECTIONS). @type {Record<string, Mount>} */
const SECTIONS = {
	overview: mountOverview,
	settings: mountSettings,
	clients: mountClients,
	pairing: mountPairing,
	control: mountControl,
	files: mountFiles,
	activity: mountActivity,
	events: mountEvents,
	sources: mountSources,
};

/**
 * @param {HTMLElement} root @param {import('../types.js').Ctx} ctx @param {string[]} path what follows `#/admin/`
 * @returns {() => void} what tears it down
 */
export function mountAdmin(root, ctx, path) {
	const env = makeEnv(ctx);
	const name = path.length ? path[0] : 'overview';
	const known = Object.hasOwn(SECTIONS, name);
	const frame = mountFrame(root, ctx, { at: 'admin', section: name });
	frame.title(S.core.admin);
	const main = h('main', { class: 'adm-main' });
	frame.body.append(h('div', { class: 'adm' }, main));
	document.title = fill(S.admin.docTitle, known ? S.admin.nav[/** @type {keyof typeof S.admin.nav} */ (name)] : S.admin.nav.label, ctx.hello.name);
	/** @type {() => void} */
	let destroy = () => {};
	if (known) destroy = SECTIONS[name](main, env, path.slice(1));
	else main.append(h('p', { class: 'note' }, S.core.notFound));
	return () => {
		frame.destroy();
		env.destroy();
		destroy();
	};
}
