import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { isFingerprint, colonFingerprint, printedFingerprint } from '../js/cert.js';
import { asked, snap, asNumber, initial, valid, phraseWith, togglePhrase, typedChoice, valueWords } from '../js/actions.js';
import { errorText } from '../js/errors.js';
import { errorSentence } from '../js/admin/errors.js';
import { ApiError } from '../js/api.js';
import { S, fill } from '../js/strings.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/${name}`, import.meta.url), 'utf8'));
const ui = fx('ui.json');
const action = (id, u = ui) => u.actions.find((a) => a.id === id);

test('the fingerprint as a browser shows it and as elamond pair prints it', () => {
	const fp = fx('0.11/hello.json').fingerprint;
	assert.ok(isFingerprint(fp));
	assert.equal(isFingerprint('abc'), false);
	assert.equal(isFingerprint(undefined), false);
	assert.equal(colonFingerprint(fp).slice(0, 11), '1F:0E:8D:2C');
	assert.equal(colonFingerprint(fp).split(':').length, 32);
	assert.equal(printedFingerprint(fp), '1F0E 8D2C 3B4A 5968 7766 5544 3322 1100');
});

test('the values an action asks for: those a widget leaves open that must be given', () => {
	assert.deepEqual(asked(action('backup'), { verify: true }), []);
	assert.deepEqual(asked(action('mode'), {}).map((p) => p.id), ['mode']);
	assert.deepEqual(asked(action('process.signal'), { pid: 1 }).map((p) => p.id), ['signal']);
	assert.deepEqual(asked(action('volume.set'), {}), []);
	assert.equal(initial(asked(action('mode'), {})[0]), 'desktop');
	assert.equal(initial({ id: 'x', kind: 'range', min: 5 }), 5);
	assert.equal(initial({ id: 'x', kind: 'bool' }), false);
});

test('values checked as the service checks them', () => {
	const fan = action('fan').params[0];
	assert.equal(valid(fan, 50), true);
	assert.equal(valid(fan, 52), false);
	assert.equal(valid(fan, 105), false);
	assert.equal(valid(fan, '50'), false);
	assert.equal(snap(52, fan), 50);
	assert.equal(snap(53, fan), 55);
	assert.equal(snap(140, fan), 100);
	assert.equal(asNumber(50, fan), 50);
	assert.equal(valid({ id: 'x', kind: 'integer' }, 1.5), false);
	assert.equal(valid({ id: 'x', kind: 'choice', choices: [{ value: 'a' }] }, 'a'), true);
	assert.equal(valid({ id: 'x', kind: 'later' }, {}), true);
});

test('what asks names the value: a choice by its label, a number with its unit', () => {
	const mode = action('mode').params;
	assert.equal(phraseWith('Mode', mode, { mode: 'server' }), 'Mode: Server');
	assert.equal(phraseWith('Mode', mode, {}), 'Mode');
	assert.equal(valueWords(action('fan').params[0], 55), '55%');
	assert.equal(togglePhrase('Mute', action('volume.set').params[1], true), 'Mute');
	const display = { id: 'state', kind: 'choice', choices: [{ value: 'on', label: 'On' }, { value: 'off', label: 'Off' }] };
	assert.equal(togglePhrase('DP-1', display, 'off'), 'DP-1: Off');
});

test('a choice that is typed in', () => {
	const ups = fx('0.7/ui-ups.json');
	const cmd = ups.actions.find((a) => a.id === 'nut.command.eco');
	const typed = fx('0.7/action-request-typed.json');
	const c = typedChoice(cmd, typed.params);
	assert.ok(c);
	assert.equal(c.confirm_text, typed.confirm_text);
	assert.equal(typedChoice(cmd, { cmd: 'outlet.1.load.on' }), null);
});

test('refusals in words', () => {
	const said = (status, body) => errorText(new ApiError(status, body.error, body.message ?? null, body));
	const displays = fx('0.10/actions-displays.json');
	assert.equal(said(502, displays.not_applied.body), S.panels.errNotApplied);
	assert.equal(said(502, displays.waiting_for_a_screen.body), S.panels.errWaitingForAScreen);
	assert.match(said(403, displays.confirm_required.body), /DP-1/);
	assert.equal(said(404, displays.gone.body), S.panels.errNoSuchAction);
	assert.equal(said(502, { error: 'failed', message: 'access denied' }), 'access denied');
	assert.match(said(400, { error: 'bad_param', message: 'level' }), /level/);
	assert.equal(said(403, { error: 'wrong_code' }), S.panels.pairWrongCode);
	assert.equal(said(403, { error: 'bad_origin' }), S.panels.errBadOrigin);
	assert.match(said(418, { error: 'later_code' }), /later_code/);
	// Every code the service answers has words of its own, never only "the computer said: <code>":
	// the admin panel's for all; the dashboards' and pairing's for all but those the admin API alone
	// answers (docs/protocol.md, what came with elamond 0.11).
	const adminOnly = new Set(['admin_off', 'writes_off', 'read_only', 'at_computer', 'bad_name', 'changed_on_disk', 'bad_settings']);
	for (const e of fx('errors.json')) {
		const err = new ApiError(e.status, e.error, null, { error: e.error });
		assert.notEqual(errorSentence(err), fill(S.admin.errors.other, e.error), e.error);
		if (!adminOnly.has(e.error)) assert.notEqual(errorText(err), fill(S.panels.errCode, e.error), e.error);
	}
	assert.equal(errorText(new TypeError('fetch failed')), S.panels.errUnreachable);
});

test('the page asks for the types it has a module for, and no other', async () => {
	const { TYPES } = await import('../js/widgets/index.js');
	const { DRAWS } = await import('../js/panels.js');
	assert.deepEqual(DRAWS.split(',').sort(), [...TYPES].sort());
	// Every type the fixtures' ui documents use is drawn.
	const used = new Set();
	const walk = (ws) => ws.forEach((w) => (used.add(w.type), w.widgets && walk(w.widgets)));
	for (const f of ['ui.json', 'ui-default.json', '0.7/ui-ups.json', '0.8/ui-sources.json', '0.8.1/ui-legend.json', '0.9/ui-events.json', '0.10/ui-displays.json']) for (const d of fx(f).dashboards) walk(d.widgets);
	for (const t of used) assert.ok(TYPES.includes(t), t);
});

test('a role in words, one of a newer service by its id', async () => {
	const { roleName, roleChoice } = await import('../js/roles.js');
	assert.equal(roleName('owner'), S.roles.names.owner);
	assert.equal(roleName('steward'), 'Steward');
	assert.equal(roleName(null), '');
	assert.equal(roleChoice('monitor'), `${S.roles.names.monitor}: ${S.roles.about.monitor}`);
	assert.equal(roleChoice('steward'), 'Steward');
	assert.equal(roleName('toString'), 'ToString', "not a name of the table's");
	// Every role the service lists has words.
	for (const r of fx('0.11/admin-clients.json').roles) assert.notEqual(roleChoice(r), roleName(r), r);
});
