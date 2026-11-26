import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Hist, rangeChips, spanParts, MAX_POINTS } from '../js/history.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/${name}`, import.meta.url), 'utf8'));

test('the first data line replaces what there was with its history, its own point after', () => {
	const data = fx('stream-data.json');
	const h = new Hist(2, 300_000);
	h.append(1, [9, 9]);
	h.take(data.t, data.widgets['home/net'].v, data.history['home/net']);
	assert.deepEqual(h.t, [1699999999123, 1700000000123]);
	assert.deepEqual(h.v, [[100.5, 200.5], [10.5, 20.5]]);
	// A later line's point is appended; a repeat is skipped.
	h.take(data.t + 1000, [1500.5, 300.5], null);
	h.take(data.t + 1000, [1, 1], null);
	assert.equal(h.size, 3);
	assert.deepEqual(h.v[0], [100.5, 200.5, 1500.5]);
});

test('a spark: null where a value was missing', () => {
	const data = fx('stream-data.json');
	const h = new Hist(1, 600_000);
	h.take(data.t, [data.widgets['home/temp'].v], data.history['home/temp']);
	assert.deepEqual(h.v[0], [50.0, null]);
});

test('what falls out of the window goes, one point before it kept', () => {
	const h = new Hist(1, 10_000);
	for (let t = 0; t <= 30_000; t += 1000) h.append(t, [t]);
	assert.equal(h.t[0], 20_000);
	assert.equal(h.lastTime, 30_000);
	assert.ok(h.t[1] > h.lastTime - h.windowMs);
	h.setWindow(20_000);
	assert.equal(h.t[0], 20_000);
});

test('a clock turned back past the window starts over; a little back is a repeat', () => {
	const h = new Hist(1, 10_000);
	h.append(100_000, [1]);
	h.append(101_000, [2]);
	assert.equal(h.append(95_000, [3]), false);
	assert.equal(h.size, 2);
	assert.equal(h.append(50_000, [4]), true);
	assert.deepEqual(h.t, [50_000]);
});

test('a long range takes a stream point once a bucket has passed', () => {
	const hist = fx('0.7/history.json');
	const h = new Hist(2, hist.to - hist.from);
	h.replace(hist);
	assert.equal(h.size, 6);
	assert.deepEqual(h.v[1], [205.1, 207.6, 203.9, null, 210.4, 208.0]);
	assert.equal(h.appendEvery(hist.t[5] + 1000, [1, 2], hist.step_ms), false);
	assert.equal(h.appendEvery(hist.t[5] + hist.step_ms, [1, 2], hist.step_ms), true);
	assert.equal(h.size, 7);
});

test('memory guard and gaps', () => {
	const h = new Hist(1, 1e12);
	for (let i = 0; i < MAX_POINTS + 10; i++) h.append(i * 1000, [i]);
	assert.equal(h.size, MAX_POINTS);
	const gaps = h.gapsMs();
	assert.equal(gaps.length, h.size);
	assert.equal(gaps[0], Infinity);
	assert.ok(gaps.slice(1).every((g) => g === 3000));
});

test('the history\'s two steps and the stream\'s are no gaps, however many of each (G107); a hole is', () => {
	// The service's history: a point each 10 s, a 2-minute hole, then its last minutes a point a second; then the stream's.
	const t = [];
	for (let x = 0; x < 300_000; x += 10_000) if (x < 100_000 || x >= 220_000) t.push(x);
	for (let x = 300_000; x <= 600_000; x += 1000) t.push(x);
	const h = new Hist(1, 900_000, 1000);
	h.replace({ t, v: [t.map(() => 1)] });
	for (let x = 601_000; x <= 900_000; x += 1000) h.append(x, [2]);
	const gaps = h.gapsMs();
	const breaks = [];
	for (let i = 1; i < h.size; i++) if (h.t[i] - h.t[i - 1] > gaps[i]) breaks.push([h.t[i - 1], h.t[i]]);
	assert.deepEqual(breaks, [[90_000, 220_000]]);
	// A hole among the stream's points, and one point alone between two holes.
	const live = new Hist(1, 600_000, 1000);
	for (const x of [0, 1000, 2000, 3000, 63_000, 64_000, 65_000, 200_000, 400_000, 401_000, 402_000]) live.append(x, [1]);
	const g = live.gapsMs();
	const cut = [];
	for (let i = 1; i < live.size; i++) if (live.t[i] - live.t[i - 1] > g[i]) cut.push(live.t[i]);
	assert.deepEqual(cut, [63_000, 200_000, 400_000]);
});

test('range chips: its own and the long ones, shortest first, none as long as its own', () => {
	const all = ['1h', '24h', '7d', '30d', '1y', 'all'];
	const chips = rangeChips(600_000, all);
	assert.deepEqual(chips.map((c) => c.range && c.range.key), [null, '1h', '24h', '7d', '30d', '1y', 'all']);
	assert.deepEqual(rangeChips(86_400_000, all).map((c) => c.range && c.range.key), ['1h', null, '7d', '30d', '1y', 'all']);
	assert.deepEqual(rangeChips(600_000, undefined), []);
	assert.deepEqual(rangeChips(600_000, ['nope']), []);
});

test('a span as a chip says it', () => {
	assert.deepEqual(spanParts(30_000), ['s', 30]);
	assert.deepEqual(spanParts(600_000), ['min', 10]);
	assert.deepEqual(spanParts(86_400_000), ['h', 24]);
	assert.deepEqual(spanParts(7 * 86_400_000), ['d', 7]);
});
