// The layout from what a browser kept: each pick that is one of its choice's values, the rest what
// is there until picked; boot.js, which sets it before the first frame, reads it the same way. And
// the room the editor's sizes go by.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { layoutOf, PICKS, DEFAULTS, KEYS } from '../js/arrange.js';
import { roomOf } from '../js/shell.js';

const BOOT = readFileSync(new URL('../js/boot.js', import.meta.url), 'utf8');

/** What boot.js puts on html for what localStorage keeps. @param {Record<string, string>} kept */
function boot(kept) {
	const dataset = {};
	const ctx = {
		localStorage: { getItem: (k) => (k in kept ? kept[k] : null) },
		document: { documentElement: { dataset }, querySelector: () => null, getElementById: () => null },
		matchMedia: () => ({ matches: false }),
		getComputedStyle: () => ({ getPropertyValue: () => '' }),
		window: { addEventListener() {} },
		HTMLScriptElement: class {},
	};
	vm.runInNewContext(BOOT, ctx);
	return dataset;
}

test('until picked: the whole window, Auto columns, normal density and edges', () => {
	assert.deepEqual(DEFAULTS, { width: 'full', columns: 'auto', density: 'normal', inset: 'normal' });
	for (const v of [null, undefined, 'full', 7, [], ['fixed']]) assert.deepEqual(layoutOf(v), DEFAULTS);
	assert.deepEqual(layoutOf({}), DEFAULTS);
});

test('each pick that is a value of its choice is kept; the rest as they are until picked', () => {
	assert.deepEqual(layoutOf({ width: 'fixed', columns: '8', density: 'compact', inset: 'none' }), { width: 'fixed', columns: '8', density: 'compact', inset: 'none' });
	assert.deepEqual(layoutOf({ density: 'roomy' }), { ...DEFAULTS, density: 'roomy' });
	assert.deepEqual(layoutOf({ density: 'huge', columns: 8, inset: 'large', theme: 'dark' }), { ...DEFAULTS, inset: 'large' });
	for (const k of KEYS) for (const v of PICKS[k]) assert.equal(layoutOf({ [k]: v })[k], v);
});

test('boot.js reads what was kept as layoutOf does, and the side bar folded or not', () => {
	const cases = [
		null,
		{ width: 'fixed', columns: '4', density: 'roomy', inset: 'small' },
		{ density: 'huge', columns: 8, inset: 'large' },
		{ width: 'full' },
		'roomy',
		[1, 2],
	];
	for (const c of cases) {
		const kept = c === null ? {} : { 'elamon.layout': JSON.stringify(c) };
		const got = boot(kept);
		const want = layoutOf(c);
		for (const k of KEYS) assert.equal(got[k], want[k], `${JSON.stringify(c)}: ${k}`);
		assert.equal(got.side, 'open');
	}
	assert.equal(boot({ 'elamon.side': '"rail"' }).side, 'rail');
	assert.equal(boot({ 'elamon.side': '"wide"' }).side, 'open');
	assert.equal(boot({ 'elamon.layout': '{not json' }).density, 'normal');
});

test('the room beside the side bar, as the editor goes by it', () => {
	assert.equal(roomOf(1400), '');
	assert.equal(roomOf(901), '');
	assert.equal(roomOf(900), 'sheet');
	assert.equal(roomOf(601), 'sheet');
	assert.equal(roomOf(600), 'sheet two');
	assert.equal(roomOf(360), 'sheet two');
	assert.equal(roomOf(320), 'sheet two one');
});
