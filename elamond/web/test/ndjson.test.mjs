import { test } from 'node:test';
import assert from 'node:assert/strict';
import { splitLines } from '../js/ndjson.js';

test('whole lines out, the unfinished one kept', () => {
	assert.deepEqual(splitLines('', '{"a":1}\n{"b"'), { lines: ['{"a":1}'], rest: '{"b"' });
	assert.deepEqual(splitLines('{"b"', ':2}\n\n'), { lines: ['{"b":2}'], rest: '' });
	assert.deepEqual(splitLines('', ''), { lines: [], rest: '' });
	assert.deepEqual(splitLines('x', 'y'), { lines: [], rest: 'xy' });
});
