// The dashboards' grid, as the app lays it out (data/Dashboards.kt, Grid), widened for a wide
// window: 4 columns, 8 where the grid is wide enough (the layout's pick, js/look.js), 2 on a narrow
// screen, 1 on a very narrow one. A widget is as wide as its span (from `ui`, in columns of four):
// as it is on 4 columns and more (on 8, span 4 is half a row), halved rounding up on 2. One that
// doesn't fit beside the one before starts a row; a row the widgets fill has the grid's columns,
// one they leave incomplete is shared evenly by them (`Grid.place`), so every row runs from edge to
// edge. Pure.

import { COLUMNS, WIDE_COLUMNS, NARROW_PX, ONE_COLUMN_PX, COLUMN_MIN_PX, AUTO_WIDE_PX, SPAN_MIN } from './consts.js';

/** @typedef {'auto' | '4' | '8'} ColumnsPick */

/**
 * The columns for a grid `width` px wide, as `pick` asks: `auto` 8 from AUTO_WIDE_PX on, else 4;
 * `8` 8 where each column is at least COLUMN_MIN_PX; `4` never more. Narrow, 2 or 1 whatever the pick.
 * @param {number} width @param {ColumnsPick} [pick]
 */
export function columns(width, pick = 'auto') {
	if (width < ONE_COLUMN_PX) return 1;
	if (width < NARROW_PX) return COLUMNS / 2;
	if (pick === '4') return COLUMNS;
	if (pick === '8') return width >= WIDE_COLUMNS * COLUMN_MIN_PX ? WIDE_COLUMNS : COLUMNS;
	return width >= AUTO_WIDE_PX ? WIDE_COLUMNS : COLUMNS;
}

/** A widget's `span` in a grid of `cols`: as it is where there are 4 or more, halved rounding up where fewer. @param {number} span @param {number} cols */
export function span(span, cols) {
	const s = Math.min(Math.max(Math.trunc(Number(span) || 0), SPAN_MIN), COLUMNS);
	if (cols >= COLUMNS) return s;
	return Math.min(Math.max(Math.trunc((s * cols + COLUMNS - 1) / COLUMNS), 1), cols);
}

/**
 * A row of widgets: each cell `[index into spans, columns it takes]` of the row's `columns` — the
 * grid's where the widgets fill them, one each where they leave the row incomplete.
 * @typedef {{ cells: [number, number][], columns: number }} Line
 */

/**
 * The widgets of `spans`, in order, row by row.
 * @param {number[]} spans @param {number} cols @returns {Line[]}
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
	return rows.map((row) =>
		row.reduce((a, [, s]) => a + s, 0) === cols ? { cells: row, columns: cols } : { cells: row.map(([i]) => /** @type {[number, number]} */ ([i, 1])), columns: row.length },
	);
}

/**
 * Each widget's width in `total` equal tracks across its row, as `place` lays the rows out: for a
 * single CSS grid of `total` tracks (the editor's canvas), where `total` is a multiple of every
 * row's columns (12 for 4 columns: a row shared by 1, 2 or 3; 2 for 2).
 * @param {number[]} spans @param {number} cols @param {number} total @returns {number[]}
 */
export function tracks(spans, cols, total) {
	const out = new Array(spans.length).fill(total);
	for (const line of place(spans, cols)) for (const [i, s] of line.cells) out[i] = (s * total) / line.columns;
	return out;
}
