import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ApiError } from '../js/api.js';
import { errorSentence, isRefusal, isAbort, mistakesOf } from '../js/admin/errors.js';
import { S } from '../js/strings.js';

const save = JSON.parse(readFileSync(new URL('../../fixtures/0.11/admin-save.json', import.meta.url), 'utf8'));
/** An ApiError as api() makes one from a fixture's answer. @param {{ status: number, body: any }} a */
const from = (a) => new ApiError(a.status, a.body.error, a.body.message ?? null, a.body);

test('each code of the admin API has a sentence', () => {
	for (const code of ['not_allowed', 'admin_off', 'bad_origin', 'at_computer', 'bad_name', 'changed_on_disk', 'not_found', 'unauthorized']) {
		assert.equal(errorSentence(new ApiError(403, code, null, { error: code })), S.admin.errors.codes[code], code);
	}
	assert.equal(errorSentence(from(save.not_allowed)), "Your role doesn't allow this.");
	assert.equal(errorSentence(from(save.admin_off)), S.admin.errors.codes.admin_off);
	assert.equal(errorSentence(from(save.at_computer)), S.admin.errors.codes.at_computer);
	assert.equal(errorSentence(from(save.bad_name)), S.admin.errors.codes.bad_name);
});

test("writes_off says why, by the service's message", () => {
	assert.equal(errorSentence(from(save.writes_off)), S.admin.errors.writes_off_held);
	const off = (/** @type {string | null} */ message) => errorSentence(new ApiError(403, 'writes_off', message, { error: 'writes_off', message }));
	assert.equal(off('held_off'), S.admin.errors.writes_off_held);
	assert.equal(off('settings'), S.admin.errors.writes_off_settings);
	assert.match(off('settings'), /\[web\] write = false/);
	// control = false refuses files only: the words don't say writes are off altogether.
	assert.equal(off('control'), S.admin.errors.writes_off_control);
	assert.match(off('control'), /control = false/);
	assert.match(off('control'), /editor still writes/);
	assert.equal(off(null), S.admin.errors.writes_off);
	assert.equal(off('later_reason'), `${S.admin.errors.writes_off} The computer says: later_reason`);
});

test("the service's words where the sentence doesn't hold them", () => {
	// read_only: why, in the sentence.
	assert.equal(errorSentence(from(save.read_only)), 'This file is read-only: set in Nix: change it in your NixOS or Home Manager configuration.');
	assert.equal(errorSentence(new ApiError(403, 'read_only', null, {})), 'This file is read-only.');
	assert.equal(errorSentence(new ApiError(403, 'not_allowed', 'owner needed', {})), "Your role doesn't allow this. The computer says: owner needed");
	assert.equal(errorSentence(new ApiError(400, 'bad_param', 'profile', {})), 'The computer refused the parameter profile.');
	assert.equal(errorSentence(new ApiError(502, 'failed', 'timed_out', {})), 'It failed on the computer: timed_out');
});

test('bad_settings counts its mistakes, and carries them', () => {
	const e = from(save.bad_settings);
	assert.equal(errorSentence(e), "The settings with this change don't parse (1 mistake): nothing was written.");
	assert.equal(mistakesOf(e).length, 1);
	assert.equal(mistakesOf(e)[0].path, 'dashboards.mine.widgets[0].max');
	const two = new ApiError(422, 'bad_settings', null, { error: 'bad_settings', mistakes: [{}, {}] });
	assert.equal(errorSentence(two), "The settings with this change don't parse (2 mistakes): nothing was written.");
	assert.equal(errorSentence(new ApiError(422, 'bad_settings', null, null)), S.admin.errors.bad_settings);
	assert.deepEqual(mistakesOf(new Error('x')), []);
});

test('unknown codes, plain-text answers, an unreachable computer', () => {
	assert.equal(errorSentence(new ApiError(403, 'something_new', null, {})), 'The computer refused: something_new.');
	assert.equal(errorSentence(new ApiError(413, 'http_413', 'Payload Too Large', null)), `${S.admin.errors.too_large} The computer says: Payload Too Large`);
	assert.equal(errorSentence(new ApiError(500, 'http_500', null, null)), 'The computer failed to answer (HTTP 500).');
	assert.equal(errorSentence(new TypeError('Failed to fetch')), S.admin.errors.unreachable);
	assert.equal(errorSentence(new Error('x')), S.admin.errors.unknown);
	assert.equal(errorSentence(null), S.admin.errors.unknown);
});

test('what is a refusal, what was given up on', () => {
	assert.equal(isRefusal(new ApiError(401, 'unauthorized', null, null)), true);
	assert.equal(isRefusal({ status: 403, code: 'x' }), true);
	assert.equal(isRefusal(new TypeError('x')), false);
	assert.equal(isRefusal(null), false);
	assert.equal(isAbort(new DOMException('gone', 'AbortError')), true);
	assert.equal(isAbort(new Error('x')), false);
});
