import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { classifyDiff, diffCounts } from '../js/admin/diff.js';

const check = JSON.parse(readFileSync(new URL('../../fixtures/0.11/admin-check.json', import.meta.url), 'utf8'));

test("the fixture's diff: each line's kind and its numbers on disk and in the draft", () => {
	// A new drop-in: every line put in, numbered in the draft alone.
	const lines = classifyDiff(check.sound.response.diff);
	assert.deepEqual(lines[0], { kind: 'hunk', text: '@@ -0,0 +1,5 @@', old: null, new: null });
	assert.deepEqual(
		lines.slice(1).map((l) => [l.kind, l.old, l.new]),
		[1, 2, 3, 4, 5].map((n) => ['add', null, n]),
	);
	assert.equal(lines[1].text, '[dashboards.mine]');
	assert.deepEqual(diffCounts(lines), { added: 5, removed: 0 });
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
