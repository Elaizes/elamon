// A role in words: its name, and with what it allows where the admin panel offers it. A role of a
// newer service's, which the page has no words for, by its id. Pure.
import { S, fill } from './strings.js';

/** A role's name: "Admin". @param {string | null | undefined} role */
export function roleName(role) {
	const r = String(role ?? '');
	const names = /** @type {Record<string, string>} */ (S.roles.names);
	return Object.hasOwn(names, r) ? names[r] : r.charAt(0).toUpperCase() + r.slice(1);
}

/** A role as a choice: its name and what it allows ("Monitor: everything there is to read; runs nothing"). @param {string | null | undefined} role */
export function roleChoice(role) {
	const r = String(role ?? '');
	const about = /** @type {Record<string, string>} */ (S.roles.about);
	return Object.hasOwn(about, r) ? fill(S.roles.with, roleName(r), about[r]) : roleName(r);
}
