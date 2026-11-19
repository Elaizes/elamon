// What the page's address asks for (`#/tab/cpu`, `#/admin/files/…`, `#pair=123456`), and keeping
// routes that overlap apart: one that waited draws nothing once a newer one started. Pure.

/** A pairing code in the address (`#pair=123456`), or null. @param {string} hash */
export function pairCode(hash) {
	const m = /^#pair=(\d{6})$/.exec(hash);
	return m ? m[1] : null;
}

/** A part of the address decoded; one that isn't sound percent-encoding (a stray `%`) as it is. @param {string} part */
function decodePart(part) {
	try {
		return decodeURIComponent(part);
	} catch {
		return part;
	}
}

/** The address's parts after `#/`, each decoded. @param {string} hash @returns {string[]} */
export function hashParts(hash) {
	return hash.replace(/^#\/?/, '').split('/').filter(Boolean).map(decodePart);
}

/** `fn`, run the first time it is called and never again. @param {() => void} fn @returns {() => void} */
export function once(fn) {
	let done = false;
	return () => {
		if (done) return;
		done = true;
		fn();
	};
}

/**
 * Numbers runs of something that waits (a route): `next()` starts one and gives its number,
 * `stale(n)` says a newer one started since.
 */
export function generations() {
	let n = 0;
	return {
		next: () => ++n,
		/** @param {number} g */
		stale: (g) => g !== n,
	};
}
