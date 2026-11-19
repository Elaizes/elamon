// The description the editor's forms come from, and the service's mistakes put back where they go.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { locate, pathParts, sortMistakes } from '../js/editor/draft.js';
import {
	normalise, typeOf, keysOf, unknownKeys, holdsWidgets, spanRange, refOptions, valueOptions, search, anyOf, whichOf, fits,
	origins, keysFromBelow, matchDrawn, nameOf, typeLabel,
} from '../js/editor/describe.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/0.11/${name}`, import.meta.url), 'utf8'));
const desc = normalise(fx('editor.json'));

test('a widget takes its type\'s keys, then the common ones its type doesn\'t have', () => {
	const table = typeOf(desc, 'table');
	const { own, common } = keysOf(desc, { type: 'table' });
	assert.deepEqual(own.map((k) => k.key), table.keys.map((k) => k.key));
	assert.ok(common.some((k) => k.key === 'label'));
	for (const k of common) assert.ok(!own.some((o) => o.key === k.key), `${k.key} is offered once`);
	// A type the description doesn't have: the common keys, the rest as they are.
	const odd = { type: 'sparkle', label: 'x', glitter: 1 };
	assert.equal(typeOf(desc, 'sparkle'), null);
	assert.deepEqual(keysOf(desc, odd).own, []);
	assert.deepEqual(unknownKeys(desc, odd), ['glitter']);
	assert.equal(holdsWidgets(desc, { type: 'group' }), 'widgets');
	assert.equal(holdsWidgets(desc, { type: 'gauge' }), null);
	assert.deepEqual(spanRange(desc), { min: 1, max: 4 });
	assert.equal(nameOf(desc, { type: 'gauge' }), typeOf(desc, 'gauge').label);
	assert.equal(nameOf(desc, { type: 'gauge', label: 'CPU' }), 'CPU');
	assert.equal(typeLabel(desc, 'toggle'), typeOf(desc, 'toggle').label);
	assert.equal(typeLabel(desc, 'sparkle'), 'sparkle');
});

test('the description: every key has a kind, and every record and reference it names is there', () => {
	const refs = new Set(['action', 'dashboard', 'collection', 'link', 'source']);
	/** @param {any} k @param {string} where */
	const check = (k, where) => {
		assert.equal(typeof k.kind, 'string', where);
		if (k.kind === 'record') assert.ok(Array.isArray(desc.records[k.record]), `${where}: record ${k.record}`);
		if (k.kind === 'ref') assert.ok(refs.has(k.to), `${where}: ref to ${k.to}`);
		if (k.kind === 'enum') assert.ok(Array.isArray(k.choices) && k.choices.length, `${where}: choices`);
		for (const o of anyOf(k)) if (k.kind === 'any' || k.kind === 'list' || k.kind === 'map') check(o, `${where} of`);
	};
	for (const t of desc.types) for (const k of t.keys) check(k, `${t.type}.${k.key}`);
	for (const k of desc.common) check(k, `common.${k.key}`);
	for (const k of desc.dashboard.keys) check(k, `dashboard.${k.key}`);
	for (const [name, keys] of Object.entries(desc.records)) for (const k of keys) check(k, `${name}.${k.key}`);
});

test('an older or partial description still has every list', () => {
	const d = normalise({ hash: 'x' });
	assert.deepEqual(d.types, []);
	assert.deepEqual(d.dashboard.keys, []);
	assert.equal(d.writable, true);
	assert.deepEqual(normalise(null).profiles, []);
});

test('what a reference may name, by its `to`', () => {
	assert.deepEqual(refOptions(desc, 'action'), desc.actions.map((a) => ({ value: a.id, label: a.name || a.id, detail: a.kind })));
	assert.deepEqual(refOptions(desc, 'dashboard').map((o) => o.value), desc.dashboards.map((d) => d.id));
	assert.deepEqual(refOptions(desc, 'collection').map((o) => o.value), desc.collections.map((c) => c.name));
	assert.deepEqual(refOptions(desc, 'link').map((o) => o.value), desc.links.map((l) => l.id));
	assert.deepEqual(refOptions(desc, 'source').map((o) => o.value), desc.sources.map((x) => x.id));
	assert.deepEqual(refOptions(desc, 'rule'), []);
	const v = desc.values[0];
	assert.deepEqual(valueOptions(desc)[0], { value: v.address, label: v.address, detail: v.now == null ? '' : String(v.now) });
	assert.deepEqual(refOptions(normalise({ actions: [{ id: 'mode', name: 'Server mode', kind: 'actions' }] }), 'action'), [{ value: 'mode', label: 'Server mode', detail: 'actions' }]);
});

