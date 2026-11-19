// The dashboard editor: forms the service describes (`GET /api/v1/admin/editor`), a live preview
// drawn with panels.js, and writes the service makes (`editor` in `features`). The page keeps no
// list of widget types, keys or values of its own, and writes no commands: a button, switch or
// row only picks an action there is.
import { api, ApiError } from '../api.js';
import { h, clear } from '../dom.js';
import { S, fill } from '../strings.js';
import * as state from '../state.js';
import {
	draft, isDirty, rebase, clone, stable, widgetAt, addWidget, removeWidget, moveWidget, nudge, setSpan, pathKey, samePath,
	sortMistakes, isId, addTab, changes, WIDGETS,
} from './draft.js';
import { normalise, isGroup, spanRange, nameOf, exampleOf, origins, fromWords, keysFromBelow } from './describe.js';
import { keyFields, otherField, markField } from './form.js';
import { Canvas } from './canvas.js';
import { Previewer } from './preview.js';
import { profileCard } from './profile.js';
import { widgetForm } from './side.js';
import { addDialog } from './add.js';
import { ask } from './ask.js';
import { errorSentence } from '../admin/errors.js';

/**
 * A dashboard made on `#/edit` and not saved yet, which the editor it goes to takes up.
 * @type {{ id: string, def: Record<string, any>, tab: string | null } | null}
 */
let made = null;

/**
 * The open editor's question before the page goes to another address. Listened for here, when this
 * module is first run: main.js imports it, so it runs before main.js adds its router, and this
 * listener comes first and can stop it (the window, the target, calls its listeners in the order
 * they were added, capture or not).
 * @type {((e: HashChangeEvent) => void) | null}
 */
let guard = null;
window.addEventListener('hashchange', (e) => {
	if (guard) guard(e);
});

/**
 * @param {HTMLElement} root @param {import('../types.js').Ctx} ctx @param {string | null} dashboard the one to edit, none for a new one
 * @returns {() => void} what tears it down
 */
export function mountEditor(root, ctx, dashboard) {
	if (dashboard == null) return mountNew(root, ctx);
	const editor = new Editor(root, ctx, dashboard);
	editor.start();
	return () => editor.destroy();
}

/** The description, read. @returns {Promise<import('./describe.js').Editor>} */
async function describe() {
	return normalise(await api('/api/v1/admin/editor'));
}

/** What an answer that isn't 2xx means, in words. @param {unknown} e */
function words(e) {
	if (!(e instanceof ApiError)) return S.core.unreachable;
	const E = /** @type {Record<string, string>} */ (S.editor.errors);
	if (e.code === 'writes_off') return errorSentence(e);
	const known = E[e.code];
	if (known) return e.detail && e.code !== 'not_allowed' ? `${known} (${e.detail})` : known;
	return fill(S.editor.errorCode, e.code, e.detail || '');
}

// ---------------------------------------------------------------- a new dashboard

