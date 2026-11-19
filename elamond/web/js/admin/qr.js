// A QR code as the service gives it (`qr.size` modules to a row, `qr.modules` row by row, `1`
// dark), as rows of booleans to draw; and how big a module is drawn.

/**
 * The rows of a QR code. Null when `modules` isn't a whole square of `size` × `size` 0s and 1s.
 * @param {{ size: number, modules: string } | null | undefined} qr
 * @returns {boolean[][] | null}
 */
export function qrMatrix(qr) {
	if (!qr || typeof qr !== 'object') return null;
	const { size, modules } = qr;
	if (!Number.isInteger(size) || size < 1 || typeof modules !== 'string') return null;
	if (modules.length !== size * size || /[^01]/.test(modules)) return null;
	/** @type {boolean[][]} */
	const rows = [];
	for (let y = 0; y < size; y++) {
		/** @type {boolean[]} */
		const row = [];
		for (let x = 0; x < size; x++) row.push(modules.charCodeAt(y * size + x) === 49);
		rows.push(row);
	}
	return rows;
}

/**
 * How many device pixels each module gets: a code of `size` modules with a quiet zone of `quiet`
 * on each side, in about `target` CSS pixels at `ratio` device pixels to one; at least 1, and
 * whole, so the modules stay sharp.
 * @param {number} size @param {number} quiet @param {number} target @param {number} ratio
 */
export function qrScale(size, quiet, target, ratio) {
	return Math.max(1, Math.floor((target * (ratio > 0 ? ratio : 1)) / (size + 2 * quiet)));
}
