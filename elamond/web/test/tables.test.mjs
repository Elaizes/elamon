import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { searchFields, searchIn, sortRows, barFractions, fillParams, rowButtons, linkUrl, sameValue } from '../js/tables.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/${name}`, import.meta.url), 'utf8'));
const ui = fx('ui.json');
const widget = (id) => ui.dashboards.flatMap((d) => d.widgets).find((w) => w.id === id);

const rows = [
	{ key: 'a', f: { name: 'Firefox', cpu: 3.5, cmd: '/usr/bin/firefox' } },
	{ key: 'b', f: { name: 'bash', cpu: 12, cmd: '-bash' } },
	{ key: 'c', f: { name: 'cron' } },
	{ key: 'd', f: { name: 'Xorg', cpu: 3.5, cmd: '/usr/bin/Xorg' } },
];

test('search: the fields the service names, any case', () => {
	const procs = widget('detail/procs');
	assert.deepEqual(searchFields(procs), ['name', 'cmd']);
	assert.deepEqual(searchFields(widget('home/links')), ['name', 'up']);
	assert.deepEqual(searchIn(rows, ['name', 'cmd'], 'USR/BIN').map((r) => r.key), ['a', 'd']);
	assert.equal(searchIn(rows, ['name'], '  ').length, 4);
});

test('order: numbers as numbers, words as words, the missing last, stable', () => {
	assert.deepEqual(sortRows(rows, 'cpu', true).map((r) => r.key), ['b', 'a', 'd', 'c']);
	assert.deepEqual(sortRows(rows, 'cpu', false).map((r) => r.key), ['a', 'd', 'b', 'c']);
	assert.deepEqual(sortRows(rows, 'name', false).map((r) => r.key), ['b', 'c', 'a', 'd']);
});

test('bars: full at a number, a field, or a quarter above the largest', () => {
	assert.deepEqual(barFractions([{ key: 'x', f: { p: 50 } }], { field: 'p', bar: { max: 100 } }), [0.5]);
	assert.deepEqual(barFractions([{ key: 'x', f: { p: 50, crit: 200 } }], { field: 'p', bar: { max: 'crit' } }), [0.25]);
	assert.deepEqual(barFractions([{ key: 'x', f: { p: 50 } }, { key: 'y', f: { p: 100 } }], { field: 'p', bar: { max: 'top' } }), [0.4, 0.8]);
	assert.deepEqual(barFractions([{ key: 'x', f: {} }], { field: 'p', bar: { max: 100 } }), [null]);
	assert.deepEqual(barFractions([{ key: 'x', f: { p: 500 } }], { field: 'p', bar: { max: 100 } }), [1]);
});

test('row actions: their parameters from the row, as the values they are', () => {
	const procs = widget('detail/procs');
	const row = fx('stream-data-detail.json').widgets['detail/procs'].rows[0];
	const ra = procs.row_actions[0];
	assert.deepEqual(fillParams(ra.params, row.f), { pid: 4242, signal: 'term', start: 1700000100 });
	assert.deepEqual(fillParams({ pid: '{pid}', x: '{nope}' }, { pid: 1 }), { pid: 1 });
	const buttons = rowButtons(ui, procs, row);
	assert.equal(buttons.length, 1);
	assert.equal(buttons[0].runs.id, 'process.signal');
});

test('a row action that opens is offered where its dashboard is', () => {
	const sources = fx('0.8/ui-sources.json');
	const links = sources.dashboards.flatMap((d) => d.widgets).find((w) => (w.row_actions || []).some((r) => r.open));
	assert.ok(links);
	const opened = rowButtons(sources, links, { key: 'example' });
	assert.equal(opened.find((b) => b.opens).opens, 'unit_log.example');
	assert.equal(rowButtons(sources, links, { key: 'missing' }).filter((b) => b.opens).length, 0);
});

test('a link at the name this page reaches the computer by', () => {
	assert.equal(linkUrl('http://{host}:8080', '192.0.2.10'), 'http://192.0.2.10:8080');
	assert.equal(linkUrl('localhost:8096/web', 'desk.example.net'), 'http://desk.example.net:8096/web');
	assert.equal(linkUrl('https://media.example.net/?h={host}', 'desk.example.net'), 'https://media.example.net/?h=desk.example.net');
	assert.equal(linkUrl('http://127.0.0.1:9000', '2001:db8::1'), 'http://[2001:db8::1]:9000');
	assert.equal(linkUrl('http://user@{host}:80', '192.0.2.10'), 'http://user@192.0.2.10:80');
});

test('same values', () => {
	assert.equal(sameValue(true, true), true);
	assert.equal(sameValue('on', 'on'), true);
	assert.equal(sameValue(1, '1'), false);
	assert.equal(sameValue(null, false), false);
});