/** `#/edit`: an id and a title, or a copy of a dashboard there is. @param {HTMLElement} root @param {import('../types.js').Ctx} ctx */
function mountNew(root, ctx) {
	let gone = false;
	const box = h('div', { class: 'ed ed-new' }, h('p', null, S.core.loading));
	root.append(box);
	describe()
		.then((desc) => {
			if (gone) return;
			clear(box);
			const id = h('input', { type: 'text', id: 'ed-new-id', class: 'mono', required: true, maxlength: 32, autocapitalize: 'off', spellcheck: 'false', 'aria-describedby': 'ed-new-id-hint' });
			const idNote = h('p', { class: 'ed-hint muted', id: 'ed-new-id-hint' }, S.editor.newIdHint);
			const title = h('input', { type: 'text', id: 'ed-new-title', required: true });
			const from = h('select', { id: 'ed-new-from' });
			from.append(h('option', { value: '' }, S.editor.newEmpty));
			for (const d of desc.dashboards) from.append(h('option', { value: d.id }, fill(S.editor.copyOf, d.title || d.id, d.id)));
			const tab = h('input', { type: 'checkbox', id: 'ed-new-tab' });
			tab.checked = !!desc.profile;
			const own = desc.profiles.find((p) => p.id === desc.profile);
			const why = h('p', { class: 'ed-local', role: 'alert', hidden: true });
			let titleTouched = false;
			title.addEventListener('input', () => (titleTouched = true));
			from.addEventListener('change', () => {
				const d = desc.dashboards.find((x) => x.id === from.value);
				if (d && !titleTouched) title.value = fill(S.editor.copyTitle, d.title || d.id);
			});
			const form = h(
				'form',
				{
					class: 'card ed-new-form',
					onsubmit: (/** @type {Event} */ e) => {
						e.preventDefault();
						const v = id.value.trim();
						let bad = '';
						if (!isId(v)) bad = S.editor.badId;
						else if (desc.dashboards.some((d) => d.id === v)) bad = fill(S.editor.idTaken, v);
						else if (!title.value.trim()) bad = S.editor.needTitle;
						why.textContent = bad;
						why.hidden = !bad;
						id.setAttribute('aria-invalid', bad && bad !== S.editor.needTitle ? 'true' : 'false');
						if (bad) return;
						const src = desc.dashboards.find((d) => d.id === from.value);
						const def = src ? clone(src.def || {}) : { widgets: [] };
						def.title = title.value.trim();
						made = { id: v, def, tab: tab.checked && desc.profile ? desc.profile : null };
						ctx.go(`#/edit/${encodeURIComponent(v)}`);
					},
				},
				h('h1', null, S.editor.newTitle),
				h('div', { class: 'ed-field' }, h('label', { for: 'ed-new-id' }, S.editor.newId), id, idNote),
				h('div', { class: 'ed-field' }, h('label', { for: 'ed-new-title' }, S.editor.newName), title),
				h('div', { class: 'ed-field' }, h('label', { for: 'ed-new-from' }, S.editor.startFrom), from),
				desc.profile ? h('div', { class: 'ed-field' }, h('label', { class: 'ed-check', for: 'ed-new-tab' }, tab, h('span', null, fill(S.editor.addToTabs, own ? own.name || own.id : desc.profile)))) : null,
				why,
				h('div', { class: 'ed-row-buttons' }, h('button', { type: 'button', onclick: () => ctx.go('#/') }, S.core.cancel), h('button', { type: 'submit', class: 'primary' }, S.editor.create)),
			);
			id.addEventListener('input', () => {
				const ok = id.value === '' || isId(id.value);
				id.setAttribute('aria-invalid', ok ? 'false' : 'true');
				idNote.textContent = ok ? S.editor.newIdHint : S.editor.badId;
				idNote.classList.toggle('ed-local', !ok);
			});
			box.append(form);
			id.focus();
		})
		.catch((e) => {
			if (gone) return;
			clear(box);
			box.append(h('p', { class: 'note critical' }, words(e)), h('button', { type: 'button', onclick: () => ctx.go('#/edit') }, S.core.retry));
		});
	return () => {
		gone = true;
	};
}

// ---------------------------------------------------------------- the editor

class Editor {
	/** @param {HTMLElement} root @param {import('../types.js').Ctx} ctx @param {string} id */
	constructor(root, ctx, id) {
		this.root = root;
		this.ctx = ctx;
		this.id = id;
		/** @type {import('./describe.js').Editor} */
		this.desc = normalise(null);
		/** @type {import('./describe.js').EditorDashboard | null} */
		this.entry = null;
		/** @type {import('./draft.js').Draft<import('./draft.js').DraftDashboard>} */
		this.dash = draft({});
		this.isNew = false;
		/** @type {Map<string, import('./draft.js').Draft<import('./draft.js').DraftProfile>>} */
		this.profiles = new Map();
		/** @type {string | null} */
		this.profileId = null;
		/** @type {import('./draft.js').WidgetPath | null} */
		this.selected = null;
		this.sheetOpen = false;
		/** @type {ReturnType<typeof sortMistakes>} */
		this.mistakes = sortMistakes([], null, null);
		this.mistakesSig = '[]';
		/** @type {Map<string, string>} */
		this.clientNames = new Map();
		this.saving = false;
		this.leaving = false;
		this.gone = false;
		/** @type {{ widget: Record<string, any>, path: import('./draft.js').WidgetPath } | null} */
		this.removed = null;
		/** @type {(() => void) | null} */
		this.closeAdd = null;
		this.here = location.href;

		this.el = h('div', { class: 'ed' }, h('p', null, S.core.loading));
		root.append(this.el);
		this.canvas = new Canvas({
			desc: () => this.desc,
			dash: () => this.dash.value,
			hooks: {
				select: (path, open, into) => this.select(path, open, into),
				move: (from, parent, index) => this.move(from, parent, index),
				nudge: (path, delta) => this.nudge(path, delta),
				span: (path, n) => this.span(path, n),
				remove: (path) => this.remove(path),
			},
		});
		this.previewer = new Previewer({
			body: () => this.previewBody(),
			interval: Math.max(1000, (ctx.ui() && ctx.ui()?.service.interval_ms) || 1000),
			done: (a) => this.previewed(a),
			failed: (e) => this.previewFailed(e),
		});

		this.onHash = (/** @type {HashChangeEvent} */ e) => {
			if (this.leaving || !this.dirty()) return;
			// Before the page's router: stay, and ask.
			e.stopImmediatePropagation();
			const to = location.href;
			history.pushState(null, '', this.here);
			ask(S.editor.leaveTitle, S.editor.leaveText, S.editor.leaveGo, true).then((ok) => {
				if (!ok || this.gone) return;
				this.leaving = true;
				location.href = to;
			});
		};
		this.onUnload = (/** @type {BeforeUnloadEvent} */ e) => {
			if (this.leaving || !this.dirty()) return;
			e.preventDefault();
			e.returnValue = '';
		};
		this.onKey = (/** @type {KeyboardEvent} */ e) => {
			if (e.key === 'Escape' && this.selected && !document.querySelector('dialog[open]')) {
				const was = this.selected;
				this.select(null);
				this.canvas.focus(was);
			}
		};
		guard = this.onHash;
		window.addEventListener('beforeunload', this.onUnload);
		document.addEventListener('keydown', this.onKey);
	}

