// The dashboards' grid, as the app lays it out (data/Dashboards.kt, Grid): 4 columns, 2 on a
// narrow screen, 1 on a very narrow one; each widget as wide as its span (from `ui`), halved
// rounding up where there are half as many columns; one that doesn't fit beside the one before
// starts a row, and the last of a row takes the columns left free, so no row ends in a gap. Pure.

import { COLUMNS, NARROW_PX, ONE_COLUMN_PX, SPAN_MIN } from './consts.js';

/** The columns for a grid `width` px wide. @param {number} width */
export function columns(width) {
	if (width < ONE_COLUMN_PX) return 1;
	if (width < NARROW_PX) return COLUMNS / 2;
	return COLUMNS;
}

/** A widget's `span` in a grid of `cols`: halved, rounding up, where there are half as many. @param {number} span @param {number} cols */
export function span(span, cols) {
	const s = Math.min(Math.max(Math.trunc(Number(span) || 0), SPAN_MIN), COLUMNS);
	return Math.min(Math.max(Math.trunc((s * cols + COLUMNS - 1) / COLUMNS), 1), cols);
}

/**
 * The widgets of `spans`, in order, row by row: each cell `[index into spans, span]`, the last of
 * a row widened to the columns left free.
 * @param {number[]} spans @param {number} cols @returns {[number, number][][]}
 */
export function place(spans, cols) {
	/** @type {[number, number][][]} */
	const rows = [];
	let used = cols;
	spans.forEach((raw, i) => {
		const s = span(raw, cols);
		if (used + s > cols) {
			rows.push([]);
			used = 0;
		}
		rows[rows.length - 1].push([i, s]);
		used += s;
	});
	for (const row of rows) {
		const free = cols - row.reduce((a, [, s]) => a + s, 0);
		if (free > 0) row[row.length - 1] = [row[row.length - 1][0], row[row.length - 1][1] + free];
	}
	return rows;
}

/**
 * Each widget's span as placed, in order: what a CSS grid of `cols` columns needs to lay the rows
 * out as `place` does (each row filling the columns exactly).
 * @param {number[]} spans @param {number} cols @returns {number[]}
 */
export function placedSpans(spans, cols) {
	const out = new Array(spans.length).fill(1);
	for (const row of place(spans, cols)) for (const [i, s] of row) out[i] = s;
	return out;
}
