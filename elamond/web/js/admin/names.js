// Settings files' names: a new drop-in's, as the protocol takes it (`elamond.d/<name>.toml`, the
// name of letters, digits, `_`, `.` and `-`, starting with a letter or a digit, at most 64
// characters before `.toml`), and a file's place in the page's address.
import { DROPIN_STEM_MAX } from '../consts.js';

export const DROPIN_DIR = 'elamond.d/';
export const SUFFIX = '.toml';

/**
 * A drop-in's name from what was typed: `mine`, `mine.toml` and `elamond.d/mine.toml` all name
 * `elamond.d/mine.toml`. Else why not: `empty`, `long`, `start` (not a letter or a digit first) or
 * `chars` (anything but letters, digits, `_`, `.` and `-`).
 * @param {unknown} input @returns {{ name: string, error?: undefined } | { error: 'empty' | 'long' | 'start' | 'chars', name?: undefined }}
 */
export function dropinName(input) {
	let s = String(input ?? '').trim();
	if (s.startsWith(DROPIN_DIR)) s = s.slice(DROPIN_DIR.length);
	if (s.endsWith(SUFFIX)) s = s.slice(0, -SUFFIX.length);
	if (!s) return { error: 'empty' };
	if (s.length > DROPIN_STEM_MAX) return { error: 'long' };
	if (!/^[A-Za-z0-9]/.test(s)) return { error: 'start' };
	if (!/^[A-Za-z0-9_.-]+$/.test(s)) return { error: 'chars' };
	return { name: `${DROPIN_DIR}${s}${SUFFIX}` };
}

/** Whether a file's name is a drop-in's, as the protocol writes it. @param {string} name */
export function isDropin(name) {
	return name.startsWith(DROPIN_DIR) && dropinName(name).name === name;
}

/** A drop-in's name without its folder and `.toml`: what a rename starts from. @param {string} name */
export function dropinStem(name) {
	let s = name.startsWith(DROPIN_DIR) ? name.slice(DROPIN_DIR.length) : name;
	if (s.endsWith(SUFFIX)) s = s.slice(0, -SUFFIX.length);
	return s;
}

/**
 * The page's address of a file in the files editor, at a line if one is given.
 * @param {string} name @param {number | null} [line]
 */
export function fileHref(name, line) {
	const at = line && line > 0 ? `/${Math.floor(line)}` : '';
	return `#/admin/files/${encodeURIComponent(name)}${at}`;
}

/**
 * The file and line an address names: what follows `#/admin/files/`, its parts decoded. The name
 * may come as one part (as `fileHref` writes it) or, typed by hand, as several; a last part of
 * digits alone is the line.
 * @param {string[]} parts @returns {{ name: string | null, line: number | null }}
 */
export function fileRoute(parts) {
	const p = parts.filter((x) => x !== '');
	/** @type {number | null} */
	let line = null;
	if (p.length > 1 && /^\d+$/.test(p[p.length - 1])) line = Number(p.pop()) || null;
	return { name: p.length ? p.join('/') : null, line };
}