	async start() {
		try {
			this.desc = await describe();
		} catch (e) {
			if (this.gone) return;
			clear(this.el);
			this.el.append(h('p', { class: 'note critical' }, words(e)), h('button', { type: 'button', onclick: () => this.ctx.go(location.hash) }, S.core.retry));
			return;
		}
		if (this.gone) return;
		this.entry = this.desc.dashboards.find((d) => d.id === this.id) || null;
		if (!this.entry && !(made && made.id === this.id)) {
			clear(this.el);
			this.el.append(
				h('p', { class: 'note warning' }, fill(S.editor.noSuchDashboard, this.id)),
				h('div', { class: 'ed-row-buttons' }, h('a', { class: 'button', href: '#/edit' }, S.editor.makeOne), h('a', { class: 'button', href: '#/' }, S.core.back)),
			);
			return;
		}
		if (this.entry) this.dash = draft(this.entry.def || {});
		else if (made) {
			this.isNew = true;
			this.dash = draft({});
			this.dash.value = clone(made.def);
			if (made.tab) addTab(this.profile(made.tab).value, this.id);
		}
		this.profileId = (made && made.id === this.id && made.tab) || this.desc.profile || (this.desc.profiles[0] ? this.desc.profiles[0].id : null);
		this.build();
		this.previewer.soon(0);
		this.loadNames();
	}

	destroy() {
		this.gone = true;
		this.previewer.stop();
		this.canvas.destroyViews();
		if (this.closeAdd) this.closeAdd();
		if (guard === this.onHash) guard = null;
		window.removeEventListener('beforeunload', this.onUnload);
		document.removeEventListener('keydown', this.onKey);
		document.body.classList.remove('ed-sheet-open');
	}

	/** The names of the clients, to say who uses a profile (the admin API's list; ids without it). */
	async loadNames() {
		try {
			const r = await api('/api/v1/admin/clients');
			for (const c of (r && r.clients) || []) if (c && c.id) this.clientNames.set(String(c.id), String(c.name || c.id));
			if (!this.gone && this.profileCard) this.profileCard.render();
		} catch {}
	}

	/** A profile's draft, made from its `def` the first time. @param {string} id */
	profile(id) {
		let d = this.profiles.get(id);
		if (!d) {
			const p = this.desc.profiles.find((x) => x.id === id);
			d = draft(p && p.def && typeof p.def === 'object' ? p.def : {});
			this.profiles.set(id, d);
		}
		return d;
	}

	dirty() {
		return this.isNew || isDirty(this.dash) || [...this.profiles.values()].some(isDirty);
	}

	// ------------------------------------------------------------ drawing