test('a search finds what starts with it first', () => {
	const opts = [{ value: 'memory.used' }, { value: 'cpu.usage' }, { value: 'gpu.usage', detail: 'cpu-ish' }];
	assert.deepEqual(search(opts, 'usage').map((o) => o.value), ['cpu.usage', 'gpu.usage']);
	assert.deepEqual(search(opts, 'cpu').map((o) => o.value), ['cpu.usage', 'gpu.usage']);
	assert.deepEqual(search(opts, '').length, 3);
	assert.deepEqual(search(opts, 'nothing'), []);
});

test('an `any` key: which of its shapes a value has', () => {
	const max = typeOf(desc, 'gauge').keys.find((k) => k.key === 'max');
	assert.deepEqual(anyOf(max).map((k) => k.kind), ['number', 'address']);
	assert.equal(whichOf(max, 100), 0);
	assert.equal(whichOf(max, 'cpu.usage'), 1);
	assert.equal(whichOf(max, undefined), -1);
	assert.equal(whichOf(max, [1]), -1);
	const each = desc.common.find((k) => k.key === 'for_each');
	assert.equal(whichOf(each, 'disks'), 0);
	assert.equal(whichOf(each, ['a', 'b']), 1);
	assert.equal(fits({ kind: 'int' }, 1.5), false);
	assert.equal(fits({ kind: 'enum', choices: ['a'] }, 'b'), false);
	assert.equal(fits({ kind: 'scalar' }, true), true);
	assert.equal(fits({ kind: 'sparkle' }, 1), false);
});

test('where a dashboard comes from, and which of its keys can\'t be taken out', () => {
	const file = desc.dashboards.find((d) => !d.editor && d.from && d.from.length);
	assert.ok(file, 'a dashboard from the files');
	const last = file.from[file.from.length - 1];
	assert.deepEqual(origins(desc, file).at(-1), { kind: 'file', file: last.file, line: last.line });
	assert.deepEqual(origins(desc, { from: [{ file: desc.file, line: 5 }] }), [{ kind: 'editor', file: desc.file, line: 5 }]);
	assert.deepEqual(origins(desc, { from: [] }), [{ kind: 'defaults', file: '', line: null }]);
	// The Nix store as the description marks it, not by a guess at the path.
	assert.equal(origins(desc, { from: [{ file: 'elamond.toml', line: 9, nix: true }] })[0].kind, 'nix');
	assert.equal(origins(desc, file).some((o) => o.kind === 'nix'), file.from.some((w) => w.nix === true));
	// The keys below, as `below` says, each with its file and line.
	const below = keysFromBelow(file);
	assert.deepEqual([...below.keys()].sort(), Object.keys(file.below).sort());
	const title = file.below.title;
	assert.equal(below.get('title'), `${title.file}:${title.line}`);
	const own = { id: 'mine', def: { title: 'Mine', icon: 'gauge' }, from: [{ file: desc.file, line: 5 }], below: {}, editor: true, own: true };
	assert.equal(keysFromBelow(own).size, 0, "the editor's own: everything can go");
	assert.equal(keysFromBelow({ ...file, below: undefined }).size, 0, 'nothing said, nothing held');
	assert.equal(keysFromBelow(null).size, 0);
});

