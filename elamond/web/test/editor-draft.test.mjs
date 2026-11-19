// The editor's drafts: widgets added, moved, changed and taken out, profiles' tabs, what changed,
// and that a key no form knows goes back to the service as it came.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
	draft, isDirty, rebase, clone, stable, setKey, listAt, widgetAt, addWidget, removeWidget, moveWidget, nudge,
	setSpan, allPaths, addTab, changes, writeInTurn, isId,
} from '../js/editor/draft.js';
import { placedSpans } from '../js/layout.js';
import { normalise, exampleOf, unknownKeys, spanOf, isGroup } from '../js/editor/describe.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/0.11/${name}`, import.meta.url), 'utf8'));
const desc = normalise(fx('editor.json'));
const labels = (list) => list.map((w) => w.label);

/** A dashboard with a widget of a key no form knows, one of a type the service didn't describe, and a group. */
function sample() {
	return {
		title: 'Mine',
		icon: 'gauge',
		someday: { kept: true },
		widgets: [
			{ type: 'gauge', label: 'A', value: 'cpu.usage', glow: { radius: 3, colours: ['cpu', 'hot'] } },
			{ type: 'sparkle', label: 'B', glitter: [1, 2, 3] },
			{ type: 'group', label: 'G', widgets: [{ type: 'value', label: 'G0' }, { type: 'value', label: 'G1' }] },
			{ type: 'text', label: 'C', text: 'Up {system.uptime}' },
		],
	};
}

test('a widget is added from its type\'s example, as the description has it', () => {
	const d = draft({ title: 'Mine', widgets: [] });
	for (const t of desc.types) {
		const w = exampleOf(desc, t.type);
		assert.deepEqual(w, { type: t.type, ...t.example }, t.type);
		addWidget(d.value, [], 1e9, w);
	}
	assert.equal(d.value.widgets.length, desc.types.length);
	assert.equal(d.value.widgets[0].type, desc.types[0].type);
	// The example is a copy: changing the draft leaves the description as it was.
	const first = desc.types[0];
	widgetAt(d.value, [0]).label = 'Changed';
	assert.deepEqual(exampleOf(desc, first.type), { type: first.type, ...first.example });
	// A group's example comes with its widgets, and is opened out on the canvas.
	const group = desc.types.find((t) => t.keys.some((k) => k.kind === 'widgets'));
	assert.ok(group, 'the description has a type that holds widgets');
	const at = addWidget(d.value, [], 0, exampleOf(desc, group.type));
	assert.ok(isGroup(desc, widgetAt(d.value, at)));
	assert.deepEqual(widgetAt(d.value, [0, 0]), group.example.widgets[0]);
	// A type the service didn't describe still makes a widget of just its type.
	assert.deepEqual(exampleOf(desc, 'nothing-known'), { type: 'nothing-known' });
});

test('every example\'s keys are the description\'s, or kept as they are', () => {
	for (const t of desc.types) {
		const w = exampleOf(desc, t.type);
		const unknown = unknownKeys(desc, w);
		assert.ok(!unknown.includes('type'), 'type is never one of the other keys');
		for (const k of unknown) assert.deepEqual(w[k], t.example[k], `${t.type}.${k} kept`);
	}
	const w = { ...exampleOf(desc, 'gauge'), glow: 3 };
	assert.deepEqual(unknownKeys(desc, w), ['glow']);
});

test('moved up, down, to a place, into a group and out of it', () => {
	const d = sample();
	assert.deepEqual(nudge(d, [1], -1), [0]);
	assert.deepEqual(labels(d.widgets), ['B', 'A', 'G', 'C']);
	assert.deepEqual(nudge(d, [0], 1), [1]);
	assert.deepEqual(labels(d.widgets), ['A', 'B', 'G', 'C']);
	assert.deepEqual(nudge(d, [0], -1), [0], 'the first stays first');
	assert.deepEqual(nudge(d, [3], 1), [3], 'the last stays last');
	// To a place: the index counts the list as it was before the move.
	assert.deepEqual(moveWidget(d, [0], [], 3), [2]);
	assert.deepEqual(labels(d.widgets), ['B', 'G', 'A', 'C']);
	// Into the group (which moves up one place once B is out of the way).
	assert.deepEqual(moveWidget(d, [0], [1], 1), [0, 1]);
	assert.deepEqual(labels(d.widgets), ['G', 'A', 'C']);
	assert.deepEqual(labels(d.widgets[0].widgets), ['G0', 'B', 'G1']);
	// Out of it past its ends.
	assert.deepEqual(nudge(d, [0, 0], -1), [0]);
	assert.deepEqual(labels(d.widgets), ['G0', 'G', 'A', 'C']);
	assert.deepEqual(nudge(d, [1, 1], 1), [2]);
	assert.deepEqual(labels(d.widgets), ['G0', 'G', 'G1', 'A', 'C']);
	assert.deepEqual(labels(d.widgets[1].widgets), ['B']);
	// Out of the group to a place before it.
	assert.deepEqual(moveWidget(d, [1, 0], [], 0), [0]);
	assert.deepEqual(labels(d.widgets), ['B', 'G0', 'G', 'G1', 'A', 'C']);
	assert.deepEqual(d.widgets[2].widgets, []);
	// Into an empty group, and a group never into itself.
	assert.deepEqual(moveWidget(d, [5], [2], 0), [2, 0]);
	assert.equal(moveWidget(d, [2], [2], 0), null);
	assert.equal(moveWidget(d, [2], [2, 0], 0), null);
	assert.equal(moveWidget(d, [9], [], 0), null, 'nothing there to move');
});