	build() {
		clear(this.el);
		this.titleEl = h('h1', { class: 'name' });
		this.dirtyEl = h('span', { class: 'ed-dirty', hidden: true }, S.editor.unsaved);
		this.saveBtn = h('button', { type: 'button', class: 'primary', onclick: () => this.save() }, S.editor.save);
		this.statusEl = h('div', { class: 'ed-status', role: 'status', 'aria-live': 'polite' });
		this.originEl = h('div', { class: 'ed-origin' });
		// The dashboard's keys fold away (on a phone they would push the widgets off the screen).
		this.keysEl = h('details', { class: 'card ed-keys' });
		this.keysEl.open = !!state.load('editor.keysOpen', !window.matchMedia('(max-width: 600px)').matches);
		this.keysEl.addEventListener('toggle', () => this.keysEl && state.save('editor.keysOpen', this.keysEl.open));
		this.otherEl = h('div', { class: 'ed-other-mistakes' });
		this.previewNote = h('p', { class: 'ed-preview-note muted' }, S.editor.previewWaiting);
		this.side = h('aside', { class: 'ed-side', 'aria-labelledby': 'ed-side-title' });
		this.profileCard = profileCard({
			desc: () => this.desc,
			draft: (id) => this.profile(id),
			current: () => this.profileId,
			setCurrent: (id) => {
				this.profileId = id;
				this.applyMistakes(true);
				this.profileCard && this.profileCard.render();
			},
			dashboard: this.id,
			title: () => String(this.dash.value.title || ''),
			changed: () => this.changed(false),
			revert: (id) => this.revertProfile(id),
			names: () => this.clientNames,
			mistakes: () => this.mistakes.prof,
		});
		const top = h(
			'header',
			{ class: 'topbar ed-top' },
			h('button', { type: 'button', onclick: () => this.leave(this.backHash()) }, S.core.back),
			this.titleEl,
			this.dirtyEl,
			this.saveBtn,
		);
		const tools = h(
			'div',
			{ class: 'ed-tools' },
			h('button', { type: 'button', class: 'primary', onclick: () => this.openAdd(null) }, S.editor.addWidget),
			this.previewNote,
		);
		this.el.append(
			top,
			this.statusEl,
			this.originEl,
			this.otherEl,
			this.keysEl,
			this.profileCard.el,
			tools,
			h('div', { class: 'ed-main' }, this.canvas.el, this.side),
		);
		this.renderTitle();
		this.renderOrigin();
		this.renderKeys();
		this.canvas.render();
		this.renderSide();
		this.renderDirty();
	}

	backHash() {
		const ui = this.ctx.ui();
		return ui && ui.tabs.includes(this.id) ? `#/tab/${encodeURIComponent(this.id)}` : '#/';
	}

	renderTitle() {
		if (!this.titleEl) return;
		const t = String(this.dash.value.title || this.id);
		clear(this.titleEl);
		this.titleEl.append(fill(this.isNew ? S.editor.titleNew : S.editor.titleEdit, t), ' ', h('small', { class: 'muted mono' }, this.id));
		document.title = `${fill(S.editor.titleEdit, t)} · ${this.ctx.hello ? this.ctx.hello.name : S.core.title}`;
	}

	renderDirty() {
		if (!this.dirtyEl || !this.saveBtn) return;
		const dirty = this.dirty();
		this.dirtyEl.hidden = !dirty;
		this.saveBtn.disabled = !dirty || this.saving || !this.desc.writable;
		this.saveBtn.title = this.desc.writable ? '' : this.desc.why || S.editor.notWritable;
		if (this.profileCard) this.profileCard.renderSummary();
	}

	/** Where the dashboard comes from, whether the editor's version overrides it; going back to the files, deleting. */
	renderOrigin() {
		const el = /** @type {HTMLElement} */ (this.originEl);
		clear(el);
		const e = this.entry;
		/** @type {(string | Node)[]} */
		const lines = [];
		if (!this.desc.writable) lines.push(h('p', { class: 'note warning' }, fill(S.editor.notWritableWhy, this.desc.file, this.desc.why || S.editor.notWritable)));
		if (this.isNew || !e) {
			lines.push(h('p', { class: 'muted' }, S.editor.isNew));
		} else {
			const from = origins(this.desc, e).map(fromWords).join(' · ');
			const p = h('p', { class: 'muted' }, fill(S.editor.from, from));
			if (e.own) p.append(' ', S.editor.ownDashboard);
			else if (e.editor) p.append(' ', S.editor.overrides);
			lines.push(p);
			const buttons = h('div', { class: 'ed-row-buttons' });
			if (e.editor && !e.own) buttons.append(h('button', { type: 'button', onclick: () => this.revert() }, S.editor.revert));
			if (e.own) buttons.append(h('button', { type: 'button', class: 'danger', onclick: () => this.del() }, S.editor.deleteDashboard));
			if (buttons.children.length) lines.push(buttons);
		}
		for (const l of lines) el.append(l);
	}

	/** The dashboard's own keys (title, icon, role…), from `dashboard.keys`; its widgets are the canvas. */
	renderKeys() {
		const el = /** @type {HTMLElement} */ (this.keysEl);
		clear(el);
		const keys = this.desc.dashboard.keys.filter((k) => k.kind !== 'widgets');
		const known = new Set(this.desc.dashboard.keys.map((k) => k.key));
		const fc = { desc: this.desc, changed: () => { this.renderTitle(); this.changed(false); }, below: keysFromBelow(this.isNew ? null : this.entry), mistakes: this.mistakes.dash };
		const v = this.dash.value;
		el.append(
			h('summary', null, h('h2', { id: 'ed-keys-title' }, S.editor.dashboardKeys)),
			h(
				'div',
				{ class: 'ed-fields' },
				...keyFields(fc, keys, v),
				...Object.keys(v)
					.filter((k) => !known.has(k) && k !== WIDGETS)
					.map((k) => otherField(fc, k, v)),
			),
		);
	}

