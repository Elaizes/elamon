// The page's own state, kept in the browser: which tab, the theme, what is folded. Never the token
// (the service keeps that in a cookie the page can't read), never anything of the computer's.

const PREFIX = 'elamon.';

/** @param {string} key @param {any} fallback */
export function load(key, fallback) {
	try {
		const raw = localStorage.getItem(PREFIX + key);
		return raw == null ? fallback : JSON.parse(raw);
	} catch {
		return fallback;
	}
}

/** @param {string} key @param {any} value */
export function save(key, value) {
	try {
		if (value === undefined) localStorage.removeItem(PREFIX + key);
		else localStorage.setItem(PREFIX + key, JSON.stringify(value));
	} catch {
		// Private windows may refuse; the page works without.
	}
}