test('taken out, and the paths of every widget', () => {
	const d = sample();
	assert.deepEqual(allPaths(d), [[0], [1], [2], [2, 0], [2, 1], [3]]);
	assert.equal(removeWidget(d, [2, 0]).label, 'G0');
	assert.equal(removeWidget(d, [7]), null);
	assert.deepEqual(labels(d.widgets[2].widgets), ['G1']);
	assert.equal(listAt(d, [0]), null, 'a gauge holds no widgets');
	assert.equal(widgetAt(d, []), null);
});

test('a widget with keys no form knows survives being added, moved, changed and sent', () => {
	const d = draft(sample());
	const v = d.value;
	addWidget(v, [], 0, exampleOf(desc, 'graph'));
	assert.deepEqual(moveWidget(v, [1], [3], 1), [2, 1]); // A into the group
	assert.deepEqual(moveWidget(v, [2, 1], [], 0), [0]); // and out to the top
	assert.deepEqual(nudge(v, [0], 1), [1]);
	assert.deepEqual(labels(v.widgets), [exampleOf(desc, 'graph').label, 'A', 'B', 'G', 'C']);
	setKey(v.widgets[0], 'label', 'Graph');
	setSpan(v.widgets[1], 3);
	const sparkle = v.widgets.find((w) => w.type === 'sparkle');
	setKey(sparkle, 'label', 'Still sparkling');
	setSpan(sparkle, 9);
	const sent = JSON.parse(JSON.stringify({ id: 'mine', dashboard: v, base: desc.hash }));
	assert.deepEqual(Object.keys(sent).sort(), ['base', 'dashboard', 'id']);
	const a = sent.dashboard.widgets.find((w) => w.label === 'A');
	assert.deepEqual(a.glow, { radius: 3, colours: ['cpu', 'hot'] });
	assert.equal(a.span, 3);
	const s = sent.dashboard.widgets.find((w) => w.type === 'sparkle');
	assert.deepEqual(s.glitter, [1, 2, 3]);
	assert.equal(s.span, 4, 'the width is clamped to the grid');
	assert.deepEqual(sent.dashboard.someday, { kept: true });
	assert.deepEqual(sent.dashboard.widgets.find((w) => w.type === 'graph'), { ...exampleOf(desc, 'graph'), label: 'Graph' });
	assert.equal(sent.dashboard.widgets[0].label, 'Graph');
	assert.deepEqual(unknownKeys(desc, a), ['glow']);
	assert.deepEqual(unknownKeys(desc, s), ['glitter']);
	assert.ok(isDirty(d));
});

test('what changed: a change undone is no change, and the order of keys doesn\'t count', () => {
	const d = draft({ title: 'Mine', icon: 'gauge', widgets: [{ type: 'value', label: 'A' }] });
	assert.equal(isDirty(d), false);
	d.value.title = 'Other';
	assert.equal(isDirty(d), true);
	d.value.title = 'Mine';
	assert.equal(isDirty(d), false);
	d.value = { widgets: [{ label: 'A', type: 'value' }], icon: 'gauge', title: 'Mine' };
	assert.equal(isDirty(d), false);
	setKey(d.value, 'icon', undefined);
	assert.equal('icon' in d.value, false);
	assert.equal(isDirty(d), true);
	rebase(d);
	assert.equal(isDirty(d), false);
	assert.equal(stable({ b: 1, a: { d: 1, c: 2 } }), '{"a":{"c":2,"d":1},"b":1}');
	assert.deepEqual(clone({ a: [1, { b: 2 }] }), { a: [1, { b: 2 }] });
});

test('widths: the widget\'s own, else its type\'s, clamped', () => {
	for (const t of desc.types) assert.equal(spanOf(desc, { type: t.type }), t.span, t.type);
	assert.equal(spanOf(desc, { type: 'graph', span: 1 }), 1);
	assert.equal(spanOf(desc, { type: 'gauge', span: 7 }), 4);
	assert.equal(spanOf(desc, { type: 'nothing-known' }), 4);
	const w = { type: 'value' };
	assert.equal(setSpan(w, 0), 1);
	assert.equal(setSpan(w, 2.6), 3);
});