	/** The side panel: the picked widget's form, or how to pick one. */
	renderSide() {
		const side = /** @type {HTMLElement} */ (this.side);
		clear(side);
		this.form = null;
		const w = this.selected ? widgetAt(this.dash.value, this.selected) : null;
		document.body.classList.toggle('ed-sheet-open', !!w && this.sheetOpen);
		side.classList.toggle('open', !!w && this.sheetOpen);
		if (!w || !this.selected) {
			this.selected = null;
			side.append(
				h('h2', { id: 'ed-side-title' }, S.editor.keys),
				h('p', { class: 'muted' }, S.editor.pickOne),
				h('ul', { class: 'ed-help muted' }, ...S.editor.help.map((t) => h('li', null, t))),
			);
			return;
		}
		const path = this.selected;
		const { min, max } = spanRange(this.desc);
		const info = this.canvas.infoAt(path);
		this.form = widgetForm({
			desc: this.desc,
			dash: this.dash.value,
			path,
			mistakes: this.mistakes.widgets.get(pathKey(path)) || new Map(),
			changed: (shape) => {
				this.changed(shape);
				if (shape) this.renderSide();
				else this.canvas.relabel(path);
			},
			close: () => {
				this.select(null);
				this.canvas.focus(path);
			},
			span: (n) => this.span(path, Math.max(min, Math.min(max, n))),
			nudge: (d) => this.nudge(path, d),
			moveInto: (parent) => {
				const list = this.dash.value[WIDGETS];
				const target = parent.length ? widgetAt(this.dash.value, parent) : null;
				const len = target ? (Array.isArray(target[WIDGETS]) ? target[WIDGETS].length : 0) : Array.isArray(list) ? list.length : 0;
				this.move(path, parent, len);
			},
			remove: () => this.remove(path),
			addInto: isGroup(this.desc, w) ? () => this.openAdd(path) : null,
			as: info.as,
			width: info.span,
		});
		side.append(this.form.el);
	}

	// ------------------------------------------------------------ changes

	/** After any change: what changed is marked, the preview asked again. @param {boolean} shape whether widgets came, went, moved or changed type or width */
	changed(shape) {
		if (shape) {
			this.canvas.markStale();
			this.canvas.render();
		}
		this.renderDirty();
		this.previewer.soon(shape ? 150 : 400);
	}

	/**
	 * Picks a widget. @param {import('./draft.js').WidgetPath | null} path @param {boolean} [open] whether its form opens (a
	 * sheet on a narrow screen) @param {boolean} [into] whether the focus goes into the form (from the keyboard)
	 */
	select(path, open = false, into = false) {
		const same = !!path && !!this.selected && samePath(path, this.selected);
		this.selected = path;
		this.sheetOpen = !!path && (open || (same && this.sheetOpen));
		this.canvas.select(path);
		if (!same || open) this.renderSide();
		else this.side && this.side.classList.toggle('open', this.sheetOpen);
		if (path && open && this.side && (into || window.matchMedia('(max-width: 900px)').matches)) {
			const first = this.side.querySelector(into ? '.ed-section input, .ed-section select, .ed-section textarea' : 'button');
			if (first instanceof HTMLElement) first.focus({ preventScroll: !into });
		}
	}

	/** @param {import('./draft.js').WidgetPath} from @param {import('./draft.js').WidgetPath} parent @param {number} index */
	move(from, parent, index) {
		const to = moveWidget(this.dash.value, from, parent, index);
		if (!to) return;
		this.moved(to);
	}

	/** @param {import('./draft.js').WidgetPath} path @param {-1 | 1} delta */
	nudge(path, delta) {
		const to = nudge(this.dash.value, path, delta);
		if (samePath(to, path)) return;
		this.moved(to);
	}

	/** A widget is somewhere else now: it stays picked, and focused. @param {import('./draft.js').WidgetPath} to */
	moved(to) {
		this.clearMistakes();
		const picked = !!this.selected;
		this.selected = picked ? to : null;
		this.changed(true);
		if (picked) {
			this.canvas.select(to);
			this.renderSide();
		}
		this.canvas.focus(to);
	}

	/** @param {import('./draft.js').WidgetPath} path @param {number} n */
	span(path, n) {
		const w = widgetAt(this.dash.value, path);
		if (!w) return;
		const { min, max } = spanRange(this.desc);
		if (Math.max(min, Math.min(max, n)) === this.canvas.infoAt(path).span && w.span !== undefined) return;
		setSpan(w, n, min, max);
		this.changed(true);
		if (this.selected && samePath(this.selected, path)) this.renderSide();
		this.canvas.focus(path);
	}

