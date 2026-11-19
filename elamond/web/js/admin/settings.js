// The settings as the service merged them (`GET admin/settings`): a tree to fold and filter, each
// part with the file:line that set it, opening the files editor there; the changes against the
// defaults; the warnings of `elamond check`.
import { h, clear } from '../dom.js';
import { S, fill } from '../strings.js';
import { loadInto, head, button, card, chip, whereLink, debounce } from './ui.js';
import { buildTree, filterTree, countNodes, countHits } from './tree.js';
import { FILTER_DEBOUNCE_MS } from './consts.js';

/** @typedef {import('./tree.js').TreeNode} TreeNode */

/** The parts left open, kept while the page is open. */
const open = new Set();
/** The filter as last typed. */
let lastQuery = '';

/** @type {import('./index.js').Mount} */
export function mountSettings(main, env) {
	/** @type {(() => void) & { cancel: () => void } | null} */
	let later = null;
	loadInto(main, env, () => env.call('settings'), (st, reload) => {
		later = draw(main, /** @type {import('./types.js').AdminSettings} */ (st), reload);
	});
	return () => later?.cancel();
}

/** @param {HTMLElement} main @param {import('./types.js').AdminSettings} st @param {() => void} reload */
function draw(main, st, reload) {
	const T = S.admin.settings;
	const tree = buildTree(st.settings, st.whence, st.files);
	const filter = h('input', { type: 'search', class: 'adm-filter', placeholder: T.filter, 'aria-label': T.filter, autocomplete: 'off', spellcheck: 'false' });
	filter.value = lastQuery;
	const count = h('p', { class: 'muted adm-count' });
	const box = h('div', { class: 'adm-tree mono' });
	const redraw = () => {
		lastQuery = filter.value;
		const nodes = filterTree(tree, filter.value);
		clear(box);
		const filtering = !!filter.value.trim();
		count.textContent = filtering ? fill(T.matching, countHits(nodes)) : fill(T.parts, countNodes(tree));
		if (!nodes.length) box.append(h('p', { class: 'muted' }, filtering ? T.noMatch : T.empty));
		else for (const n of nodes) box.append(drawNode(n, filtering));
	};
	const later = debounce(redraw, FILTER_DEBOUNCE_MS);
	filter.addEventListener('input', later);
	redraw();

	main.append(
		head(S.admin.nav.settings, button(T.foldAll, () => {
			open.clear();
			redraw();
		}), button(S.admin.refresh, reload)),
		h('p', { class: 'muted' }, T.intro),
		filesLine(st.files),
		card(T.merged, filter, count, box),
		changesCard(st),
		warningsCard(st.warnings),
	);
	return later;
}

/** The files the settings were read from, in order. @param {string[]} files */
function filesLine(files) {
	return h('p', { class: 'adm-read-from' }, S.admin.settings.readFrom, ' ', ...(files || []).flatMap((f, i) => [i ? S.admin.marks.then : '', whereLink(f, null)]));
}

/**
 * A part of the tree. A table or a list folds; its parts are drawn when it is first opened.
 * Filtering, a part kept for what it holds is open, one that matched itself is closed.
 * @param {TreeNode} n @param {boolean} filtering @returns {HTMLElement}
 */
function drawNode(n, filtering) {
	const T = S.admin.settings;
	const where = n.where ? whereLink(n.where.file, n.where.line) : null;
	const key = h('span', { class: n.hit ? 'adm-key hit' : 'adm-key' }, n.key);
	if (n.kind === 'value') {
		return h('div', { class: 'adm-leaf' }, key, h('span', { class: 'adm-eq' }, S.admin.marks.eq), h('span', { class: 'adm-val' }, n.text), where ? h('span', { class: 'adm-at' }, where) : null);
	}
	const size = h('span', { class: 'adm-size' }, n.kind === 'table' ? fill(n.size === 1 ? T.oneKey : T.keys, n.size) : fill(n.size === 1 ? T.oneItem : T.items, n.size));
	const kids = h('div', { class: 'adm-kids' });
	const isOpen = filtering ? n.hit === false : open.has(n.path);
	const details = h('details', { class: 'adm-branch', open: isOpen }, h('summary', null, key, ' ', size, where ? h('span', { class: 'adm-at' }, where) : null), kids);
	let drawn = false;
	const fillKids = () => {
		if (drawn) return;
		drawn = true;
		for (const c of n.children) kids.append(drawNode(c, filtering));
	};
	if (isOpen) fillKids();
	details.addEventListener('toggle', () => {
		if (details.open) {
			fillKids();
			if (!filtering) open.add(n.path);
		} else if (!filtering) open.delete(n.path);
	});
	return details;
}

/** What differs from the built-in defaults (`elamond defaults --diff`). @param {import('./types.js').AdminSettings} st */
function changesCard(st) {
	const T = S.admin.settings;
	const kinds = /** @type {Record<string, string>} */ (T.changeKinds);
	const changes = st.changes || [];
	return card(
		fill(T.changes, changes.length),
		changes.length
			? h(
					'ul',
					{ class: 'adm-changes' },
					...changes.map((c) =>
						h(
							'li',
							null,
							chip(kinds[c.kind] || c.kind, `ch-${c.kind}`),
							' ',
							h('span', { class: 'mono' }, c.path),
							c.file ? h('span', { class: 'adm-at' }, whereLink(c.file, c.line)) : null,
						),
					),
				)
			: h('p', { class: 'muted' }, T.noChanges),
	);
}

/** @param {string[] | null | undefined} warnings */
function warningsCard(warnings) {
	const T = S.admin.settings;
	if (!warnings || !warnings.length) return card(T.warnings, h('p', { class: 'muted' }, T.noWarnings));
	return card(T.warnings, h('ul', { class: 'adm-warnings' }, ...warnings.map((w) => h('li', { class: 'note warning' }, w))));
}
