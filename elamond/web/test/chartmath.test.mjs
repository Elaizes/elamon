import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chartX, timeAtX, nearestIndex, firstVisibleIndex, niceCeiling, niceBinary, chartScale, timeTicks, axisLabels, tooltipLeft } from '../js/chartmath.js';

test('moments and x', () => {
	assert.equal(chartX(100, 100, 50, 200), 200);
	assert.equal(chartX(50, 100, 50, 200), 0);
	assert.equal(timeAtX(100, 100, 50, 200), 75);
});

test('nearest and first visible points', () => {
	const t = [0, 10, 20, 30];
	assert.equal(nearestIndex(t, 14), 1);
	assert.equal(nearestIndex(t, 16), 2);
	assert.equal(nearestIndex(t, 99), 3);
	assert.equal(nearestIndex([], 5), -1);
	assert.equal(firstVisibleIndex(t, 15), 1);
	assert.equal(firstVisibleIndex(t, -5), 0);
});

test('round scales', () => {
	assert.equal(niceCeiling(7), 10);
	assert.equal(niceCeiling(1.7), 2);
	assert.equal(niceCeiling(2.2), 2.5);
	assert.equal(niceCeiling(0), 1);
	assert.equal(niceBinary(700_000), 1_048_576);
	assert.equal(chartScale(50, 100, 1, false), 100);
	assert.equal(chartScale(80, null, 1, false), 100);
	assert.equal(chartScale(0, null, 65536, true), 65536);
	assert.equal(chartScale(0, 0, 1, false), 1);
});

test('time ticks at round moments, no more than asked', () => {
	process.env.TZ = 'UTC';
	const start = Date.UTC(2026, 10, 19, 10, 3);
	const end = start + 3_600_000;
	const ticks = timeTicks(start, end, 6);
	assert.equal(ticks.unit, 'time');
	assert.deepEqual(ticks.times.map((t) => new Date(t).getUTCMinutes()), [10, 20, 30, 40, 50, 0]);
	const week = timeTicks(Date.UTC(2026, 10, 1), Date.UTC(2026, 10, 30), 5);
	assert.equal(week.unit, 'day');
	assert.ok(week.times.every((t) => new Date(t).getUTCDay() === 1));
	assert.deepEqual(timeTicks(5, 5, 4).times, []);
	assert.deepEqual(timeTicks(0, 10, 0).times, []);
});

test('axis labels that would touch are left out', () => {
	assert.deepEqual(axisLabels([10, 20, 100], [30, 30, 30], 200, 4), [[0, 0], [2, 85]]);
	assert.equal(tooltipLeft(5, 40, 200), 0);
	assert.equal(tooltipLeft(199, 40, 200), 160);
});
