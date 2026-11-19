import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as L from '../js/logs.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/${name}`, import.meta.url), 'utf8'));
const texts = (l) => l.items.map((i) => ('line' in i ? i.line.text : `… ${i.more}`));

test('a reset replaces; later lines follow, "… n more" before those after a gap', () => {
	const first = fx('0.8/stream-sources.json').widgets['sources/app_log'];
	let l = L.take(L.empty(), first, 200);
	assert.equal(L.lineCount(l), 3);
	const next = fx('0.8/stream-log-next.json').widgets['journal/system'];
	l = L.take(l, next, 200);
	assert.deepEqual(texts(l).slice(3), ['… 12', next.lines[0].text, next.lines[1].text]);
	// Left out again with no line since: one "… n more" for both.
	l = L.take(l, { lines: [], more: 3 }, 200);
	l = L.take(l, { lines: [], more: 2 }, 200);
	assert.equal(texts(l).at(-1), '… 5');
	const again = L.take(l, { lines: [], reset: true }, 200);
	assert.equal(again.items.length, 0);
	assert.ok(again.generation > l.generation);
});

test('a note stays until a reset comes without one', () => {
	let l = L.take(L.empty(), { lines: [], reset: true, note: 'Only this user' }, 200);
	l = L.take(l, { lines: [] }, 200);
	assert.equal(l.note, 'Only this user');
	l = L.take(l, { lines: [], reset: true }, 200);
	assert.equal(l.note, null);
});

test('only the newest lines a widget keeps', () => {
	const lines = Array.from({ length: 15 }, (_, i) => ({ c: i, t: i, text: `l${i}` }));
	const l = L.take(L.empty(10), { lines, reset: true }, 10);
	assert.equal(L.lineCount(l), 10);
	assert.equal(L.oldest(l).text, 'l5');
	assert.equal(L.before(l), 5);
});

test('an older page goes before, without what is held already; its before is the next', () => {
	const page = fx('0.8/log.json');
	let l = L.take(L.empty(), { lines: [{ c: 1699999935250000, t: 1699999935250, text: 'held' }], reset: true }, 200);
	l = L.older(L.asking(l), page);
	assert.deepEqual(texts(l), [page.lines[0].text, page.lines[1].text, 'held']);
	assert.equal(l.cursor, page.before);
	assert.equal(L.before(l), page.before);
	assert.equal(L.canLoadOlder(l), true);
	const events = fx('0.9/log-events.json');
	const atStart = L.older(L.empty(), events);
	assert.equal(atStart.atStart, true);
	assert.equal(L.canLoadOlder(atStart), false);
	assert.equal(L.failed(L.asking(l)).olderFailed, true);
});

test('levels, tones and filters', () => {
	assert.deepEqual(L.levels('info'), ['debug', 'info', 'notice', 'warning', 'error']);
	assert.deepEqual(L.levels('critical').at(-1), 'critical');
	assert.equal(L.tone('critical'), 'error');
	assert.equal(L.tone(undefined), 'normal');
	assert.equal(L.filterFor('warning', 'warning', '  '), null);
	assert.deepEqual(L.filterFor('warning', 'error', ''), { level: 'error' });
	assert.deepEqual(L.filterFor('info', 'info', ' timeout '), { level: 'info', search: 'timeout' });
	assert.equal(L.filterFor('info', 'info', 'x'.repeat(150)).search.length, 100);
});
