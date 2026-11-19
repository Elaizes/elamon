import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { format, number, parts, round, duration, clock, withUnit, bytes, share, percent } from '../js/format.js';

const cases = JSON.parse(readFileSync(new URL('../../fixtures/formats.json', import.meta.url), 'utf8'));

test('every case of formats.json reads as the service writes it', () => {
	assert.ok(cases.length > 40);
	for (const c of cases) assert.equal(format(c.value, c.format, c.unit), c.text, `${c.format} ${c.unit} ${c.value}`);
});

test('halves round away from zero, by the number as it reads', () => {
	assert.equal(round(12.25, 1), '12.3');
	assert.equal(round(0.125, 2), '0.13');
	assert.equal(round(2.5, 0), '3');
	assert.equal(round(-2.5, 0), '-3');
	assert.equal(round(9.995, 2), '10.00');
	assert.equal(round(99.96, 1), '100.0');
	assert.equal(round(1e21, 0), '1000000000000000000000');
	assert.equal(round(1.5e-7, 2), '0.00');
	assert.equal(round(0.00051, 3), '0.001');
});

test('no minus before nothing', () => {
	assert.equal(round(-0.04, 1), '0.0');
	assert.equal(round(-0, 0), '0');
	assert.equal(number(-0.3, 'integer', ''), '0');
	assert.equal(bytes(-0.2), '0 B');
	assert.equal(bytes(-2048), '-2.00 KiB');
});

test('a value that is not one number', () => {
	assert.equal(format(null, 'percent', '%'), null);
	assert.equal(format(undefined, 'percent', '%'), null);
	assert.equal(format([1, 2], 'percent', '%'), null);
	assert.equal(format('On battery', 'percent', '%'), 'On battery');
	assert.equal(format(true, 'text', ''), 'yes');
});

test('the figure and its unit apart', () => {
	assert.deepEqual(parts(16000000000, 'bytes', 'B'), ['14.9', 'GiB']);
	assert.deepEqual(parts(42.4, 'percent', '%'), ['42', '%']);
	assert.deepEqual(parts(55, 'temp', ''), ['55', '°C']);
	assert.deepEqual(parts(3, 'duration', 's'), ['1 min', null]);
	assert.deepEqual(parts(1, 'bool', ''), ['yes', null]);
});

test('units after their figure', () => {
	assert.equal(withUnit('12', '%'), '12%');
	assert.equal(withUnit('12', 'W'), '12 W');
	assert.equal(withUnit('12', ''), '12');
	assert.equal(withUnit('12', null), '12');
	assert.equal(percent(12.5), '13%');
});

test('durations, shares and a track clock', () => {
	assert.equal(duration(59), '1 min');
	assert.equal(duration(86400 * 2 + 3600 * 5 + 60), '2 d 5 h');
	assert.equal(share('under'), '<1%');
	assert.equal(share(42), '42%');
	assert.equal(clock(187), '3:07');
	assert.equal(clock(3765), '1:02:45');
	assert.equal(clock(-5), '0:00');
});
