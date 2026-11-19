// A widget of a type this page doesn't draw: an honest notice, as the app shows one. The service
// describes only the types the page lists, so this is a service newer than its own page, or a draft.
import { h } from '../dom.js';
import { notice } from './common.js';
import { S, fill } from '../strings.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const el = h('div', { class: 'w w-unknown' }, notice(w.label, fill(S.panels.unknownWidget, w.type), 'warning'));
	return { el, update() {} };
}
