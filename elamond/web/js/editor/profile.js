// A profile's part the editor writes: its tabs (which dashboards, in which order), its header and
// the app's home-screen widget, Quick Settings tiles and card. The asking client's own profile by
// default, any of the others on choosing.
import { h, clear } from '../dom.js';
import { S, fill } from '../strings.js';
import * as state from '../state.js';
import { addTab, removeTab, moveTab, setAndroid, isDirty } from './draft.js';
import { origins } from './describe.js';

/**
 * @typedef {{
 *   desc: () => import('./describe.js').Editor,
 *   draft: (id: string) => import('./draft.js').Draft<import('./draft.js').DraftProfile>,
 *   current: () => string | null, setCurrent: (id: string) => void,
 *   dashboard: string, title: () => string, changed: () => void, revert: (id: string) => void,
 *   names: () => Map<string, string>, mistakes: () => Map<string, string[]>,
 * }} ProfileHooks
 */

/** The profile's keys besides its tabs, by the protocol (the description doesn't describe a profile's keys). */
const ANDROID = /** @type {const} */ (['widget', 'tiles', 'card']);

/** @param {ProfileHooks} pc */
export function profileCard(pc) {
	const body = h('div', { class: 'ed-profile-body' });
	const summary = h('summary', null);
	const el = h('details', { class: 'card ed-profile' }, summary, body);
	el.open = !!state.load('editor.profileOpen', false);
	el.addEventListener('toggle', () => state.save('editor.profileOpen', el.open));

	/** Every dashboard a select offers: those there are, and this one while it is new. */
	const dashboards = () => {
		const desc = pc.desc();
		/** @type {{ id: string, title: string }[]} */
		const out = desc.dashboards.map((d) => ({ id: d.id, title: d.title || (d.def && d.def.title) || d.id }));
		if (!out.some((d) => d.id === pc.dashboard)) out.push({ id: pc.dashboard, title: pc.title() || pc.dashboard });
		return out;
	};
	/** @param {string} id */
	const titleOf = (id) => {
		const d = dashboards().find((x) => x.id === id);
		return d ? d.title : id;
	};

	/**
	 * A select of a dashboard or none (`""`); a value no dashboard has is kept, and said.
	 * @param {string} label @param {unknown} value @param {(v: string) => void} set @param {string[]} mistakes
	 */
	const pick = (label, value, set, mistakes) => {
		const id = `ed-p-${label.replace(/\W+/g, '-')}`;
		const sel = h('select', { id });
		sel.append(h('option', { value: '' }, S.editor.none));
		const all = dashboards();
		for (const d of all) sel.append(h('option', { value: d.id }, d.title === d.id ? d.id : `${d.title} (${d.id})`));
		if (typeof value === 'string' && value && !all.some((d) => d.id === value)) sel.append(h('option', { value }, fill(S.editor.notThere, value)));
		sel.value = typeof value === 'string' ? value : '';
		sel.addEventListener('change', () => set(sel.value));
		return h(
			'div',
			{ class: `ed-field${mistakes.length ? ' bad' : ''}` },
			h('label', { for: id }, label),
			sel,
			...mistakes.map((m) => h('p', { class: 'ed-mistake', role: 'alert' }, m)),
		);
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
		const d = pc.draft(pid);
		const p = d.value;
		const m = pc.mistakes();

		// Whose.
		const whose = h('select', { id: 'ed-p-whose' });
		for (const x of desc.profiles) whose.append(h('option', { value: x.id }, `${x.name || x.id}${x.id === desc.profile ? S.editor.yours : ''}${isDirty(pc.draft(x.id)) ? ' •' : ''}`));
		whose.value = pid;
		whose.addEventListener('change', () => pc.setCurrent(whose.value));
		const names = pc.names();
		const clients = (entry && entry.clients) || [];
		const from = entry
			? origins(desc, entry)
					.map((o) => (o.kind === 'defaults' ? S.editor.fromDefaults : o.kind === 'editor' ? S.editor.fromEditor : fill(o.kind === 'nix' ? S.editor.fromNix : S.editor.fromFile, o.line != null ? `${o.file}:${o.line}` : o.file)))
					.join(' · ')
			: '';
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

		// Tabs.
		const tabs = Array.isArray(p.dashboards) ? p.dashboards : [];
		const tabMistakes = m.get('dashboards') || [];
		const list = h('ol', { class: 'ed-tabs' });
		tabs.forEach((id, i) => {
			list.append(
				h(
					'li',
					{ class: id === pc.dashboard ? 'this' : null },
					h('span', { class: 'ed-tab-name' }, titleOf(id), ' ', h('small', { class: 'muted mono' }, id), id === pc.dashboard ? h('small', { class: 'ed-this' }, S.editor.thisOne) : null),
					h('button', { type: 'button', class: 'quiet ed-icon', disabled: i === 0, 'aria-label': fill(S.editor.tabUp, titleOf(id)), title: fill(S.editor.tabUp, titleOf(id)), onclick: () => { moveTab(p, i, i - 1); pc.changed(); render(); } }, S.editor.up),
					h('button', { type: 'button', class: 'quiet ed-icon', disabled: i === tabs.length - 1, 'aria-label': fill(S.editor.tabDown, titleOf(id)), title: fill(S.editor.tabDown, titleOf(id)), onclick: () => { moveTab(p, i, i + 1); pc.changed(); render(); } }, S.editor.down),
					h('button', { type: 'button', class: 'quiet ed-icon', 'aria-label': fill(S.editor.tabRemove, titleOf(id)), title: fill(S.editor.tabRemove, titleOf(id)), onclick: () => { removeTab(p, i); pc.changed(); render(); } }, S.editor.remove),
				),
			);
		});
		if (!tabs.length) list.append(h('li', { class: 'muted' }, S.editor.noTabs));
		const rest = dashboards().filter((x) => !tabs.includes(x.id));
		const add = h('select', { 'aria-label': S.editor.addTab });
		for (const x of rest) add.append(h('option', { value: x.id }, x.title === x.id ? x.id : `${x.title} (${x.id})`));
		add.value = rest.some((x) => x.id === pc.dashboard) ? pc.dashboard : rest.length ? rest[0].id : '';
		body.append(
			h(
				'div',
				{ class: `ed-field${tabMistakes.length ? ' bad' : ''}` },
				h('h3', null, S.editor.tabs),
				list,
				rest.length
					? h('div', { class: 'ed-tab-add' }, add, h('button', { type: 'button', onclick: () => { if (add.value) { addTab(p, add.value); pc.changed(); render(); } } }, S.editor.addTab))
					: null,
				...tabMistakes.map((x) => h('p', { class: 'ed-mistake', role: 'alert' }, x)),
			),
		);

		// Header and the app's.
		const android = p.android && typeof p.android === 'object' ? p.android : {};
		body.append(
			h(
				'div',
				{ class: 'ed-fields' },
				pick(S.editor.header, p.header, (v) => { p.header = v; pc.changed(); renderSummary(); }, m.get('header') || []),
				...ANDROID.map((k) => pick(S.editor.android[k], android[k], (v) => { setAndroid(p, k, v); pc.changed(); renderSummary(); }, m.get(`android.${k}`) || [])),
			),
			h('p', { class: 'ed-hint muted' }, S.editor.androidHint),
		);
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
