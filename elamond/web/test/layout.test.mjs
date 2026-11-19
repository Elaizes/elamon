import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { columns, span, place, placedSpans } from '../js/layout.js';

const ui = JSON.parse(readFileSync(new URL('../../fixtures/ui.json', import.meta.url), 'utf8'));

test('four columns, two below 600 px, one below 320 px', () => {
	assert.equal(columns(1280), 4);
	assert.equal(columns(600), 4);
	assert.equal(columns(599), 2);
	assert.equal(columns(320), 2);
	assert.equal(columns(319), 1);
	assert.equal(columns(0), 1);
});

test('a span halved, rounding up, where there are half as many columns', () => {
	assert.equal(span(1, 4), 1);
	assert.equal(span(3, 4), 3);
	assert.equal(span(1, 2), 1);
	assert.equal(span(2, 2), 1);
	assert.equal(span(3, 2), 2);
	assert.equal(span(4, 2), 2);
	assert.equal(span(4, 1), 1);
	assert.equal(span(9, 4), 4);
	assert.equal(span(0, 4), 1);
});

test('rows: one that does not fit starts a row, the last of a row takes what is left', () => {
	assert.deepEqual(place([2, 2, 2], 4), [[[0, 2], [1, 2]], [[2, 4]]]);
	assert.deepEqual(place([1, 1, 3], 4), [[[0, 1], [1, 3]], [[2, 4]]]);
	assert.deepEqual(place([4, 1, 1, 1, 1], 4), [[[0, 4]], [[1, 1], [2, 1], [3, 1], [4, 1]]]);
	assert.deepEqual(place([2, 2, 2], 2), [[[0, 1], [1, 1]], [[2, 2]]]);
	assert.deepEqual(place([], 4), []);
});

test('the fixture home dashboard: each row fills the columns', () => {
	const home = ui.dashboards.find((d) => d.id === 'home');
	const spans = home.widgets.map((w) => w.span);
	for (const cols of [4, 2, 1]) {
		for (const row of place(spans, cols)) assert.equal(row.reduce((a, [, s]) => a + s, 0), cols);
		assert.equal(placedSpans(spans, cols).length, spans.length);
	}
	// Gauge, meter (2 + 2); value, status; graph, value; bars 4 …
	assert.deepEqual(placedSpans(spans, 4).slice(0, 7), [2, 2, 2, 2, 2, 2, 4]);
	assert.deepEqual(placedSpans(spans, 2).slice(0, 7), [1, 1, 1, 1, 1, 1, 2]);
});
