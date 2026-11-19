import { test } from 'node:test';
import assert from 'node:assert/strict';

process.env.TZ = 'UTC';
const { ago, duration, countdown, dateTime, dateTimeMs } = await import('../js/admin/time.js');

const now = 1763550000;

test('"seen" as words', () => {
	assert.equal(ago(null, now), 'never');
	assert.equal(ago(0, now), 'never');
	assert.equal(ago(now, now), 'just now');
	assert.equal(ago(now - 59, now), 'just now');
	assert.equal(ago(now + 300, now), 'just now');
	assert.equal(ago(now - 60, now), '1 min ago');
	assert.equal(ago(now - 5 * 60 - 30, now), '5 min ago');
	assert.equal(ago(now - 3599, now), '59 min ago');
	assert.equal(ago(now - 3600, now), '1 h ago');
	assert.equal(ago(now - 23 * 3600, now), '23 h ago');
	assert.equal(ago(now - 86400, now), '1 day ago');
	assert.equal(ago(now - 12 * 86400 - 5, now), '12 days ago');
});

test('a span, as uptime', () => {
	assert.equal(duration(45), '45 s');
	assert.equal(duration(-3), '0 s');
	assert.equal(duration(12 * 60 + 59), '12 min');
	assert.equal(duration(3 * 3600 + 5 * 60), '3 h 5 min');
	assert.equal(duration(2 * 86400 + 4 * 3600 + 59), '2 d 4 h');
});

test('a countdown', () => {
	assert.equal(countdown(299.2), '5:00');
	assert.equal(countdown(65), '1:05');
	assert.equal(countdown(0.4), '0:01');
	assert.equal(countdown(0), '0:00');
	assert.equal(countdown(-10), '0:00');
});

test("a time in the browser's zone", () => {
	assert.equal(dateTime(1763550000), '2025-11-19 11:00');
	assert.equal(dateTime(1763550007, true), '2025-11-19 11:00:07');
	assert.equal(dateTime(null), '');
	assert.equal(dateTimeMs(1763550007123), '2025-11-19 11:00:07');
	assert.equal(dateTimeMs(undefined), '');
});
