// `log`: the computer's journal, or its events, as they come: the service's note, chips for the
// least level and a search field (both filter it on the computer, through the stream), then its
// lines, newest at the bottom, following the newest while at the bottom. Older lines load at the
// top from `/api/v1/log`.
import { h, setText } from '../dom.js';
import { api, ApiError } from '../api.js';
import { icon } from '../icons.js';
import { card, chips } from './common.js';
import * as L from '../logs.js';
import { S, fill } from '../strings.js';
import * as F from '../format.js';

/** How long typing has to stop before a search is told to the computer. */
const SEARCH_SETTLE_MS = 400;
/** Within this many pixels of the bottom, the list follows the newest line. */
const BOTTOM_SLACK = 8;

/** @type {Record<string, string>} */
const LEVEL_WORDS = {
	debug: S.panels.levelDebug,
	info: S.panels.levelInfo,
	notice: S.panels.levelNotice,
	warning: S.panels.levelWarning,
	error: S.panels.levelError,
	critical: S.panels.levelCritical,
};

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const events = w.scope === 'events';
	const own = typeof w.level === 'string' ? w.level : L.DEFAULT_LOG_LEVEL;
	const keep = typeof w.lines === 'number' ? w.lines : L.LOG_LINES_DEFAULT;
	const kept = env.asked.log[w.id];
	let level = kept ? kept.level : own;
	let search = kept && kept.search ? kept.search : '';
	/** @type {L.LogLines | null} */
	let model = null;
	let follow = true;
	/** @type {ReturnType<typeof setTimeout> | null} */
	let settle = null;
	/** Lines opened whole, by key. */
	const open = new Set();

	const { el, body } = card(w, env, { icon: events ? 'clock' : 'terminal' });
	const note = h('p', { class: 'log-note' });
	note.hidden = true;
	const picker = chips(S.panels.logLevels, L.levels(own).map((l) => ({ value: l, text: LEVEL_WORDS[l] || l })), level, (l) => {
		level = l;
		tell();
	});
	body.append(note, picker.el);
	if (w.search !== false) {
		const field = h('input', { type: 'search', class: 't-search', maxlength: String(L.LOG_SEARCH_MAX), placeholder: events ? S.panels.logSearchEvents : S.panels.logSearch, 'aria-label': events ? S.panels.logSearchEvents : S.panels.logSearch });
		field.value = search;
		field.addEventListener('input', () => {
			search = field.value;
			if (settle) clearTimeout(settle);
			settle = setTimeout(tell, SEARCH_SETTLE_MS);
		});
		body.append(h('div', { class: 't-search-row' }, icon('search', 'w-icon'), field));
	}
	const olderButton = h('button', { type: 'button', class: 'quiet' }, S.panels.logOlder);
	const olderNote = h('span', { class: 'log-older-failed' }, S.panels.logOlderFailed);
	const spinner = h('span', { class: 'spinner', 'aria-hidden': 'true' });
	const older = h('div', { class: 'log-older' }, spinner, olderNote, olderButton);
	const status = h('p', { class: 'muted log-status' });
	const lines = h('ol', { class: 'log-lines', 'aria-live': 'off' });
	const scroller = h('div', { class: 'log-scroll', tabindex: '0', role: 'log', 'aria-label': w.label || (events ? S.panels.logSearchEvents : S.panels.logSearch) }, older, status, lines);
	const newest = h('button', { type: 'button', class: 'log-newest' }, icon('expand_more'), ` ${S.panels.logNewest}`);
	newest.hidden = true;
	body.append(h('div', { class: 'log-box' }, scroller, newest));

	olderButton.addEventListener('click', loadOlder);
	newest.addEventListener('click', () => {
		follow = true;
		scroller.scrollTop = scroller.scrollHeight;
		newest.hidden = true;
	});
	scroller.addEventListener('scroll', () => {
		const atBottom = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - BOTTOM_SLACK;
		follow = atBottom;
		newest.hidden = atBottom || !model || model.items.length === 0;
	});

	/** Tells the stream the filter, which starts the lines anew. */
	function tell() {
		if (settle) clearTimeout(settle);
		settle = null;
		env.change({ log: { [w.id]: L.filterFor(own, level, search) } });
	}

	async function loadOlder() {
		if (!model || !env.live || model.loadingOlder || !L.canLoadOlder(model)) return;
		const gen = model.generation;
		const before = L.before(model);
		model = L.asking(model);
		render(false);
		const q = new URLSearchParams({ widget: w.id });
		if (before != null) q.set('before', String(before));
		if (level !== own) q.set('level', level);
		if (search.trim()) q.set('search', search.trim());
		try {
			const page = await api(`/api/v1/log?${q}`);
			if (!model || model.generation !== gen) return;
			const height = scroller.scrollHeight;
			model = L.older(model, page || {});
			render(false);
			scroller.scrollTop += scroller.scrollHeight - height;
		} catch (e) {
			if (!model || model.generation !== gen) return;
			// A service before 0.8 has no older lines to give: no button then, rather than an error.
			model = e instanceof ApiError && e.code === 'not_found' ? L.noOlder(model) : L.failed(model);
			render(false);
		}
	}

	/** @type {Map<number, HTMLElement>} */
	const drawn = new Map();

	/** @param {L.LogLine} line @param {number} key */
	function lineEl(line, key) {
		const source = line.id || line.unit;
		const t = L.tone(line.level);
		const li = h('li', { class: `log-line tone-${t}` },
			h('span', { class: 'log-dot', role: 'img', 'aria-label': LEVEL_WORDS[line.level || 'info'] || line.level || '' }),
			h('time', { class: 'log-time mono', datetime: new Date(line.t).toISOString() }, F.clockTime(line.t)),
			h('span', { class: 'log-text' }, source ? h('span', { class: 'log-source' }, source, ' ') : null, line.text || ''));
		li.tabIndex = -1;
		li.addEventListener('click', () => {
			if (open.has(key)) open.delete(key);
			else open.add(key);
			li.classList.toggle('open', open.has(key));
		});
		return li;
	}

	/** @param {boolean} scroll whether to follow the newest */
	function render(scroll = true) {
		if (!model) {
			status.hidden = false;
			setText(status, env.live ? S.panels.loading : S.panels.logPreview);
			older.hidden = true;
			lines.replaceChildren();
			drawn.clear();
			return;
		}
		note.hidden = !model.note;
		setText(note, model.note || '');
		const canOlder = env.live && (L.canLoadOlder(model) || model.loadingOlder);
		older.hidden = !canOlder;
		spinner.hidden = !model.loadingOlder;
		olderButton.hidden = model.loadingOlder;
		olderNote.hidden = !model.olderFailed;
		setText(olderButton, model.olderFailed ? S.core.retry : S.panels.logOlder);
		status.hidden = !(model.items.length === 0 && !model.loadingOlder);
		setText(status, S.panels.logEmpty);
		/** @type {Set<number>} */
		const seen = new Set();
		let prev = /** @type {Element | null} */ (null);
		for (const item of model.items) {
			seen.add(item.key);
			let li = drawn.get(item.key);
			if (!li) {
				li = 'line' in item ? lineEl(item.line, item.key) : h('li', { class: 'log-more muted' }, fill(S.panels.logMore, item.more));
				drawn.set(item.key, li);
			} else if ('more' in item) setText(li, fill(S.panels.logMore, item.more));
			const next = prev ? prev.nextElementSibling : lines.firstElementChild;
			if (next !== li) lines.insertBefore(li, next);
			prev = li;
		}
		for (const [k, li] of drawn) {
			if (!seen.has(k)) {
				li.remove();
				drawn.delete(k);
				open.delete(k);
			}
		}
		if (scroll && follow) scroller.scrollTop = scroller.scrollHeight;
		newest.hidden = follow || model.items.length === 0;
	}
	render();

	return {
		el,
		update(s) {
			if (!s) return;
			const before = model ? model.generation : -1;
			model = L.take(model || L.empty(keep), s, keep);
			if (model.generation !== before && s.reset) follow = true;
			render();
		},
		clear() {
			model = null;
			render();
		},
		destroy() {
			if (settle) clearTimeout(settle);
		},
	};
}