	/** Takes a widget out, with a way to put it back. @param {import('./draft.js').WidgetPath} path */
	remove(path) {
		const w = removeWidget(this.dash.value, path);
		if (!w) return;
		this.clearMistakes();
		this.removed = { widget: w, path };
		if (this.selected && (samePath(this.selected, path) || pathKey(this.selected).startsWith(`${pathKey(path)}/`))) this.selected = null;
		else if (this.selected) this.selected = null;
		this.canvas.select(null);
		this.changed(true);
		this.renderSide();
		this.say(fill(S.editor.removed, nameOf(this.desc, w)), 'normal', h('button', { type: 'button', onclick: () => this.undoRemove() }, S.editor.putBack));
		// Focus goes to the widget that took its place, or the one before.
		const list = path.length > 1 ? (widgetAt(this.dash.value, path.slice(0, -1)) || {})[WIDGETS] : this.dash.value[WIDGETS];
		const n = Array.isArray(list) ? list.length : 0;
		if (n) this.canvas.focus([...path.slice(0, -1), Math.min(path[path.length - 1], n - 1)]);
	}

	undoRemove() {
		const r = this.removed;
		if (!r) return;
		this.removed = null;
		const at = addWidget(this.dash.value, r.path.slice(0, -1), r.path[r.path.length - 1], r.widget);
		this.say('', 'normal');
		this.changed(true);
		this.select(at);
		this.canvas.focus(at);
	}

	/** "Add widget": into the dashboard (after the picked widget), or into a group. @param {import('./draft.js').WidgetPath | null} group */
	openAdd(group) {
		const g = group ? widgetAt(this.dash.value, group) : null;
		this.closeAdd = addDialog({
			desc: this.desc,
			ui: this.ctx.ui(),
			editor: () => /** @type {Record<string, unknown>} */ (this.previewBody().editor),
			into: g ? nameOf(this.desc, g) : null,
			pick: (type) => {
				this.closeAdd = null;
				const w = exampleOf(this.desc, type);
				/** @type {import('./draft.js').WidgetPath} */
				let parent = [];
				let index = Array.isArray(this.dash.value[WIDGETS]) ? this.dash.value[WIDGETS].length : 0;
				if (group) {
					parent = group;
					index = 1e9;
				} else if (this.selected) {
					parent = this.selected.slice(0, -1);
					index = this.selected[this.selected.length - 1] + 1;
				}
				const at = addWidget(this.dash.value, parent, index, w);
				this.clearMistakes();
				this.changed(true);
				this.select(at, true);
				this.canvas.focus(at);
			},
		});
	}

	// ------------------------------------------------------------ the preview

	/** The preview's body: this dashboard's draft, and the profiles changed. */
	previewBody() {
		/** @type {Record<string, unknown>} */
		const editor = { dashboards: { [this.id]: this.dash.value } };
		/** @type {Record<string, unknown>} */
		const profiles = {};
		for (const [id, d] of this.profiles) if (isDirty(d)) profiles[id] = changes(d.start, d.value);
		if (Object.keys(profiles).length) editor.profiles = profiles;
		return { editor, dashboard: this.id };
	}

	/** @param {any} a the preview's answer */
	previewed(a) {
		if (this.gone || !a || !a.ui) return;
		const own = this.ctx.ui();
		// The page's own `ui` fills in what a preview leaves out (the service, the profile, control).
		/** @type {import('../types.js').Ui} */
		const ui = { ...(own || {}), ...a.ui, service: a.ui.service || (own && own.service), profile: a.ui.profile || (own && own.profile), control: a.ui.control || (own && own.control), actions: a.ui.actions || (own && own.actions) || [], dashboards: Array.isArray(a.ui.dashboards) ? a.ui.dashboards : [] };
		const d = ui.dashboards.find((x) => x.id === this.id) || ui.dashboards.find((x) => x.id.startsWith(`${this.id}.`)) || null;
		this.setMistakes([], 'preview');
		this.note(d ? S.editor.previewLive : S.editor.previewNone, !d);
		this.canvas.setDrawn({ ui, dashboard: d, data: a.data || null });
	}

	/** @param {unknown} e */
	previewFailed(e) {
		if (this.gone) return;
		if (e instanceof ApiError && e.code === 'bad_settings') {
			this.setMistakes((e.body && e.body.mistakes) || [], 'preview');
			this.note(S.editor.previewMistakes, true);
		} else if (e instanceof ApiError && e.code === 'no_such_dashboard') {
			this.note(S.editor.previewNone, true);
		} else this.note(fill(S.editor.previewFailed, words(e)), true);
		this.canvas.el.classList.remove('stale');
	}

	/** @param {string} text @param {boolean} [warn] */
	note(text, warn = false) {
		if (!this.previewNote) return;
		this.previewNote.textContent = text;
		this.previewNote.classList.toggle('warn', warn);
	}

