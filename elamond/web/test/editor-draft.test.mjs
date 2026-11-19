// The editor's drafts: widgets added, moved, changed and taken out, profiles' tabs, what changed,
// and that a key no form knows goes back to the service as it came.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
	draft, isDirty, rebase, clone, stable, setKey, listAt, widgetAt, addWidget, removeWidget, moveWidget, nudge,
	setSpan, allPaths, addTab, removeTab, moveTab, setAndroid, flow, isId,
} from '../js/editor/draft.js';
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
	const g = exampleOf(desc, 'gauge');
	assert.deepEqual(g, { type: 'gauge', label: 'CPU', value: 'cpu.usage', max: 100 });
	const at = addWidget(d.value, [], 99, g);
	assert.deepEqual(at, [0]);
	assert.equal(widgetAt(d.value, [0]).value, 'cpu.usage');
	// The example is a copy: changing the draft leaves the description as it was.
	widgetAt(d.value, [0]).label = 'Changed';
	assert.equal(exampleOf(desc, 'gauge').label, 'CPU');
	// A group's example comes with its widgets, and is opened out on the canvas.
	addWidget(d.value, [], 0, exampleOf(desc, 'group'));
	assert.ok(isGroup(desc, widgetAt(d.value, [0])));
	assert.equal(widgetAt(d.value, [0, 0]).type, 'value');
	// A type the service didn't describe still makes a widget of just its type.
	assert.deepEqual(exampleOf(desc, 'nothing-known'), { type: 'nothing-known' });
});

test('an example\'s key its type doesn\'t list is kept as is', () => {
	// The graph's example has `max`, which the hand-written description doesn't list for a graph.
	const g = exampleOf(desc, 'graph');
	assert.deepEqual(unknownKeys(desc, g), ['max']);
	assert.equal(g.max, 100);
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
	assert.deepEqual(labels(v.widgets), ['CPU', 'A', 'B', 'G', 'C']);
	setKey(v.widgets[0], 'label', 'Graph');
	setSpan(v.widgets[1], 3);
	const sparkle = v.widgets.find((w) => w.type === 'sparkle');
	setKey(sparkle, 'label', 'Still sparkling');
	setSpan(sparkle, 9);
	const sent = JSON.parse(JSON.stringify({ id: 'mine', dashboard: v, base: desc.hash }));
	const a = sent.dashboard.widgets.find((w) => w.label === 'A');
	assert.deepEqual(a.glow, { radius: 3, colours: ['cpu', 'hot'] });
	assert.equal(a.span, 3);
	const s = sent.dashboard.widgets.find((w) => w.type === 'sparkle');
	assert.deepEqual(s.glitter, [1, 2, 3]);
	assert.equal(s.span, 4, 'the width is clamped to the grid');
	assert.deepEqual(sent.dashboard.someday, { kept: true });
	assert.equal(sent.dashboard.widgets.find((w) => w.type === 'graph').max, 100);
	assert.deepEqual(unknownKeys(desc, a), ['glow']);
	assert.deepEqual(unknownKeys(desc, s), ['label', 'glitter', 'span'].filter((k) => !desc.common.some((c) => c.key === k)));
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
	assert.equal(spanOf(desc, { type: 'gauge' }), 2);
	assert.equal(spanOf(desc, { type: 'graph' }), 4);
	assert.equal(spanOf(desc, { type: 'graph', span: 1 }), 1);
	assert.equal(spanOf(desc, { type: 'gauge', span: 7 }), 4);
	assert.equal(spanOf(desc, { type: 'nothing-known' }), 4);
	const w = { type: 'value' };
	assert.equal(setSpan(w, 0), 1);
	assert.equal(setSpan(w, 2.6), 3);
});

test('the grid: rows filled, the last of a row taking the columns left', () => {
	assert.deepEqual(flow([2, 2, 2, 2], 4), [2, 2, 2, 2]);
	assert.deepEqual(flow([2, 1], 4), [2, 2]);
	assert.deepEqual(flow([2, 2, 2], 4), [2, 2, 4]);
	assert.deepEqual(flow([1, 1, 1, 4, 3, 3], 4), [1, 1, 2, 4, 4, 4]);
	assert.deepEqual(flow([2, 1, 1, 3], 2), [1, 1, 2, 2]);
	assert.deepEqual(flow([4, 1], 2), [2, 2]);
	assert.deepEqual(flow([1, 2, 3], 1), [1, 1, 1]);
	assert.deepEqual(flow([], 4), []);
});

test('a profile\'s tabs: added once, taken out, moved; its Android dashboards', () => {
	const p = draft(fx('editor.json').profiles[0].def);
	assert.equal(addTab(p.value, 'cpu'), true);
	assert.equal(addTab(p.value, 'cpu'), false);
	assert.deepEqual(p.value.dashboards, ['overview', 'mine', 'cpu']);
	moveTab(p.value, 2, 0);
	assert.deepEqual(p.value.dashboards, ['cpu', 'overview', 'mine']);
	removeTab(p.value, 1);
	assert.deepEqual(p.value.dashboards, ['cpu', 'mine']);
	addTab(p.value, 'overview', 1);
	assert.deepEqual(p.value.dashboards, ['cpu', 'overview', 'mine']);
	setAndroid(p.value, 'tiles', 'tiles');
	assert.deepEqual(p.value.android, { widget: 'phone', tiles: 'tiles', card: '' });
	assert.equal(isDirty(p), true);
	// What is sent is the profile's request in the fixture's shape.
	const body = fx('editor-profile.json').request;
	moveTab(p.value, 0, 2);
	removeTab(p.value, 2);
	assert.deepEqual(p.value, body.profile);
	const empty = {};
	setAndroid(empty, 'card', 'mine');
	addTab(empty, 'mine');
	assert.deepEqual(empty, { android: { card: 'mine' }, dashboards: ['mine'] });
});

test('ids as the settings take them', () => {
	for (const ok of ['mine', 'a', 'cpu_2', 'x-y', 'a'.repeat(32)]) assert.equal(isId(ok), true, ok);
	for (const bad of ['', 'Mine', 'a.b', 'a b', 'a'.repeat(33), 3, null]) assert.equal(isId(bad), false, String(bad));
});
