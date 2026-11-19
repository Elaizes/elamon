// `table`: rows from a collection, in a card: a heading for the columns that have labels, each
// row's text and figures, a bar under a row where a column is one. With `search`, a search field
// (searched here, in the rows there are), chips for the orders the service offers (asked of the
// stream, and kept here until it sends them so) and how many there are. A row opens its link
// (`row_open`), or shows what `expand` adds and its actions (`row_actions`).
import { h, setText, setStyle, safeUrl } from '../dom.js';
import { icon } from '../icons.js';
import { card, captionLine, chips, setLevel } from './common.js';
import { searchFields, searchIn, sortRows, barFractions, fillParams, rowButtons, linkUrl } from '../tables.js';
import { S, fill } from '../strings.js';
import * as F from '../format.js';

/** The most rows a table draws; those past them are counted under it. */
const CARD_ROWS = 100;
/** A table with a search field draws more: the search narrows them. */
const SEARCH_ROWS = 300;
/** An expanded field longer than this is set as a block of its own, like a command line. */
const LONG_FIELD = 40;

/** @typedef {import('../tables.js').Row} Row @typedef {import('../tables.js').Column} Column */

/** How wide a column that doesn't grow is, by what it holds. @param {Column} c */
function widthClass(c) {
	switch (c.format) {
		case 'percent':
		case 'temp':
			return 'c-narrow';
		case 'integer':
		case 'number':
		case 'load':
		case 'bool':
			return 'c-number';
		case 'time':
			return 'c-time';
		default:
			return 'c-wide';
	}
}

