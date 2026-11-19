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
	const { own, common } = keysOf(desc, { type: 'table' });
	assert.deepEqual(own.map((k) => k.key), ['source', 'columns', 'limit', 'collapsed']);
	assert.ok(common.some((k) => k.key === 'label'));
	assert.ok(!common.some((k) => k.key === 'collapsed'), 'a key is offered once');
	// A type the description doesn't have: the common keys, the rest as they are.
	const odd = { type: 'sparkle', label: 'x', glitter: 1 };
	assert.equal(typeOf(desc, 'sparkle'), null);
	assert.deepEqual(keysOf(desc, odd).own, []);
	assert.deepEqual(unknownKeys(desc, odd), ['glitter']);
	assert.equal(holdsWidgets(desc, { type: 'group' }), 'widgets');
	assert.equal(holdsWidgets(desc, { type: 'gauge' }), null);
	assert.deepEqual(spanRange(desc), { min: 1, max: 4 });
	assert.equal(nameOf(desc, { type: 'gauge' }), 'Gauge');
	assert.equal(nameOf(desc, { type: 'gauge', label: 'CPU' }), 'CPU');
	assert.equal(typeLabel(desc, 'toggle'), 'Switch');
	assert.equal(typeLabel(desc, 'sparkle'), 'sparkle');
});

test('an older or partial description still has every list', () => {
	const d = normalise({ hash: 'x' });
	assert.deepEqual(d.types, []);
	assert.deepEqual(d.dashboard.keys, []);
	assert.equal(d.writable, true);
	assert.deepEqual(normalise(null).profiles, []);
});

test('what a reference may name, by its `to`', () => {
	assert.deepEqual(refOptions(desc, 'action'), [
		{ value: 'power.lock', label: 'Lock', detail: 'power' },
		{ value: 'mode', label: 'Server mode', detail: 'actions' },
	]);
	assert.deepEqual(refOptions(desc, 'dashboard').map((o) => o.value), ['overview', 'mine']);
	assert.deepEqual(refOptions(desc, 'collection').map((o) => o.value), ['disks', 'processes']);
	assert.deepEqual(refOptions(desc, 'link'), [{ value: 'music', label: 'Music' }]);
	assert.deepEqual(refOptions(desc, 'source'), [{ value: 'ups', label: 'ups', detail: 'nut' }]);
	assert.deepEqual(refOptions(desc, 'rule'), []);
	assert.deepEqual(valueOptions(desc)[0], { value: 'cpu.usage', label: 'cpu.usage', detail: '12 %' });
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
	const [overview, mine] = desc.dashboards;
	assert.deepEqual(origins(desc, overview), [{ kind: 'file', file: 'elamond.d/dashboards.toml', line: 3 }]);
	assert.deepEqual(origins(desc, mine), [{ kind: 'editor', file: 'elamond.d/zz-editor.toml', line: 5 }]);
	assert.deepEqual(origins(desc, { from: [] }), [{ kind: 'defaults', file: '', line: null }]);
	assert.equal(origins(desc, { from: [{ file: '/nix/store/abc-elamond.toml', line: 9 }] })[0].kind, 'nix');
	const below = keysFromBelow(desc, overview);
	assert.deepEqual([...below.keys].sort(), ['icon', 'title']);
	assert.equal(below.where, 'elamond.d/dashboards.toml:3');
	assert.equal(below.sure, true);
	assert.equal(keysFromBelow(desc, mine).keys.size, 0, 'the editor\'s own: everything can go');
	const both = keysFromBelow(desc, { ...overview, editor: true, from: [...overview.from, { file: desc.file, line: 20 }] });
	assert.equal(both.sure, false, 'the editor overrides it: any key might be from below');
	assert.equal(both.where, 'elamond.d/dashboards.toml:3');
	assert.equal(keysFromBelow(desc, null).keys.size, 0);
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
	const preview = fx('admin-preview.json').response.ui.dashboards[0];
	// The fixture names a widget without an id `w<place>`; the settings' docs, `<place>`: either is matched.
	assert.deepEqual(matchDrawn(preview.widgets, 'mine/', [{ type: 'gauge' }]).map((l) => l.map((w) => w.id)), [['mine/w0']]);
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
