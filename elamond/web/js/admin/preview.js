// A preview of drafts (`POST admin/preview`): the `ui` the settings would give with them, and one
// dashboard's data from the reading there is now, drawn as the dashboards are and asked again at
// the profile's interval while it is open and in sight. Nothing is written, nothing of the drafts
// is run.
import { h, clear } from '../dom.js';
import { S, fill } from '../strings.js';
import { dashboardView, DRAWS } from '../panels.js';
import { failure } from './ui.js';
import { errorSentence, isRefusal, mistakesOf } from './errors.js';
import { draftDashboards } from './text.js';
import { dateTimeMs } from './time.js';
import { PREVIEW_MIN_MS, intervalOf } from '../consts.js';

/** @typedef {import('../types.js').Ui} Ui */

/**
 * The preview's `ui`, with what an answer may leave out (the service, the profile, control) taken
 * from the page's own, so a dashboard draws as it would.
 * @param {any} ui @param {Ui | null} own @returns {Ui}
 */
export function previewUi(ui, own) {
	const u = ui && typeof ui === 'object' ? ui : {};
	return /** @type {Ui} */ ({
		...u,
		version: u.version ?? '',
		service: u.service ?? own?.service,
		profile: u.profile ?? own?.profile,
		control: u.control ?? own?.control,
		role: u.role ?? own?.role,
		tabs: Array.isArray(u.tabs) ? u.tabs : [],
		header: u.header ?? null,
		dashboards: Array.isArray(u.dashboards) ? u.dashboards : [],
		actions: Array.isArray(u.actions) ? u.actions : [],
	});
}

/**
 * The id in the settings of a dashboard in `ui`: one made `for_each` is there once for each item,
 * as `<id>.<item>` (a settings id has no dot), and the preview is asked for by its own id.
 * @param {string} id
 */
export function settingsId(id) {
	const dot = id.indexOf('.');
	return dot > 0 ? id.slice(0, dot) : id;
}

/**
 * The dashboards a preview may show, by their ids in the settings (never an item's of a
 * `for_each`): the preview's tabs and dashboards, those the drafts have tables for, the page's own
 * tabs; each once, with its title where one is known.
 * @param {Ui | null} ui @param {string[]} texts @param {Ui | null} own @returns {{ id: string, title: string }[]}
 */
export function dashboardChoices(ui, texts, own) {
	/** @type {Map<string, string>} */
	const out = new Map();
	const titles = new Map([...(own?.dashboards || []), ...(ui?.dashboards || [])].map((d) => [d.id, d.title]));
	const add = (/** @type {string} */ shown) => {
		const id = shown ? settingsId(shown) : '';
		if (id && !out.has(id)) out.set(id, titles.get(id) || id);
	};
	for (const id of ui?.tabs || []) add(id);
	for (const d of ui?.dashboards || []) add(d.id);
	for (const t of texts) for (const id of draftDashboards(t)) add(id);
	for (const id of own?.tabs || []) add(id);
	return [...out].map(([id, title]) => ({ id, title }));
}

/**
 * Opens the preview of `files` (each `{ name, text }`). Returns what closes it.
 * @param {import('./types.js').Env} env @param {{ name: string, text: string }[]} files
 * @param {(file: string, line: number | null) => void} openMistake
 * @returns {() => void}
 */
