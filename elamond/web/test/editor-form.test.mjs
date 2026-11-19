// What the forms read back from a field that takes a word, a number or yes/no.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseScalar } from '../js/editor/form.js';

test('a scalar: numbers and true/false as such, quotes keep words', () => {
	assert.equal(parseScalar(''), undefined);
	assert.equal(parseScalar('server'), 'server');
	assert.equal(parseScalar('42'), 42);
	assert.equal(parseScalar('-1.5'), -1.5);
	assert.equal(parseScalar('true'), true);
	assert.equal(parseScalar('false'), false);
	assert.equal(parseScalar('"true"'), 'true');
	assert.equal(parseScalar('"42"'), '42');
	assert.equal(parseScalar(' '), ' ');
	assert.equal(parseScalar('1e3'), 1000);
	assert.equal(parseScalar('{cpu.usage}'), '{cpu.usage}');
});
