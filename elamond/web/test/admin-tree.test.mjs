import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildTree, filterTree, lookup, valueText, pathKey, countNodes, countHits } from '../js/admin/tree.js';

const fixture = JSON.parse(readFileSync(new URL('../../fixtures/0.11/admin-settings.json', import.meta.url), 'utf8'));

/** @param {any[]} nodes @param {string} path */
function find(nodes, path) {
	for (const n of nodes) {
		if (n.path === path) return n;
		const f = find(n.children, path);
		if (f) return f;
	}
	return null;
}

test("the fixture's settings, each part with its file and line", () => {
	const tree = buildTree(fixture.settings, fixture.whence, fixture.files);
	assert.deepEqual(
		tree.map((n) => [n.key, n.kind]),
		[
			['schema', 'value'],
			['port', 'value'],
			['control', 'value'],
			['dashboards', 'table'],
			['profiles', 'table'],
		],
	);
	assert.deepEqual(find(tree, 'port'), { key: 'port', path: 'port', kind: 'value', text: '47823', size: 0, where: { file: 'elamond.toml', line: 2 }, children: [] });
	assert.deepEqual(find(tree, 'dashboards.mine.title').where, { file: 'elamond.toml', line: 6 });
	const widgets = find(tree, 'dashboards.mine.widgets');
	assert.equal(widgets.kind, 'list');
	assert.equal(widgets.size, 1);
	assert.deepEqual(find(tree, 'dashboards.mine.widgets[0]').where, { file: 'elamond.toml', line: 7 });
	// Deeper than three parts: no line of its own (its part above has it).
	const type = find(tree, 'dashboards.mine.widgets[0].type');
	assert.equal(type.text, '"value"');
	assert.equal(type.where, null);
	// A short list of plain values stays on one line.
	const tabs = find(tree, 'profiles.default.dashboards');
	assert.equal(tabs.kind, 'value');
	assert.equal(tabs.text, '["mine"]');
	assert.deepEqual(tabs.where, { file: 'elamond.toml', line: 10 });
	assert.equal(countNodes(tree), 15);
});

test('values as the settings write them', () => {
	assert.equal(valueText('a "b"'), '"a \\"b\\""');
	assert.equal(valueText(3.5), '3.5');
	assert.equal(valueText(true), 'true');
	assert.equal(valueText(null), 'null');
	assert.equal(valueText([1, 'x']), '[1, "x"]');
	assert.equal(valueText({ a: 1, 'b c': [] }), '{ a = 1, "b c" = [] }');
	assert.equal(valueText({}), '{}');
	assert.equal(pathKey('cpu_2-x'), 'cpu_2-x');
	assert.equal(pathKey('ups.eco'), '"ups.eco"');
});

test('a key with a dot is found quoted or as it is; a file out of range is none', () => {
	const files = ['a.toml', 'b.toml'];
	assert.deepEqual(lookup({ 'dashboards."ups.eco"': [1, 3] }, files, 'dashboards."ups.eco"', 'dashboards.ups.eco'), { file: 'b.toml', line: 3 });
	assert.deepEqual(lookup({ 'dashboards.ups.eco': [0, 9] }, files, 'dashboards."ups.eco"', 'dashboards.ups.eco'), { file: 'a.toml', line: 9 });
	assert.equal(lookup({ x: [5, 1] }, files, 'x'), null);
	assert.equal(lookup({ x: /** @type {any} */ ('bad') }, files, 'x'), null);
	assert.equal(lookup({}, files, 'toString'), null);
	const tree = buildTree({ dashboards: { 'ups.eco': { title: 'Eco' } } }, { 'dashboards.ups.eco': [0, 4] }, files);
	assert.equal(find(tree, 'dashboards."ups.eco"').where.line, 4);
});

test('a long list of values, or one of tables, folds', () => {
	const long = Array.from({ length: 30 }, (_, i) => `dashboard-${i}`);
	const tree = buildTree({ tabs: long, rows: [{ a: 1 }, { a: 2 }] }, {}, []);
	assert.equal(tree[0].kind, 'list');
	assert.equal(tree[0].children[29].path, 'tabs[29]');
	assert.equal(tree[1].kind, 'list');
	assert.equal(tree[1].children[1].children[0].path, 'rows[1].a');
	assert.deepEqual(buildTree(null, null, null), []);
	assert.deepEqual(buildTree([1, 2], {}, []), []);
});

test('filtering keeps what matches whole, and the parts above it open', () => {
	const tree = buildTree(fixture.settings, fixture.whence, fixture.files);
	assert.equal(filterTree(tree, '  '), tree);
	const byTitle = filterTree(tree, 'TITLE');
	assert.deepEqual(byTitle.map((n) => [n.path, n.hit]), [['dashboards', false]]);
	assert.deepEqual(byTitle[0].children.map((n) => [n.path, n.hit]), [['dashboards.mine', false]]);
	assert.deepEqual(byTitle[0].children[0].children.map((n) => [n.path, n.hit]), [['dashboards.mine.title', true]]);
	assert.equal(countHits(byTitle), 1);
	// Every top-level part has a line of its own: each matches, the tables kept whole.
	assert.equal(countHits(filterTree(tree, 'elamond.toml:')), 5);
	// By value.
	const byValue = filterTree(tree, 'system.uptime');
	assert.equal(find(byValue, 'dashboards.mine.widgets[0].value').hit, true);
	// A table that matches is kept whole.
	const whole = filterTree(tree, 'profiles');
	assert.equal(whole.length, 1);
	assert.equal(whole[0].hit, true);
	assert.equal(whole[0].children.length, 1);
	// By file:line.
	assert.deepEqual(filterTree(tree, 'elamond.toml:2').map((n) => n.path), ['port']);
	assert.deepEqual(filterTree(tree, 'nothing like it'), []);
});
