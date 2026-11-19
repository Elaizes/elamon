import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pairCode, hashParts, once, generations } from '../js/route.js';

test("the address's parts, a stray % kept as it is rather than a blank page", () => {
	assert.deepEqual(hashParts('#/tab/cpu'), ['tab', 'cpu']);
	assert.deepEqual(hashParts('#/admin/files/elamond.d%2Fmine.toml/12'), ['admin', 'files', 'elamond.d/mine.toml', '12']);
	assert.deepEqual(hashParts('#/tab/%E0%A4%A'), ['tab', '%E0%A4%A']);
	assert.deepEqual(hashParts('#/tab/100%'), ['tab', '100%']);
	assert.deepEqual(hashParts(''), []);
	assert.deepEqual(hashParts('#/'), []);
});

test('a pairing code in the address', () => {
	assert.equal(pairCode('#pair=123456'), '123456');
	assert.equal(pairCode('#pair=12345'), null);
	assert.equal(pairCode('#/tab/cpu'), null);
});

test('a teardown runs once', () => {
	let n = 0;
	const t = once(() => n++);
	t();
	t();
	assert.equal(n, 1);
});

test('a route that waited knows a newer one started', () => {
	const g = generations();
	const a = g.next();
	assert.equal(g.stale(a), false);
	const b = g.next();
	assert.equal(g.stale(a), true);
	assert.equal(g.stale(b), false);
});
