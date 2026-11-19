// The merged settings as a tree to fold and filter, each part with the file and line that set it.
// `whence` names paths up to three parts deep (`dashboards.cpu.title`, `dashboards.cpu.widgets[2]`)
// as [index in `files`, line]; a deeper part has none of its own, and its nearest part above has it.
import { INLINE_LIST_MAX } from '../consts.js';

/** @typedef {{ file: string, line: number }} Where */

/**
 * A part of the settings: a table, a list or a value. `path` is how `whence` names it; `text` a
 * value as text (empty for a table or a list); `size` how many keys or items it holds; `hit`, when
 * filtered, that the part matched itself (its parts are kept whole) rather than holds one that did.
 * @typedef {{ key: string, path: string, kind: 'table' | 'list' | 'value', text: string, size: number,
 *   where: Where | null, children: TreeNode[], hit?: boolean }} TreeNode
 */

const BARE = /^[A-Za-z0-9_-]+$/;

/** A key as a path writes it: bare, or quoted when it holds anything but letters, digits, `_` and `-`. @param {string} key */
export function pathKey(key) {
	return BARE.test(key) ? key : JSON.stringify(key);
}

/** @param {unknown} v @returns {v is Record<string, unknown>} */
function isTable(v) {
	return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** @param {unknown} v */
function isPlain(v) {
	return v === null || typeof v !== 'object';
}

/**
 * A value as the settings would write it: strings quoted, lists in brackets, tables in braces.
 * @param {unknown} v @returns {string}
 */
export function valueText(v) {
	if (typeof v === 'string') return JSON.stringify(v);
	if (v === null || v === undefined) return 'null';
	if (Array.isArray(v)) return `[${v.map(valueText).join(', ')}]`;
	if (isTable(v)) {
		const parts = Object.entries(v).map(([k, x]) => `${pathKey(k)} = ${valueText(x)}`);
		return parts.length ? `{ ${parts.join(', ')} }` : '{}';
	}
	return String(v);
}

/**
 * Where `whence` says a path was set, or null. A key with a dot or a quote in it may be written
 * either quoted (`dashboards."ups.eco"`) or as it is (`dashboards.ups.eco`): both are looked up.
 * @param {Record<string, [number, number]>} whence @param {string[]} files @param {...string} paths
 * @returns {Where | null}
 */
export function lookup(whence, files, ...paths) {
	for (const p of paths) {
		const w = Object.prototype.hasOwnProperty.call(whence, p) ? whence[p] : null;
		if (Array.isArray(w) && w.length >= 2 && typeof files[w[0]] === 'string' && Number.isFinite(w[1])) return { file: files[w[0]], line: w[1] };
	}
	return null;
}

/**
 * The settings as a tree, in the order the service gives their keys.
 * @param {unknown} settings @param {Record<string, [number, number]> | null | undefined} whence
 * @param {string[] | null | undefined} files @returns {TreeNode[]}
 */
export function buildTree(settings, whence, files) {
	if (!isTable(settings)) return [];
	const w = whence || {};
	const f = files || [];
	return Object.keys(settings).map((k) => node(k, pathKey(k), k, settings[k], w, f));
}

/**
 * @param {string} key @param {string} path @param {string} raw the path with keys unquoted
 * @param {unknown} value @param {Record<string, [number, number]>} whence @param {string[]} files
 * @returns {TreeNode}
 */
function node(key, path, raw, value, whence, files) {
	const where = lookup(whence, files, path, raw);
	if (isTable(value)) {
		const children = Object.keys(value).map((k) => node(k, `${path}.${pathKey(k)}`, `${raw}.${k}`, value[k], whence, files));
		return { key, path, kind: 'table', text: '', size: children.length, where, children };
	}
	if (Array.isArray(value)) {
		if (value.every(isPlain)) {
			const text = valueText(value);
			if (text.length <= INLINE_LIST_MAX) return { key, path, kind: 'value', text, size: value.length, where, children: [] };
		}
		const children = value.map((v, i) => node(`[${i}]`, `${path}[${i}]`, `${raw}[${i}]`, v, whence, files));
		return { key, path, kind: 'list', text: '', size: children.length, where, children };
	}
	return { key, path, kind: 'value', text: valueText(value), size: 0, where, children: [] };
}

/**
 * The parts whose path, value or file:line holds `query` (any case), each kept whole, and the parts
 * above them, holding only what leads to one. An empty query keeps everything as it is.
 * @param {TreeNode[]} nodes @param {string} query @returns {TreeNode[]}
 */
export function filterTree(nodes, query) {
	const q = query.trim().toLowerCase();
	if (!q) return nodes;
	return filterBy(nodes, q);
}

/** @param {TreeNode[]} nodes @param {string} q @returns {TreeNode[]} */
function filterBy(nodes, q) {
	/** @type {TreeNode[]} */
	const out = [];
	for (const n of nodes) {
		if (matches(n, q)) out.push({ ...n, hit: true });
		else {
			const children = filterBy(n.children, q);
			if (children.length) out.push({ ...n, children, hit: false });
		}
	}
	return out;
}

/** @param {TreeNode} n @param {string} q */
function matches(n, q) {
	return n.path.toLowerCase().includes(q) || n.text.toLowerCase().includes(q) || (!!n.where && `${n.where.file}:${n.where.line}`.toLowerCase().includes(q));
}

/** How many parts of a filtered tree matched (each kept whole counts once). @param {TreeNode[]} nodes @returns {number} */
export function countHits(nodes) {
	let n = 0;
	for (const x of nodes) n += x.hit ? 1 : countHits(x.children);
	return n;
}

/** How many parts a tree holds, all the way down. @param {TreeNode[]} nodes @returns {number} */
export function countNodes(nodes) {
	let n = 0;
	for (const x of nodes) n += 1 + countNodes(x.children);
	return n;
}
