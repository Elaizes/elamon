// Text the admin panel works with: a file's lines (for the editor's line numbers and going to a
// line), a size, a pairing code, and the dashboards a draft has.
import { S, fill } from '../strings.js';

/** How many lines a text has, as an editor numbers them (a last empty one after a final newline too). @param {string} text */
export function lineCount(text) {
	let n = 1;
	for (let i = text.indexOf('\n'); i !== -1; i = text.indexOf('\n', i + 1)) n++;
	return n;
}

/** Where line `line` (from 1) starts in `text`, kept within the text's lines. @param {string} text @param {number} line */
export function offsetOfLine(text, line) {
	let at = 0;
	for (let n = 1; n < line; n++) {
		const i = text.indexOf('\n', at);
		if (i === -1) break;
		at = i + 1;
	}
	return at;
}

/** Where line `line` ends (before its newline). @param {string} text @param {number} line */
export function endOfLine(text, line) {
	const start = offsetOfLine(text, line);
	const i = text.indexOf('\n', start);
	return i === -1 ? text.length : i;
}

/** The line (from 1) an offset in `text` is on. @param {string} text @param {number} offset */
export function lineAt(text, offset) {
	const end = Math.max(0, Math.min(offset, text.length));
	let n = 1;
	for (let i = text.indexOf('\n'); i !== -1 && i < end; i = text.indexOf('\n', i + 1)) n++;
	return n;
}

/** A size in bytes: "512 B", "1.2 KiB", "3.4 MiB". @param {number | null | undefined} bytes */
export function sizeText(bytes) {
	const T = S.admin.units;
	if (bytes == null || !Number.isFinite(bytes)) return '';
	if (bytes < 1024) return fill(T.b, bytes);
	if (bytes < 1024 * 1024) return fill(T.kib, (bytes / 1024).toFixed(1));
	return fill(T.mib, (bytes / (1024 * 1024)).toFixed(1));
}

/** A six-digit code in two groups, `123 456`, to read out; any other as it is. @param {string | null | undefined} code */
export function groupCode(code) {
	const s = String(code ?? '');
	return /^\d{6}$/.test(s) ? `${s.slice(0, 3)} ${s.slice(3)}` : s;
}

/**
 * The ids of the dashboards a draft's text has tables for (`[dashboards.mine]`,
 * `[[dashboards.mine.widgets]]`, `[dashboards."ups.eco"]`), in the order it has them, once each.
 * @param {string} text @returns {string[]}
 */
export function draftDashboards(text) {
	/** @type {string[]} */
	const out = [];
	const re = /^[ \t]*\[\[?[ \t]*dashboards[ \t]*\.[ \t]*("(?:[^"\\]|\\.)*"|[A-Za-z0-9_-]+)/gm;
	for (let m = re.exec(text); m; m = re.exec(text)) {
		let id = m[1];
		if (id.startsWith('"')) {
			try {
				id = JSON.parse(id);
			} catch {
				continue;
			}
		}
		if (!out.includes(id)) out.push(id);
	}
	return out;
}
