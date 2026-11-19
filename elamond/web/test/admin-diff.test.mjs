import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { classifyDiff, diffCounts } from '../js/admin/diff.js';

const check = JSON.parse(readFileSync(new URL('../../fixtures/0.11/admin-check.json', import.meta.url), 'utf8'));

test("the fixture's diff: each line's kind and its numbers on disk and in the draft", () => {
	const lines = classifyDiff(check.sound.response.diff);
	assert.deepEqual(lines, [
		{ kind: 'hunk', text: '@@ -1,3 +1,3 @@', old: null, new: null },
		{ kind: 'same', text: '[dashboards.mine]', old: 1, new: 1 },
		{ kind: 'same', text: 'title = "Mine"', old: 2, new: 2 },
		{ kind: 'del', text: 'widgets = [{ type = "value", value = "cpu.usage" }]', old: 3, new: null },
		{ kind: 'add', text: 'widgets = [{ type = "gauge", value = "cpu.usage", max = 100 }]', old: null, new: 3 },
	]);
	assert.deepEqual(diffCounts(lines), { added: 1, removed: 1 });
});

test('headers before a hunk, a second hunk, a trimmed empty context line, no newline at the end', () => {
	const lines = classifyDiff(['--- a/x.toml', '+++ b/x.toml', '@@ -10,2 +12 @@', ' a', '', '-b', '@@ -40 +41,2 @@', '+c', '+-d', '\\ No newline at end of file']);
	assert.deepEqual(
		lines.map((l) => [l.kind, l.text, l.old, l.new]),
		[
			['meta', '--- a/x.toml', null, null],
			['meta', '+++ b/x.toml', null, null],
			['hunk', '@@ -10,2 +12 @@', null, null],
			['same', 'a', 10, 12],
			['same', '', 11, 13],
			['del', 'b', 12, null],
			['hunk', '@@ -40 +41,2 @@', null, null],
			['add', 'c', null, 41],
			['add', '-d', null, 42],
			['meta', '\\ No newline at end of file', null, null],
		],
	);
	assert.deepEqual(diffCounts(lines), { added: 2, removed: 1 });
});

test('none is nothing', () => {
	assert.deepEqual(classifyDiff([]), []);
	assert.deepEqual(classifyDiff(null), []);
	assert.deepEqual(classifyDiff(undefined), []);
});