test('the grid: rows filled, the last of a row taking the columns left', () => {
	assert.deepEqual(placedSpans([2, 2, 2, 2], 4), [2, 2, 2, 2]);
	assert.deepEqual(placedSpans([2, 1], 4), [2, 2]);
	assert.deepEqual(placedSpans([2, 2, 2], 4), [2, 2, 4]);
	assert.deepEqual(placedSpans([1, 1, 1, 4, 3, 3], 4), [1, 1, 2, 4, 4, 4]);
	assert.deepEqual(placedSpans([2, 1, 1, 3], 2), [1, 1, 2, 2]);
	assert.deepEqual(placedSpans([4, 1], 2), [2, 2]);
	assert.deepEqual(placedSpans([1, 2, 3], 1), [1, 1, 1]);
	assert.deepEqual(placedSpans([], 4), []);
});

test("a profile's write holds what was changed here, never what the files set", () => {
	// The fixture's request, from a profile whose tabs and tiles were changed in the forms.
	const want = fx('editor-profile.json').request.profile;
	const p = draft({ dashboards: ['overview'], header: 'header', android: { widget: 'phone', tiles: '', card: '' } });
	p.value.dashboards.push('mine');
	p.value.android.tiles = 'tiles';
	assert.equal(isDirty(p), true);
	assert.deepEqual(changes(p.start, p.value), { dashboards: want.dashboards, android: { tiles: want.android.tiles } });
	// "Add it to my tabs": the tabs alone.
	const own = desc.profiles.find((x) => x.id === desc.profile) || desc.profiles[0];
	const q = draft(own.def);
	assert.equal(addTab(q.value, 'mine'), true);
	assert.equal(addTab(q.value, 'mine'), false);
	assert.deepEqual(changes(q.start, q.value), { dashboards: [...own.def.dashboards, 'mine'] });
	// Put back as it was, it isn't written: the editor's file keeps what it has.
	q.value.dashboards.pop();
	assert.deepEqual(changes(q.start, q.value), {});
	// A key taken out is null: back to what the other files say; of the app's, each key.
	delete q.value.header;
	assert.deepEqual(changes(q.start, q.value), { header: null });
	delete p.value.android.widget;
	assert.deepEqual(changes(p.start, p.value).android, { widget: null, tiles: 'tiles' });
	delete p.value.android;
	assert.deepEqual(changes(p.start, p.value).android, { widget: null, tiles: null, card: null });
	p.value.android = { widget: 'phone', tiles: 'tiles', card: '' };
	// After a save, what was written is what the next change is measured from.
	p.value.header = 'mine';
	rebase(p);
	assert.deepEqual(changes(p.start, p.value), {});
	const empty = {};
	addTab(empty, 'mine');
	assert.deepEqual(empty, { dashboards: ['mine'] });
});

test('ids as the settings take them', () => {
	for (const ok of ['mine', 'a', '2', 'cpu_2', 'x-y', 'a_', 'b-', '9lives', 'a'.repeat(32)]) assert.equal(isId(ok), true, ok);
	for (const bad of ['', 'Mine', 'a.b', 'a b', 'a'.repeat(33), '_a', '-a', '_', '-', 3, null]) assert.equal(isId(bad), false, String(bad));
});

test('writes go one after another, each on the hash the one before left, and the next save on the last', async () => {
	// The editor's file as the service keeps it: a write on another hash than its own is 409.
	let file = 'h0';
	let generation = 3;
	const service = (/** @type {boolean} */ bad) => async (/** @type {string | null} */ base) => {
		if (base !== file) throw Object.assign(new Error('changed_on_disk'), { code: 'changed_on_disk' });
		if (bad) throw Object.assign(new Error('bad_settings'), { code: 'bad_settings' });
		file = `h${++generation}`;
		return { ok: true, hash: file, backup: null, generation };
	};
	// The dashboard goes, the profile is refused for its mistakes.
	const first = await writeInTurn([service(false), service(true)], 'h0');
	assert.equal(first.done, 1);
	assert.equal(first.error.code, 'bad_settings');
	assert.equal(first.hash, file, 'the hash the dashboard left');
	// The profile put right and saved again: on that hash, no false "changed meanwhile".
	const again = await writeInTurn([service(false)], first.hash);
	assert.equal(again.error, null);
	assert.equal(again.done, 1);
	assert.equal(again.generation, generation);
	// On the hash before the first write, it would have been refused.
	const stale = await writeInTurn([service(false)], 'h0');
	assert.equal(stale.error.code, 'changed_on_disk');
	assert.equal(stale.hash, 'h0');
	assert.deepEqual(await writeInTurn([], 'h9'), { hash: 'h9', generation: null, done: 0, error: null });
});
