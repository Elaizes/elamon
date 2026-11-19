// A profile's part the editor writes, in the forms the service describes: the keys of
// `profile_keys` (its tabs, its header) and, under `android`, those of `android_keys`. The asking
// client's own profile by default, any of the others on choosing.
import { h, clear } from '../dom.js';
import { S, fill } from '../strings.js';
import * as state from '../state.js';
import { isDirty } from './draft.js';
import { origins, fromWords } from './describe.js';
import { keyFields } from './form.js';

/**
 * @typedef {{
 *   desc: () => import('./describe.js').Editor,
 *   draft: (id: string) => import('./draft.js').Draft<import('./draft.js').DraftProfile>,
 *   current: () => string | null, setCurrent: (id: string) => void,
 *   dashboard: string, title: () => string, changed: () => void, revert: (id: string) => void,
 *   names: () => Map<string, string>, mistakes: () => Map<string, string[]>,
 * }} ProfileHooks
 */

/** The key a profile holds the app's dashboards under (the protocol's, as in the editor's writes). */
const ANDROID = 'android';

/** @param {ProfileHooks} pc */
export function profileCard(pc) {
	const body = h('div', { class: 'ed-profile-body' });
	const summary = h('summary', null);
	const el = h('details', { class: 'card ed-profile' }, summary, body);
	el.open = !!state.load('editor.profileOpen', false);
	el.addEventListener('toggle', () => state.save('editor.profileOpen', el.open));

	/** The description, with the dashboard edited among those a tab may name while it is new. */
	const descHere = () => {
		const desc = pc.desc();
		if (desc.dashboards.some((d) => d.id === pc.dashboard)) return desc;
		return { ...desc, dashboards: [...desc.dashboards, { id: pc.dashboard, title: pc.title() || pc.dashboard, def: {} }] };
	};

	const render = () => {
		const desc = pc.desc();
		const pid = pc.current();
		const entry = desc.profiles.find((p) => p.id === pid) || null;
		clear(summary);
		clear(body);
		const dirty = pid ? isDirty(pc.draft(pid)) : false;
		summary.append(h('span', { class: 'ed-summary-title' }, fill(S.editor.profileTitle, entry ? entry.name || entry.id : pid || '')));
		if (dirty) summary.append(h('span', { class: 'ed-dirty' }, S.editor.unsaved));
		if (!desc.profiles.length || !pid) {
			body.append(h('p', { class: 'muted' }, S.editor.noProfiles));
			return;
		}
		const p = pc.draft(pid).value;
		const m = pc.mistakes();

		// Whose.
		const whose = h('select', { id: 'ed-p-whose' });
		for (const x of desc.profiles) whose.append(h('option', { value: x.id }, `${x.name || x.id}${x.id === desc.profile ? S.editor.yours : ''}${isDirty(pc.draft(x.id)) ? S.editor.dirtyMark : ''}`));
		whose.value = pid;
		whose.addEventListener('change', () => pc.setCurrent(whose.value));
		const names = pc.names();
		const clients = (entry && entry.clients) || [];
		const from = entry ? origins(desc, entry).map(fromWords).join(' · ') : '';
		body.append(
			h(
				'div',
				{ class: 'ed-field' },
				h('label', { for: 'ed-p-whose' }, S.editor.whose),
				whose,
				h('p', { class: 'ed-hint muted' }, clients.length ? fill(S.editor.usedBy, clients.map((c) => names.get(c) || c).join(', ')) : S.editor.usedByNone),
				from ? h('p', { class: 'ed-hint muted' }, fill(S.editor.from, from), entry && entry.editor ? ` ${S.editor.profileOverridden}` : '') : null,
			),
		);

		// Its keys, as the service describes them. A key left out of a write stays as the files say,
		// so none is held here: each can be taken out.
		/** @type {import('./form.js').FormCtx} */
		const fc = { desc: descHere(), changed: () => { pc.changed(); renderSummary(); }, mistakes: m };
		body.append(h('div', { class: 'ed-fields' }, ...keyFields(fc, desc.profile_keys, p)));
		if (desc.android_keys.length) {
			// The app's part: a table of its own, made in the draft once one of its keys is set.
			const android = p[ANDROID] && typeof p[ANDROID] === 'object' && !Array.isArray(p[ANDROID]) ? p[ANDROID] : {};
			/** @type {Map<string, string[]>} */
			const androidMistakes = new Map();
			for (const [k, list] of m) if (k.startsWith(`${ANDROID}.`)) androidMistakes.set(k.slice(ANDROID.length + 1), list);
			/** @type {import('./form.js').FormCtx} */
			const afc = {
				desc: fc.desc,
				changed: () => {
					p[ANDROID] = android;
					fc.changed();
				},
				mistakes: androidMistakes,
			};
			body.append(
				h('fieldset', { class: 'ed-record ed-android' }, h('legend', null, S.editor.androidKeys), h('div', { class: 'ed-fields' }, ...keyFields(afc, desc.android_keys, android))),
				h('p', { class: 'ed-hint muted' }, S.editor.androidHint),
			);
		}
		if (entry && entry.editor) {
			body.append(h('div', { class: 'ed-row-buttons' }, h('button', { type: 'button', onclick: () => pc.revert(pid) }, S.editor.revertProfile)));
		}
	};
	const renderSummary = () => {
		const pid = pc.current();
		const dot = summary.querySelector('.ed-dirty');
		const dirty = pid ? isDirty(pc.draft(pid)) : false;
		if (dirty && !dot) summary.append(h('span', { class: 'ed-dirty' }, S.editor.unsaved));
		else if (!dirty && dot) dot.remove();
	};
	render();
	return { el, render, renderSummary };
}
