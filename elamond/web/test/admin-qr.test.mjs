import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { qrMatrix, qrScale } from '../js/admin/qr.js';

const fixture = JSON.parse(readFileSync(new URL('../../fixtures/0.11/admin-pairing.json', import.meta.url), 'utf8'));

test('modules row by row, 1 dark', () => {
	assert.deepEqual(qrMatrix({ size: 2, modules: '1001' }), [
		[true, false],
		[false, true],
	]);
	assert.deepEqual(qrMatrix({ size: 3, modules: '111000010' }), [
		[true, true, true],
		[false, false, false],
		[false, true, false],
	]);
});

test("the fixture's codes are whole squares", () => {
	for (const part of [fixture.pending.app, fixture.pending.web]) {
		const m = qrMatrix(part.qr);
		assert.ok(m);
		assert.equal(m.length, part.qr.size);
		for (const row of m) assert.equal(row.length, part.qr.size);
		assert.equal(m.flat().filter(Boolean).length, [...part.qr.modules].filter((c) => c === '1').length);
	}
});

test('anything but a whole square of 0s and 1s is refused', () => {
	assert.equal(qrMatrix(null), null);
	assert.equal(qrMatrix(undefined), null);
	assert.equal(qrMatrix({ size: 2, modules: '100' }), null);
	assert.equal(qrMatrix({ size: 2, modules: '10010' }), null);
	assert.equal(qrMatrix({ size: 2, modules: '1021' }), null);
	assert.equal(qrMatrix({ size: 0, modules: '' }), null);
	assert.equal(qrMatrix({ size: 1.5, modules: '1' }), null);
	assert.equal(qrMatrix(/** @type {any} */ ({ size: '2', modules: '1001' })), null);
	assert.equal(qrMatrix(/** @type {any} */ ({ size: 1, modules: 1 })), null);
});

test('a module is a whole number of device pixels, at least 1', () => {
	// 21 modules and 4 on each side: 29 across, in 240 CSS pixels.
	assert.equal(qrScale(21, 4, 240, 1), 8);
	assert.equal(qrScale(21, 4, 240, 2), 16);
	assert.equal(qrScale(41, 4, 240, 1), 4);
	assert.equal(qrScale(177, 4, 100, 1), 1);
	assert.equal(qrScale(21, 4, 240, 0), 8);
});