export function openPreview(env, files, openMistake) {
	const T = S.admin.preview;
	const own = env.ctx.ui();
	const texts = files.map((f) => f.text);
	const first = dashboardChoices(null, texts, own)[0];
	let dashboard = first ? first.id : '';
	let closed = false;
	let timer = 0;
	let seq = 0;
	/** @type {import('../types.js').DashboardView | null} */
	let view = null;
	let drawnFor = '';
	/** @type {Ui | null} */
	let last = null;

	const picker = h('select', { id: 'adm-pv-dashboard' });
	const status = h('span', { class: 'muted adm-pv-status', 'aria-live': 'polite' });
	const stage = h('div', { class: 'adm-pv-stage' });
	const closeBtn = h('button', { type: 'button' }, S.core.close);
	const dlg = h(
		'dialog',
		{ class: 'adm-dialog adm-preview', 'aria-label': T.title },
		h(
			'div',
			{ class: 'adm-pv-head' },
			h('h2', null, T.title),
			h('label', { for: 'adm-pv-dashboard', class: 'adm-inline' }, T.dashboard),
			picker,
			status,
			closeBtn,
		),
		h('p', { class: 'note adm-pv-note' }, T.nothingWritten, ' ', h('span', { class: 'mono' }, files.map((f) => f.name).join(', '))),
		stage,
	);

	const fillPicker = () => {
		const choices = dashboardChoices(last, texts, own);
		clear(picker);
		for (const c of choices) picker.append(h('option', { value: c.id, selected: c.id === dashboard }, c.title === c.id ? c.id : fill(T.choice, c.title, c.id)));
		picker.disabled = !choices.length;
	};

	/** A next ask waits for the page to be in sight again. */
	let due = false;
	const later = (/** @type {number} */ ms) => {
		clearTimeout(timer);
		timer = 0;
		if (closed) return;
		if (document.hidden) due = true;
		else timer = window.setTimeout(ask, ms);
	};
	// Out of sight, nothing is asked; in sight again, what was due at once.
	const onVisible = () => {
		if (document.hidden) {
			if (timer) later(0);
		} else if (due) {
			due = false;
			later(0);
		}
	};
	document.addEventListener('visibilitychange', onVisible);

	const interval = () => Math.max(PREVIEW_MIN_MS, intervalOf(last));

	async function ask() {
		timer = 0;
		if (closed) return;
		const my = ++seq;
		if (!dashboard) {
			clear(stage);
			stage.append(h('p', { class: 'note warning' }, T.noDashboard));
			return;
		}
		try {
			const answer = await env.call('preview', { files, dashboard, widgets: DRAWS });
			if (closed || my !== seq) return;
			const ui = previewUi(answer?.ui, own);
			last = ui;
			// A `for_each` dashboard is shown by its first item's: the one the data is of.
			const shown = typeof answer?.data?.dashboard === 'string' && answer.data.dashboard ? answer.data.dashboard : dashboard;
			const key = `${shown} ${ui.version}`;
			if (!view || key !== drawnFor) {
				view?.destroy();
				view = null;
				clear(stage);
				try {
					view = dashboardView(ui, shown, { preview: true });
					stage.append(view.el);
					drawnFor = key;
				} catch (e) {
					stage.append(h('p', { class: 'note critical' }, fill(T.cantDraw, String(e instanceof Error ? e.message : e))));
					return;
				}
				fillPicker();
			}
			if (answer?.data) view.update(answer.data);
			status.textContent = fill(T.asOf, dateTimeMs(answer?.data?.t || Date.now()));
			later(interval());
		} catch (e) {
			if (closed || my !== seq || env.quiet(e)) return;
			view?.destroy();
			view = null;
			drawnFor = '';
			clear(stage);
			status.textContent = '';
			if (isRefusal(e) && e.code === 'bad_settings') {
				// The drafts don't change while the preview is open: asking again would say the same.
				stage.append(h('p', { class: 'note critical' }, errorSentence(e)), mistakesList(mistakesOf(e), (file, line) => {
					dlg.close();
					openMistake(file, line);
				}));
			} else if (isRefusal(e) && e.code === 'no_such_dashboard') {
				stage.append(h('p', { class: 'note warning' }, fill(T.noSuchDashboard, dashboard)));
				fillPicker();
			} else if (isRefusal(e)) {
				stage.append(failure(e, () => ask()));
			} else {
				// Out of reach for now: try again at the interval.
				stage.append(failure(e, null));
				later(interval());
			}
		}
	}

	picker.addEventListener('change', () => {
		dashboard = picker.value;
		drawnFor = '';
		ask();
	});
	closeBtn.addEventListener('click', () => dlg.close());
	dlg.addEventListener('close', () => {
		closed = true;
		clearTimeout(timer);
		document.removeEventListener('visibilitychange', onVisible);
		view?.destroy();
		view = null;
		dlg.remove();
	});
	document.body.append(dlg);
	env.keep(dlg);
	fillPicker();
	dlg.showModal();
	ask();
	return () => {
		if (!closed) dlg.close();
	};
}

/**
 * Mistakes, each going to its file and line when chosen.
 * @param {import('./types.js').Mistake[]} mistakes @param {(file: string, line: number | null) => void} go
 */
export function mistakesList(mistakes, go) {
	return h(
		'ul',
		{ class: 'adm-mistakes' },
		...mistakes.map((m) =>
			h(
				'li',
				null,
				h(
					'button',
					{ type: 'button', class: 'adm-mistake', onclick: () => go(m.file, m.line ?? null) },
					h('span', { class: 'mono adm-where' }, m.line ? `${m.file}:${m.line}` : m.file),
					m.path ? h('span', { class: 'mono adm-path' }, m.path) : null,
					h('span', { class: 'adm-msg' }, m.message),
				),
			),
		),
	);
}

