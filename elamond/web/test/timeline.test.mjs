import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stretches, stretchAt, timeIn, shares, legendEntries, legendMode, splice } from '../js/timeline.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/${name}`, import.meta.url), 'utf8'));

const MIN = 60_000;

test('time in each state over the range shown', () => {
	const data = fx('0.7/stream-ups.json');
	const segs = data.widgets['ups.eco/power'].segments;
	const end = data.t;
	const start = end - 86_400_000;
	const totals = timeIn(segs, start, end);
	assert.equal([...totals.values()].reduce((a, b) => a + b, 0), end - start);
	assert.equal(totals.get('battery'), 1700000000123 - 1699999280000);
});

test('legend in durations, shares or both', () => {
	const totals = new Map([['line', 96 * MIN], ['battery', 4 * MIN], ['none', 20 * MIN]]);
	assert.equal(legendMode(undefined), 'duration');
	assert.equal(legendMode('nonsense'), 'duration');
	assert.deepEqual(legendEntries(totals, 'duration'), [
		{ state: 'line', seconds: 96 * 60, share: null },
		{ state: 'battery', seconds: 240, share: null },
		{ state: 'none', seconds: 1200, share: null },
	]);
	assert.deepEqual(legendEntries(totals, 'percent'), [
		{ state: 'line', seconds: null, share: 96 },
		{ state: 'battery', seconds: null, share: 4 },
		{ state: 'none', seconds: 1200, share: null },
	]);
	assert.deepEqual(legendEntries(totals, 'both')[1], { state: 'battery', seconds: 240, share: 4 });
});

test('a share above nothing but under 1% is <1%; each rounded on its own', () => {
	const s = shares(new Map([['line', 999_000], ['battery', 1_000]]));
	assert.equal(s.get('battery'), 'under');
	assert.equal(s.get('line'), 100);
	const third = shares(new Map([['line', 1], ['battery', 2]]));
	assert.equal(third.get('line'), 33);
	assert.equal(third.get('battery'), 67);
	assert.equal(shares(new Map([['none', 5]])).size, 0);
	// Nothing known at all: durations only.
	assert.deepEqual(legendEntries(new Map([['none', 60_000]]), 'percent'), [{ state: 'none', seconds: 60, share: null }]);
});

test('stretches: clipped to the bar, short ones widened, widened ones found first', () => {
	const segs = [
		{ from: 0, to: 1000, state: 'line' },
		{ from: 1000, to: 1001, state: 'battery' },
		{ from: 1001, to: 2000, state: 'line' },
		{ from: 3000, to: 4000, state: 'none' },
	];
	const all = stretches(segs, 0, 2000, 200, 4);
	assert.equal(all.length, 3);
	assert.equal(all[1].widened, true);
	assert.equal(all[1].x1 - all[1].x0, 4);
	assert.equal(all[1].segment.state, 'battery');
	assert.equal(stretchAt(all, 100), 1);
	assert.equal(stretchAt(all, 10), 0);
	assert.equal(stretchAt(all, 500), -1);
	assert.deepEqual(stretches(segs, 5, 5, 200, 4), []);
});

test('a long range carried on by the stream', () => {
	const hist = fx('0.7/history-timeline.json');
	const live = [
		{ from: hist.to - 10_000, to: hist.to + 5_000, state: 'battery' },
		{ from: hist.to + 5_000, to: hist.to + 9_000, state: 'line' },
	];
	const out = splice(hist.segments, hist.to, live);
	assert.equal(out.length, hist.segments.length + 1);
	assert.equal(out[out.length - 2].state, 'battery');
	assert.equal(out[out.length - 2].to, hist.to + 5_000);
	assert.deepEqual(out[out.length - 1], { from: hist.to + 5_000, to: hist.to + 9_000, state: 'line' });
	// The fixture is left as it was.
	assert.equal(hist.segments[hist.segments.length - 1].to, hist.to);
});
