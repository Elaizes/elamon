// The lines of a unified diff (the service's `diff`: the file on disk → the draft), each with its
// kind and its line numbers on either side.

/**
 * `hunk` is an `@@` line, `add` and `del` a line only the draft or only the disk has, `same` one
 * both have, `meta` anything else (a file header before the first hunk, `\ No newline…`). `old` is
 * the line's number on disk, `new` in the draft, where it has one; `text` is without its marker.
 * @typedef {{ kind: 'hunk' | 'add' | 'del' | 'same' | 'meta', text: string, old: number | null, new: number | null }} DiffLine
 */

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** @param {string[] | null | undefined} lines @returns {DiffLine[]} */
export function classifyDiff(lines) {
	let oldN = 0;
	let newN = 0;
	let inHunk = false;
	/** @type {DiffLine[]} */
	const out = [];
	for (const raw of lines || []) {
		const l = String(raw);
		const m = HUNK.exec(l);
		if (m) {
			oldN = Number(m[1]);
			newN = Number(m[2]);
			inHunk = true;
			out.push({ kind: 'hunk', text: l, old: null, new: null });
		} else if (!inHunk || l.startsWith('\\')) {
			out.push({ kind: 'meta', text: l, old: null, new: null });
		} else if (l.startsWith('+')) {
			out.push({ kind: 'add', text: l.slice(1), old: null, new: newN++ });
		} else if (l.startsWith('-')) {
			out.push({ kind: 'del', text: l.slice(1), old: oldN++, new: null });
		} else {
			// A line both have; an empty one is a context line whose space was trimmed.
			out.push({ kind: 'same', text: l.startsWith(' ') ? l.slice(1) : l, old: oldN++, new: newN++ });
		}
	}
	return out;
}

/** How many lines the draft adds and takes away. @param {DiffLine[]} lines */
export function diffCounts(lines) {
	let added = 0;
	let removed = 0;
	for (const l of lines) {
		if (l.kind === 'add') added++;
		else if (l.kind === 'del') removed++;
	}
	return { added, removed };
}