test('a mistake\'s path names the widget and key', () => {
	assert.deepEqual(pathParts('dashboards.mine.widgets[1].widgets[0].max'), ['dashboards', 'mine', 'widgets', 1, 'widgets', 0, 'max']);
	assert.deepEqual(pathParts('dashboards.mine.widgets[0].map."On battery"'), ['dashboards', 'mine', 'widgets', 0, 'map', 'On battery']);
	assert.deepEqual(locate('dashboards.mine.widgets[1].widgets[0].max'), { section: 'dashboards', id: 'mine', widget: [1, 0], key: 'max', rest: '' });
	assert.deepEqual(locate('dashboards.mine.widgets[2].columns[0].field'), { section: 'dashboards', id: 'mine', widget: [2], key: 'columns', rest: '[0].field' });
	assert.deepEqual(locate('dashboards.mine.widgets[3]'), { section: 'dashboards', id: 'mine', widget: [3], key: null, rest: '' });
	assert.deepEqual(locate('dashboards.mine.command'), { section: 'dashboards', id: 'mine', widget: null, key: 'command', rest: '' });
	assert.deepEqual(locate('profiles.pixel-9.android.widget'), { section: 'profiles', id: 'pixel-9', widget: null, key: 'android', rest: 'widget' });
	assert.deepEqual(locate(''), { section: '', id: null, widget: null, key: null, rest: '' });
});

test('the service\'s mistakes, sorted to the widgets, keys and profile they name', () => {
	const save = fx('admin-save.json').bad_settings.body.mistakes;
	const cmd = fx('editor-dashboard.json').a_command.response.mistakes;
	const all = [
		...save,
		...cmd,
		{ path: 'dashboards.mine.widgets[1].widgets[0].max', message: 'too much' },
		{ path: 'dashboards.mine.widgets[2].columns[0].field', message: 'no such field' },
		{ path: 'profiles.pixel-9.dashboards[2]', message: 'no dashboard nope' },
		{ path: 'profiles.pixel-9.android.tiles', message: 'no dashboard nope' },
		{ path: 'dashboards.other.title', message: 'elsewhere' },
		{ message: 'no path' },
	];
	const m = sortMistakes(all, 'mine', 'pixel-9');
	assert.deepEqual(m.widgets.get('0').get('max'), [save[0].message]);
	assert.deepEqual(m.widgets.get('1/0').get('max'), ['too much']);
	assert.deepEqual(m.widgets.get('2').get('columns'), ['[0].field: no such field']);
	assert.deepEqual(m.dash.get('command'), [cmd[0].message]);
	assert.deepEqual(m.prof.get('dashboards'), ['[2]: no dashboard nope']);
	assert.deepEqual(m.prof.get('android.tiles'), ['no dashboard nope']);
	assert.deepEqual(m.other, ['dashboards.other.title: elsewhere', 'no path']);
});

test('the preview\'s widgets matched to the draft\'s', () => {
	const preview = fx('admin-preview.json').response.ui.dashboards.find((d) => d.id === 'draft');
	// A widget without an id is named by its place, `<dashboard>/<place>` (and `w<place>` is matched too).
	assert.deepEqual(matchDrawn(preview.widgets, 'draft/', [{ type: 'gauge' }]).map((l) => l.map((w) => w.id)), [['draft/0']]);
	assert.deepEqual(matchDrawn([{ id: 'mine/w0' }], 'mine/', [{ type: 'gauge' }]).map((l) => l.map((w) => w.id)), [['mine/w0']]);
	const drawn = [
		{ id: 'home/cpu' },
		{ id: 'home/gpus.nvidia' },
		{ id: 'home/gpus.intel' },
		{ id: 'home/2' },
		{ id: 'home/power', widgets: [{ id: 'home/power/reboot' }] },
		{ id: 'elsewhere/cpu' },
	];
	const list = [{ id: 'cpu' }, { id: 'gpus' }, { type: 'value' }, { id: 'hidden' }, { id: 'power' }];
	const got = matchDrawn(drawn, 'home/', list).map((l) => l.map((w) => w.id));
	assert.deepEqual(got, [['home/cpu'], ['home/gpus.nvidia', 'home/gpus.intel'], ['home/2'], [], ['home/power']]);
	assert.deepEqual(matchDrawn(drawn[4].widgets, 'home/power/', [{ id: 'reboot' }]).map((l) => l.length), [1]);
});