/** A cell's text: its value by its column's format and unit; a dash while it is missing. @param {Row} row @param {Column} c */
function cell(row, c) {
	return F.format(row.f ? row.f[c.field] : null, c.format, c.unit, c.field) ?? F.DASH;
}

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	/** @type {Column[]} */
	const cols = Array.isArray(w.columns) ? w.columns : [];
	const grow = cols.find((c) => c.grow) || cols[0] || null;
	const barCol = cols.find((c) => c.bar) || null;
	const others = cols.filter((c) => c !== grow && c !== barCol);
	const avatars = w.source === 'processes' || w.source === 'links';
	const labelled = !!w.open && env.canOpen(w.open) && env.isTab;
	const hasSheet = (Array.isArray(w.expand) && w.expand.length > 0) || (Array.isArray(w.row_actions) && w.row_actions.length > 0);
	const own = w.sort && w.sort.field ? w.sort.field : null;
	const sorts = [...new Set([own, ...(Array.isArray(w.sorts) ? w.sorts : [])].filter(Boolean))];
	const queryKey = `${w.id}#q`;
	let query = env.memory.get(queryKey) || '';
	let asked = env.asked.sort[w.id] || null;
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	let loaded = false;
	/** The rows opened, by key, with their last readings (none after the connection went). @type {Map<string, Row | null>} */
	const opened = new Map();

	const { el, body } = card(w, env, { onFold: () => render() });
	const cap = captionLine();
	body.append(cap.el);

	// Search, orders, how many.
	const count = h('p', { class: 't-count muted' });
	if (w.search) {
		const field = h('input', { type: 'search', class: 't-search', placeholder: S.panels.search, 'aria-label': w.label ? `${S.panels.search}: ${w.label}` : S.panels.search, enterkeyhint: 'search' });
		field.value = query;
		field.addEventListener('input', () => {
			query = field.value;
			env.memory.set(queryKey, query);
			render();
		});
		body.append(h('div', { class: 't-search-row' }, icon('search', 'w-icon'), field));
		if (sorts.length > 1) {
			const label = (/** @type {string} */ f) => (w.sort_labels && w.sort_labels[f]) || (cols.find((c) => c.field === f) || {}).label || ((w.expand || []).find((/** @type {any} */ e) => e.field === f) || {}).label || f.charAt(0).toUpperCase() + f.slice(1);
			const picker = chips(S.panels.sortBy, sorts.map((f) => ({ value: /** @type {string} */ (f), text: label(/** @type {string} */ (f)) })), asked || own || '', (f) => {
				asked = f === own ? null : f;
				env.change({ sort: { [w.id]: asked } });
				render();
			});
			body.append(picker.el);
		}
		body.append(count);
	}

	// The columns' labels, where any has one.
	if (!labelled && cols.some((c) => c.label)) {
		const head = h('div', { class: 't-head muted', 'aria-hidden': 'true' });
		if (avatars) head.append(h('span', { class: 't-avatar-space' }));
		head.append(h('span', { class: 't-name' }, grow && grow.label ? grow.label : ''));
		for (const c of others) head.append(h('span', { class: `t-cell ${widthClass(c)}` }, c.label || ''));
		if (barCol) head.append(h('span', { class: `t-cell ${widthClass(barCol)}` }, barCol.label || ''));
		if (hasSheet) head.append(h('span', { class: 't-more-space' }));
		body.append(head);
	}
	const list = h('ul', { class: 't-rows', 'aria-label': w.label || null });
	const status = h('p', { class: 't-status muted' });
	const more = h('p', { class: 't-more muted' });
	body.append(list, status, more);

	/** @type {Map<string, ReturnType<typeof makeRow>>} */
	const rowEls = new Map();

	/** One row's elements, made once and filled in each line. @param {string} key */
	function makeRow(key) {
		const name = h('span', { class: 't-name' });
		const avatar = avatars ? h('span', { class: 't-avatar', 'aria-hidden': 'true' }) : null;
		const cells = others.map((c) => h('span', { class: `t-cell ${labelled ? 'labelled' : widthClass(c)}` }));
		const barText = barCol ? h('span', { class: `t-cell t-barfig ${widthClass(barCol)}` }) : null;
		const line = h('span', { class: 't-line' }, avatar, name, ...cells, barText);
		const fillEl = h('span', { class: 'bar-fill' });
		const bar = barCol ? h('span', { class: 'bar thin', 'aria-hidden': 'true' }, fillEl) : null;
		/** @type {HTMLAnchorElement | HTMLButtonElement | HTMLDivElement} */
		let main;
		const link = h('a', { class: 't-main', target: '_blank', rel: 'noopener noreferrer' });
		const toggle = h('button', { type: 'button', class: 't-main' });
		const plain = h('div', { class: 't-main' });
		const details = h('button', { type: 'button', class: 'icon-button quiet t-details', 'aria-label': S.panels.details, title: S.panels.details });
		const sheet = h('div', { class: 't-sheet' });
		const li = h('li', { class: 't-row', 'data-key': key });
		const flip = () => {
			if (opened.has(key)) opened.delete(key);
			else {
				const r = rows.get(key);
				if (r) opened.set(key, r);
			}
			render();
		};
		toggle.addEventListener('click', flip);
		details.addEventListener('click', flip);
		/** @type {Map<string, Row>} */
		const rows = new Map();
		return {
			li,
			/** @param {Row} row @param {number | null} fraction @param {boolean} gone */
			fill(row, fraction, gone) {
				rows.set(key, row);
				const text = grow ? cell(row, grow) : '';
				setText(name, text);
				if (avatar) setText(avatar, (text.trim()[0] || '?').toUpperCase());
				others.forEach((c, i) => setText(cells[i], labelled && c.label ? `${cell(row, c)} ${c.label}` : cell(row, c)));
				if (barCol && barText && bar) {
					setText(barText, cell(row, barCol));
					setLevel(barText, row.level, row.blend);
					setLevel(fillEl, row.level, row.blend);
					barText.classList.toggle('temp', barCol.format === 'temp');
					fillEl.classList.toggle('temp', barCol.format === 'temp');
					setStyle(fillEl, { width: `${((fraction || 0) * 100).toFixed(1)}%` });
				}
				const url = w.row_open && row.f ? row.f[w.row_open] : null;
				const href = typeof url === 'string' ? safeUrl(linkUrl(url, location.hostname)) : null;
				const want = href ? link : hasSheet ? toggle : plain;
				if (main !== want) {
					main = want;
					main.replaceChildren(line);
					if (bar) main.append(bar);
					li.replaceChildren(main);
					if (href && hasSheet) li.append(details);
				}
				if (href) link.href = href;
				const isOpen = opened.has(key);
				const expanded = String(isOpen);
				if (want === toggle) toggle.setAttribute('aria-expanded', expanded);
				details.setAttribute('aria-expanded', expanded);
				details.replaceChildren(icon(isOpen ? 'expand_less' : 'expand_more'));
				li.classList.toggle('gone', gone);
				if (isOpen) {
					fillSheet(sheet, row, gone, href);
					if (!sheet.isConnected) li.append(sheet);
				} else sheet.remove();
			},
		};
	}

	/** What an opened row shows: its figures, what `expand` adds, its link and its actions. @param {HTMLElement} sheet @param {Row} row @param {boolean} gone @param {string | null} href */
	function fillSheet(sheet, row, gone, href) {
		const name = grow ? cell(row, grow) : row.key;
		/** @type {Node[]} */
		const parts = [];
		if (gone) parts.push(h('p', { class: 't-gone' }, S.panels.rowGone));
		const figures = cols.filter((c) => c !== grow);
		if (figures.length) {
			parts.push(h('div', { class: 't-figures' }, ...figures.map((c) => {
				const v = h('span', { class: 't-figure-value' }, cell(row, c));
				if (c.bar) setLevel(v, row.level, row.blend);
				return h('span', { class: 't-figure' }, h('span', { class: 't-figure-label muted' }, c.label || c.field), v);
			})));
		}
		for (const e of Array.isArray(w.expand) ? w.expand : []) {
			let v = F.format(row.f ? row.f[e.field] : null, e.format, e.unit, e.field);
			if (v == null || !v.trim()) continue;
			if (e.field === w.row_open) v = linkUrl(v, location.hostname);
			const label = e.label || e.field;
			parts.push(v.length > LONG_FIELD ? h('div', { class: 't-field long' }, h('span', { class: 'muted' }, label), h('code', { class: 'mono' }, v)) : h('div', { class: 't-field' }, h('span', { class: 'muted' }, label), h('span', null, v)));
		}
		const buttons = h('div', { class: 't-actions' });
		if (href) buttons.append(h('a', { class: 'button', href, target: '_blank', rel: 'noopener noreferrer' }, icon('open', 'w-icon'), ` ${S.panels.openLink}`));
		for (const b of rowButtons(env.ui, w, row)) {
			if (b.opens) {
				const target = env.ui.dashboards.find((d) => d.id === b.opens);
				const label = b.rowAction.label || (target && target.title) || b.opens;
				buttons.append(h('button', { type: 'button', onclick: () => env.open(/** @type {string} */ (b.opens)) }, label));
				continue;
			}
			const action = b.runs;
			const label = b.rowAction.label || action.name;
			const params = fillParams(b.rowAction.params, row.f || {});
			const running = busy.has(`${action.id}/${row.key}`);
			const button = h('button', { type: 'button', class: params.signal === 'kill' ? 'danger-soft' : null, 'aria-busy': String(running) }, label);
			button.disabled = gone || running || !env.controlOn || busy.size > 0;
			button.addEventListener('click', async () => {
				const k = `${action.id}/${row.key}`;
				busy.add(k);
				render();
				await env.run(action, params, w, `${label} ${name}`);
				busy.delete(k);
				render();
			});
			buttons.append(button);
		}
		if (buttons.childElementCount) parts.push(buttons);
		sheet.replaceChildren(...parts);
	}
	/** Row actions running now: while one runs, the others wait. @type {Set<string>} */
	const busy = new Set();

	function render() {
		const s = last;
		const all = s && Array.isArray(s.rows) ? s.rows : [];
		cap.set(typeof w.caption === 'string' && w.caption ? w.caption : s && s.caption ? s.caption : null);
		const searched = w.search ? searchIn(all, searchFields(w), query) : all;
		const rows = asked && asked !== own ? sortRows(searched, asked, true) : searched;
		const most = w.search ? SEARCH_ROWS : CARD_ROWS;
		const drawn = rows.slice(0, most);
		// A row opened that left the list keeps its last readings, marked as such; after a drop there
		// are none to keep: it stays open, and shows again with the list.
		const keys = new Set(drawn.map((r) => r.key));
		for (const [k, r] of opened) {
			const now = all.find((x) => x.key === k);
			if (now) opened.set(k, now);
			else if (r && s && !keys.has(k)) drawn.push(r);
		}
		const fractions = barCol ? barFractions(drawn, barCol) : null;
		/** @type {Set<string>} */
		const seen = new Set();
		let prev = /** @type {Element | null} */ (null);
		drawn.forEach((row, i) => {
			let key = String(row.key);
			// Two rows with one key from the service are two rows here too.
			while (seen.has(key)) key += '#';
			seen.add(key);
			let r = rowEls.get(key);
			if (!r) {
				r = makeRow(key);
				rowEls.set(key, r);
			}
			r.fill(row, fractions ? fractions[i] : null, !!s && !all.some((x) => x.key === row.key));
			const next = prev ? prev.nextElementSibling : list.firstElementChild;
			if (next !== r.li) list.insertBefore(r.li, next);
			prev = r.li;
		});
		for (const [k, r] of rowEls) {
			if (!seen.has(k)) {
				r.li.remove();
				rowEls.delete(k);
			}
		}
		status.hidden = drawn.length > 0;
		setText(status, s ? S.panels.empty : S.panels.loading);
		const left = rows.length - Math.min(rows.length, most);
		more.hidden = left <= 0;
		setText(more, fill(S.panels.moreRows, left));
		if (w.search) {
			const total = s && typeof s.total === 'number' ? s.total : null;
			count.hidden = total == null;
			if (total != null) setText(count, !query.trim() || rows.length === total ? fill(S.panels.total, total) : fill(S.panels.found, rows.length, total));
		}
	}

	render();
	return {
		el,
		update(s) {
			last = s;
			loaded = true;
			render();
		},
		clear() {
			last = undefined;
			for (const k of opened.keys()) opened.set(k, null);
			render();
		},
		// A table that hides while empty leaves the grid while it is, once it has said so.
		shown: () => !(w.hide_empty && loaded && last && (!Array.isArray(last.rows) || last.rows.length === 0)),
	};
}
