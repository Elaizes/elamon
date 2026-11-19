import { test } from 'node:test';
import assert from 'node:assert/strict';
import { lineCount, offsetOfLine, endOfLine, lineAt, sizeText, fingerprint, groupCode, draftDashboards } from '../js/admin/text.js';
import { previewUi, dashboardChoices } from '../js/admin/preview.js';

test("a text's lines, as the editor numbers them", () => {
	const t = 'a\nbc\n\nd';
	assert.equal(lineCount(''), 1);
	assert.equal(lineCount('a\n'), 2);
	assert.equal(lineCount(t), 4);
	assert.equal(offsetOfLine(t, 1), 0);
	assert.equal(offsetOfLine(t, 2), 2);
	assert.equal(offsetOfLine(t, 3), 5);
	assert.equal(offsetOfLine(t, 4), 6);
	assert.equal(offsetOfLine(t, 99), 6);
	assert.equal(offsetOfLine(t, 0), 0);
	assert.equal(endOfLine(t, 2), 4);
	assert.equal(endOfLine(t, 4), 7);
	assert.equal(lineAt(t, 0), 1);
	assert.equal(lineAt(t, 1), 1);
	assert.equal(lineAt(t, 2), 2);
	assert.equal(lineAt(t, 5), 3);
	assert.equal(lineAt(t, 7), 4);
	assert.equal(lineAt(t, 100), 4);
	for (let l = 1; l <= 4; l++) assert.equal(lineAt(t, offsetOfLine(t, l)), l);
});

test('sizes, fingerprints, codes', () => {
	assert.equal(sizeText(512), '512 B');
	assert.equal(sizeText(1200), '1.2 KiB');
	assert.equal(sizeText(30210), '29.5 KiB');
	assert.equal(sizeText(3 * 1024 * 1024 + 1), '3.0 MiB');
	assert.equal(sizeText(null), '');
	assert.equal(fingerprint('1f0e8d2c'), '1F:0E:8D:2C');
	assert.equal(fingerprint('1f0e8d2c3b4a59687766554433221100ffeeddccbbaa99887766554433221100').split(':').length, 32);
	assert.equal(fingerprint('not hex'), 'not hex');
	assert.equal(fingerprint('abc'), 'abc');
	assert.equal(fingerprint(null), '');
	assert.equal(groupCode('123456'), '123 456');
	assert.equal(groupCode('12345'), '12345');
	assert.equal(groupCode(null), '');
});

test('the dashboards a draft has tables for', () => {
	const text = '[dashboards.mine]\ntitle = "Mine"\n[[dashboards.mine.widgets]]\ntype = "value"\n  [ dashboards . "ups.eco" ]\n[profiles.default]\n# [dashboards.commented]\nx = "[dashboards.inline]"\n[dashboards.cpu-2.extra]\n';
	assert.deepEqual(draftDashboards(text), ['mine', 'ups.eco', 'cpu-2']);
	assert.deepEqual(draftDashboards(''), []);
});

test("a preview's ui, filled in from the page's own; the dashboards it may show", () => {
	const own = /** @type {any} */ ({ service: { interval_ms: 1000 }, profile: { id: 'p' }, control: { enabled: true }, role: 'owner', tabs: ['overview', 'cpu'], dashboards: [{ id: 'cpu', title: 'CPU' }] });
	const answer = { version: 'v1', role: 'owner', tabs: ['overview', 'mine'], header: null, dashboards: [{ id: 'mine', title: 'Mine', widgets: [] }], actions: [] };
	const ui = previewUi(answer, own);
	assert.equal(ui.service, own.service);
	assert.equal(ui.profile, own.profile);
	assert.deepEqual(ui.tabs, ['overview', 'mine']);
	assert.equal(previewUi({ service: { interval_ms: 5 } }, own).service.interval_ms, 5);
	assert.deepEqual(previewUi(null, null).dashboards, []);
	assert.deepEqual(dashboardChoices(ui, ['[dashboards.extra]'], own), [
		{ id: 'overview', title: 'overview' },
		{ id: 'mine', title: 'Mine' },
		{ id: 'extra', title: 'extra' },
		{ id: 'cpu', title: 'CPU' },
	]);
	assert.deepEqual(dashboardChoices(null, ['[dashboards.extra]'], own)[0], { id: 'extra', title: 'extra' });
	assert.deepEqual(dashboardChoices(null, [], null), []);
});