	// ------------------------------------------------------------ mistakes

	/**
	 * The service's mistakes, put on the widgets and keys they name. Those of the preview go as the
	 * draft is put right; those of a save stay until the next answer.
	 * @param {any[]} list @param {'preview' | 'save'} from
	 */
	setMistakes(list, from) {
		const sig = JSON.stringify(list);
		if (sig === this.mistakesSig && from === 'preview') return;
		this.mistakesSig = sig;
		this.mistakesList = list;
		this.applyMistakes(false);
	}

	/** After a widget moved or went, the mistakes' paths name others: they wait for the next answer. */
	clearMistakes() {
		if (this.mistakesSig === '[]') return;
		this.mistakesSig = '[]';
		this.mistakesList = [];
		this.applyMistakes(false);
	}

	/** @param {boolean} quiet whether only the profile changed */
	applyMistakes(quiet) {
		this.mistakes = sortMistakes(this.mistakesList || [], this.id, this.profileId);
		if (quiet) return;
		this.canvas.mistakes = this.mistakes.widgets;
		this.canvas.render();
		if (this.form && this.selected) this.form.mark(this.mistakes.widgets.get(pathKey(this.selected)) || new Map());
		if (this.keysEl) {
			for (const f of this.keysEl.querySelectorAll('.ed-fields > .ed-field')) {
				const k = /** @type {HTMLElement} */ (f).dataset.key || '';
				markField(/** @type {HTMLElement} */ (f), this.mistakes.dash.get(k) || []);
			}
		}
		const other = /** @type {HTMLElement} */ (this.otherEl);
		clear(other);
		const loose = [...this.mistakes.other, ...(this.mistakes.dash.get('') || [])];
		// A key of the dashboard no field shows (one a body may not have) is said here too.
		const shown = new Set(this.keysEl ? [...this.keysEl.querySelectorAll('.ed-field')].map((f) => /** @type {HTMLElement} */ (f).dataset.key) : []);
		for (const [k, list] of this.mistakes.dash) if (k && !shown.has(k)) loose.push(...list.map((m) => `${k}: ${m}`));
		if (this.mistakes.dash.size && this.keysEl) this.keysEl.open = true;
		if (loose.length) other.append(h('div', { class: 'note critical' }, h('p', null, S.editor.mistakesElsewhere), h('ul', null, ...loose.map((m) => h('li', null, m)))));
		if (this.profileCard && this.mistakes.prof.size) {
			this.profileCard.el.open = true;
		}
		if (this.profileCard) this.profileCard.render();
	}

	// ------------------------------------------------------------ status

	/** A line under the top bar, with buttons. @param {string} text @param {'normal' | 'warning' | 'critical'} level @param {...Node} buttons */
	say(text, level, ...buttons) {
		const el = /** @type {HTMLElement} */ (this.statusEl);
		clear(el);
		if (!text) return;
		el.append(h('div', { class: `note ${level === 'normal' ? '' : level}` }, h('span', null, text), buttons.length ? h('span', { class: 'ed-status-buttons' }, ...buttons) : null));
	}

	// ------------------------------------------------------------ writing

	async save() {
		if (this.saving || !this.dirty()) return;
		this.saving = true;
		this.renderDirty();
		/** @type {number | null} */
		let generation = null;
		let base = this.desc.hash;
		try {
			if (this.isNew || isDirty(this.dash)) {
				const r = await api('/api/v1/admin/editor/dashboard', { body: { id: this.id, dashboard: this.dash.value, base } });
				base = r.hash;
				generation = r.generation;
				rebase(this.dash);
				this.isNew = false;
				if (made && made.id === this.id) made = null;
			}
			for (const [id, d] of this.profiles) {
				if (!isDirty(d)) continue;
				this.profileId = this.profileId || id;
				// Only what was changed here: a key the files set stays theirs.
				const r = await api('/api/v1/admin/editor/profile', { body: { id, profile: changes(d.start, d.value), base } });
				base = r.hash;
				generation = r.generation;
				rebase(d);
			}
			this.setMistakes([], 'save');
			this.say(fill(S.editor.saved, generation ?? '?'), 'normal');
			await this.reload(false);
			this.ctx.reloadUi().catch(() => {});
		} catch (e) {
			await this.failed(e, base !== this.desc.hash);
		} finally {
			this.saving = false;
			this.renderDirty();
		}
	}

