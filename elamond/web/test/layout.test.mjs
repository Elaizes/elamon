import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { columns, span, place } from '../js/layout.js';

const ui = JSON.parse(readFileSync(new URL('../../fixtures/ui.json', import.meta.url), 'utf8'));

/** A row: its columns, and its cells `[index, columns taken]`. */
const line = (cols, ...cells) => ({ cells, columns: cols });

test('columns: four, two below 600 px, one below 320 px', () => {
	assert.equal(columns(1280), 4);
	assert.equal(columns(600), 4);
	assert.equal(columns(599), 2);
	assert.equal(columns(320), 2);
	assert.equal(columns(319), 1);
	assert.equal(columns(0), 1);
});

test('columns: eight on a wide grid, as the pick says; narrow, two or one whatever it says', () => {
	// Auto: eight from 1500 px of grid on.
	assert.equal(columns(1499, 'auto'), 4);
	assert.equal(columns(1500, 'auto'), 8);
	assert.equal(columns(2400), 8);
	// Eight where each column is 150 px at least; four never more.
	assert.equal(columns(1199, '8'), 4);
	assert.equal(columns(1200, '8'), 8);
	assert.equal(columns(2400, '4'), 4);
	for (const pick of ['auto', '4', '8']) {
		assert.equal(columns(599, pick), 2);
		assert.equal(columns(360, pick), 2);
		assert.equal(columns(300, pick), 1);
	}
});

test('a span as it is on four columns and more, halved rounding up on fewer', () => {
	assert.equal(span(1, 4), 1);
	assert.equal(span(3, 4), 3);
	assert.equal(span(1, 2), 1);
	assert.equal(span(2, 2), 1);
	assert.equal(span(3, 2), 2);
	assert.equal(span(4, 2), 2);
	assert.equal(span(4, 1), 1);
	assert.equal(span(9, 4), 4);
	assert.equal(span(0, 4), 1);
	// Not widened on eight: 4 is half a row.
	assert.equal(span(1, 8), 1);
	assert.equal(span(4, 8), 4);
	assert.equal(span(9, 8), 4);
});

test('rows: one that does not fit starts a row', () => {
	// 2 + 2 | 4 | 1 + 1 + 2 | 3 + 1 | 4, as the app's DashboardsTest
	assert.deepEqual(place([2, 2, 4, 1, 1, 2, 3, 1, 4], 4), [
		line(4, [0, 2], [1, 2]),
		line(4, [2, 4]),
		line(4, [3, 1], [4, 1], [5, 2]),
		line(4, [6, 3], [7, 1]),
		line(4, [8, 4]),
	]);
	assert.deepEqual(place([], 4), []);
});

test('rows: one left incomplete is shared evenly, wherever it is, on any columns', () => {
	assert.deepEqual(place([3, 2], 4), [line(1, [0, 1]), line(1, [1, 1])]);
	assert.deepEqual(place([1, 1], 4), [line(2, [0, 1], [1, 1])]);
	assert.deepEqual(place([1, 2], 4), [line(2, [0, 1], [1, 1])]);
	assert.deepEqual(place([1, 1, 1], 4), [line(3, [0, 1], [1, 1], [2, 1])]);
	assert.deepEqual(place([2, 4], 4), [line(1, [0, 1]), line(4, [1, 4])]);
	assert.deepEqual(place([1, 4], 2), [line(1, [0, 1]), line(2, [1, 2])]);
	// Eight: four of 2 fill a row, 4 + 4 too; 4 + 2 before a 4 is shared.
	assert.deepEqual(place([2, 2, 2, 2, 4, 4], 8), [line(8, [0, 2], [1, 2], [2, 2], [3, 2]), line(8, [4, 4], [5, 4])]);
	assert.deepEqual(place([4, 2, 4, 4], 8), [line(2, [0, 1], [1, 1]), line(8, [2, 4], [3, 4])]);
	assert.deepEqual(place([1, 1, 1, 1, 1], 8), [line(5, [0, 1], [1, 1], [2, 1], [3, 1], [4, 1])]);
});

test('every list of spans: each widget once, in order, every row from edge to edge', () => {
	const lists = (n) => (n === 0 ? [[]] : lists(n - 1).flatMap((l) => [1, 2, 3, 4].map((s) => [...l, s])));
	for (const cols of [1, 2, 4, 8]) {
		for (let n = 0; n <= 5; n++) {
			for (const spans of lists(n)) {
				const rows = place(spans, cols);
				assert.deepEqual(rows.flatMap((r) => r.cells.map(([i]) => i)), spans.map((_, i) => i), `${spans} in ${cols}`);
				rows.forEach((row, k) => {
					assert.equal(row.cells.reduce((a, [, s]) => a + s, 0), row.columns, `${spans} in ${cols}`);
					const asked = row.cells.map(([i]) => span(spans[i], cols));
					if (asked.reduce((a, b) => a + b, 0) === cols) {
						assert.equal(row.columns, cols);
						assert.deepEqual(row.cells.map(([, s]) => s), asked);
					} else {
						assert.deepEqual(row.cells.map(([, s]) => s), row.cells.map(() => 1));
						const next = rows[k + 1];
						if (next) assert.ok(asked.reduce((a, b) => a + b, 0) + span(spans[next.cells[0][0]], cols) > cols, `${spans} in ${cols}`);
					}
				});
			}
		}
	}
});

test('the fixture home dashboard: each row fills its columns', () => {
	const home = ui.dashboards.find((d) => d.id === 'home');
	const spans = home.widgets.map((w) => w.span);
	for (const cols of [8, 4, 2, 1]) {
		const rows = place(spans, cols);
		for (const row of rows) assert.equal(row.cells.reduce((a, [, s]) => a + s, 0), row.columns);
		assert.equal(rows.flatMap((r) => r.cells).length, spans.length);
	}
	// Gauge, meter (2 + 2); value, status; graph, value; …
	assert.deepEqual(place(spans, 4).slice(0, 3), [line(4, [0, 2], [1, 2]), line(4, [2, 2], [3, 2]), line(4, [4, 2], [5, 2])]);
});