	/**
	 * A write that didn't go: a file changed meanwhile (the other editor wrote), mistakes, writes
	 * off. @param {unknown} e @param {boolean} partly whether a part went before
	 */
	async failed(e, partly) {
		if (e instanceof ApiError && e.code === 'changed_on_disk') {
			const before = { dash: this.entry ? stable(this.entry.def) : null, profiles: new Map(this.desc.profiles.map((p) => [p.id, stable(p.def)])) };
			await this.reload(false);
			const after = this.entry ? stable(this.entry.def) : null;
			const touched = before.dash !== after || [...this.profiles.keys()].some((id) => {
				const p = this.desc.profiles.find((x) => x.id === id);
				return before.profiles.get(id) !== (p ? stable(p.def) : undefined);
			});
			this.say(
				`${S.editor.changedOnDisk} ${touched ? S.editor.changedHereToo : S.editor.changedElsewhere}`,
				'warning',
				h('button', { type: 'button', class: 'primary', onclick: () => this.save() }, S.editor.saveMine),
				h('button', { type: 'button', onclick: () => this.dropMine() }, S.editor.dropMine),
			);
			return;
		}
		if (e instanceof ApiError && e.code === 'bad_settings') {
			this.setMistakes((e.body && e.body.mistakes) || [], 'save');
			this.say(partly ? `${S.editor.partlySaved} ${S.editor.badSettings}` : S.editor.badSettings, 'critical');
			return;
		}
		this.say(partly ? `${S.editor.partlySaved} ${words(e)}` : words(e), 'critical');
		if (partly) await this.reload(false);
	}

	/** The user's draft goes; the description's is taken. */
	dropMine() {
		this.isNew = false;
		if (made && made.id === this.id) made = null;
		this.say('', 'normal');
		this.reload(true);
	}

	/**
	 * Reads the description again. A draft that changed is kept (unless `reset`), one that didn't
	 * is taken anew from the description.
	 * @param {boolean} reset
	 */
	async reload(reset) {
		try {
			this.desc = await describe();
		} catch (e) {
			this.say(words(e), 'critical');
			return;
		}
		if (this.gone) return;
		this.entry = this.desc.dashboards.find((d) => d.id === this.id) || null;
		if (this.entry && (reset || !isDirty(this.dash)) && !this.isNew) {
			this.dash = draft(this.entry.def || {});
			this.selected = this.selected && widgetAt(this.dash.value, this.selected) ? this.selected : null;
		} else if (!this.entry && reset) {
			this.leaving = true;
			this.ctx.go('#/edit');
			return;
		}
		for (const [id, d] of [...this.profiles]) if (reset || !isDirty(d)) this.profiles.delete(id);
		if (reset) this.clearMistakes();
		this.renderTitle();
		this.renderOrigin();
		this.renderKeys();
		this.canvas.render();
		this.renderSide();
		this.profileCard && this.profileCard.render();
		this.renderDirty();
		this.previewer.soon(0);
	}

	/** "Back to the files": the editor's version of this dashboard taken out. */
	async revert() {
		if (!(await ask(S.editor.revertTitle, S.editor.revertText, S.editor.revert))) return;
		try {
			const r = await api('/api/v1/admin/editor/revert', { body: { dashboard: this.id, base: this.desc.hash } });
			this.say(fill(S.editor.reverted, r.generation ?? '?'), 'normal');
			await this.reload(true);
			this.ctx.reloadUi().catch(() => {});
		} catch (e) {
			await this.failed(e, false);
		}
	}

	/** @param {string} id */
	async revertProfile(id) {
		if (!(await ask(S.editor.revertProfileTitle, S.editor.revertProfileText, S.editor.revert))) return;
		try {
			const r = await api('/api/v1/admin/editor/revert', { body: { profile: id, base: this.desc.hash } });
			this.profiles.delete(id);
			this.say(fill(S.editor.reverted, r.generation ?? '?'), 'normal');
			await this.reload(false);
			this.ctx.reloadUi().catch(() => {});
		} catch (e) {
			await this.failed(e, false);
		}
	}

	/** Deletes a dashboard only the editor's file has (and its place among the editor's tabs). */
	async del() {
		if (!(await ask(S.editor.deleteTitle, fill(S.editor.deleteText, String(this.dash.value.title || this.id)), S.editor.deleteDashboard, true))) return;
		try {
			const r = await api('/api/v1/admin/editor/delete', { body: { dashboard: this.id, base: this.desc.hash } });
			this.ctx.toast(fill(S.editor.deleted, r.generation ?? '?'));
			this.leaving = true;
			this.ctx.reloadUi().catch(() => {});
			this.ctx.go('#/');
		} catch (e) {
			await this.failed(e, false);
		}
	}

	/** Leaves for another address, asking first while something isn't saved. @param {string} hash */
	async leave(hash) {
		if (this.dirty() && !(await ask(S.editor.leaveTitle, S.editor.leaveText, S.editor.leaveGo, true))) return;
		this.leaving = true;
		this.ctx.go(hash);
	}
}
