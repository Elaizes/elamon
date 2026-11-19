// The admin API of the mock (mock.mjs), with state, so the admin panel can be tried: clients whose
// role, profile and pairing change, a pairing code with someone asking, control held off, settings
// files that are saved, backed up, restored, renamed and deleted. Development only, never built
// into elamond; answers from the fixtures 0.11 where they have one. Not the service's behaviour.
//
// To see a refusal: a draft holding `lots` doesn't parse (422), one holding `# conflict` is
// refused as changed on disk (409); `elamond.toml` is read-only, as under Nix.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const fixtures = join(fileURLToPath(new URL('.', import.meta.url)), '..', '..', 'fixtures');
const fx = (name) => JSON.parse(readFileSync(join(fixtures, name), 'utf8'));
const now = () => Math.floor(Date.now() / 1000);
const hash = (text) => createHash('sha256').update(text).digest('hex').slice(0, 16);
const ok = (body) => ({ status: 200, body });
const no = (status, error, more = {}) => ({ status, body: { error, ...more } });
const ROLES = ['glance', 'monitor', 'operate', 'admin', 'owner'];
const started = now() - 3 * 3600 - 17 * 60;

const state = fx('0.11/admin-state.json');
state.service.started = started;
const clients = fx('0.11/admin-clients.json');
clients.clients[0].seen = now() - 7 * 60;
clients.clients[1].seen = now() - 20;
clients.clients.push({ id: '4567cdef', name: 'Tablet', role: 'monitor', profile: null, own_profile: 'tablet', kind: 'android', app: 'android/0.10.1', address: '192.0.2.21', seen: now() - 3 * 86400, created: now() - 40 * 86400, from: '0123abcd', added: [], you: false });
clients.clients[0].added = ['4567cdef'];

const pairingFx = fx('0.11/admin-pairing.json');
/** @type {any} */
let pending = { ...pairingFx.pending, expires: now() + 300, ask: { ...pairingFx.pending.ask, at: now() - 40 } };

const check = fx('0.11/admin-check.json');
const mine = fx('0.11/admin-file.json');
const dashboardsText = Array.from({ length: 60 }, (_, i) => (i % 6 === 0 ? `[dashboards.d${i}]\ntitle = "Dashboard ${i}"` : `widgets_${i} = [{ type = "value", value = "cpu.usage" }]`)).join('\n') + '\n';

/** name → { kind, text, writable, why, note, backups: [{ id, at, size, text }] } */
const files = new Map([
	['elamond.toml', { kind: 'main', text: readFileSync(join(fixtures, '0.11/admin-settings.toml'), 'utf8'), writable: false, why: state.files[0].why, note: null }],
	['elamond.d/dashboards.toml', { kind: 'dropin', text: dashboardsText, writable: true, why: null, note: null }],
	['elamond.d/mine.toml', { kind: 'dropin', text: mine.text, writable: true, why: null, note: null }],
	['elamond.d/phones.toml', { kind: 'dropin', text: '# Profiles of the paired phones, one each.\n\n# Pixel 9, paired 2026-11-12 (0123abcd)\n[profiles.pixel-9]\nname = "Pixel 9"\nuse = "default"\n', writable: true, why: null, note: 'phones' }],
	['elamond.d/zz-editor.toml', { kind: 'dropin', text: '# Written by the dashboard editor.\n', writable: true, why: null, note: 'editor' }],
]);
for (const f of files.values()) Object.assign(f, { modified: now() - 86400, backups: [] });
files.get('elamond.d/mine.toml').backups.push({ id: String((now() - 7200) * 1000 + 123), at: now() - 7200, size: 61, text: '[dashboards.mine]\ntitle = "Mine"\n' });
let generation = 3;

const DROPIN = /^elamond\.d\/[A-Za-z0-9][A-Za-z0-9_.-]{0,63}\.toml$/;
const fileName = (name) => (name === 'elamond.toml' || DROPIN.test(String(name || '')) ? name : null);

/** A unified diff of two texts, by their lines, as the service's `diff` (one hunk, all lines). */
function diff(a, b) {
	if (a === b) return [];
	const x = a.split('\n');
	const y = b.split('\n');
	if (x[x.length - 1] === '') x.pop();
	if (y[y.length - 1] === '') y.pop();
	const n = x.length;
	const m = y.length;
	const L = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
	for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) L[i][j] = x[i] === y[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1]);
	const out = [`@@ -1,${n} +1,${m} @@`];
	let i = 0;
	let j = 0;
	while (i < n || j < m) {
		if (i < n && j < m && x[i] === y[j]) (out.push(` ${x[i]}`), i++, j++);
		else if (j < m && (i >= n || L[i][j + 1] >= L[i + 1][j])) out.push(`+${y[j++]}`);
		else out.push(`-${x[i++]}`);
	}
	return out;
}

/** The mistakes of a draft: `lots` where a number should be. */
function mistakes(name, text) {
	const lines = text.split('\n');
	const at = lines.findIndex((l) => l.includes('lots'));
	if (at === -1) return [];
	return [{ ...check.broken.response.mistakes[0], file: name, line: at + 1 }];
}

function stateNow() {
	return {
		...state,
		control: state.control,
		settings: { ...state.settings, generation },
		files: [...files].map(([name, f]) => ({ name, kind: f.kind, writable: f.writable, why: f.why, hash: hash(f.text), size: Buffer.byteLength(f.text), modified: f.modified })),
	};
}

function fileAnswer(name, f) {
	return { name, kind: f.kind, text: f.text, hash: hash(f.text), writable: f.writable, why: f.why, note: f.note, backups: f.backups.map(({ id, at, size }) => ({ id, at, size })).reverse() };
}

/** Writes `text` to `name` as a save does: base, the settings sound, a backup kept. */
function write(name, text, base) {
	const f = files.get(name);
	if (f && !f.writable) return no(403, 'read_only', { message: f.why });
	if (!state.control.enabled && state.control.off_reason === 'held_off') return no(403, 'writes_off', { message: 'held_off' });
	if (text.includes('# conflict')) return no(409, 'changed_on_disk', { hash: '0000111122223333' });
	if (f ? base !== hash(f.text) : base !== null) return no(409, 'changed_on_disk', { hash: f ? hash(f.text) : null });
	const bad = mistakes(name, text);
	if (bad.length) return no(422, 'bad_settings', { mistakes: bad });
	let backup = null;
	if (f) {
		backup = String(Date.now());
		f.backups.push({ id: backup, at: now(), size: Buffer.byteLength(f.text), text: f.text });
		Object.assign(f, { text, modified: now() });
	} else files.set(name, { kind: 'dropin', text, writable: true, why: null, note: null, modified: now(), backups: [] });
	generation++;
	return ok({ ok: true, hash: hash(text), backup, generation });
}

/**
 * The answer to `method sub` (the path after /api/v1/admin/), or null to leave it to mock.mjs.
 * @param {string} method @param {string} sub @param {any} b the body @param {URLSearchParams} q
 * @returns {{ status: number, body: any } | null}
 */
export function adminPanel(method, sub, b, q) {
	const you = clients.clients.find((c) => c.you);
	const client = /^clients\/([^/]+)\/(role|profile|revoke)$/.exec(sub);
	if (method === 'POST' && client) {
		const c = clients.clients.find((x) => x.id === decodeURIComponent(client[1]));
		if (!c) return no(404, 'not_found');
		if (!state.control.enabled && state.control.off_reason === 'held_off') return no(403, 'writes_off', { message: 'held_off' });
		const family = [c, ...(b.with_added ? clients.clients.filter((x) => c.added.includes(x.id)) : [])];
		if (client[2] === 'role') {
			if (!ROLES.includes(b.role)) return no(400, 'bad_param', { message: 'role' });
			if (ROLES.indexOf(b.role) > ROLES.indexOf(you.role)) return no(403, 'not_allowed');
			for (const x of family) x.role = b.role;
		} else if (client[2] === 'profile') {
			if (b.profile !== null && !clients.profiles.includes(b.profile)) return no(400, 'bad_param', { message: 'profile' });
			for (const x of family) x.profile = b.profile;
		} else {
			const gone = new Set(family.map((x) => x.id));
			clients.clients = clients.clients.filter((x) => !gone.has(x.id));
		}
		return ok({ ok: true, changed: family.map((x) => x.id) });
	}
	switch (`${method} ${sub}`) {
		case 'GET state':
			return ok(stateNow());
		case 'GET clients':
			return ok(clients);
		case 'GET pairing':
			if (pending.code && pending.expires < now()) pending = { code: null };
			return ok(pending);
		case 'POST pairing':
			if (!ROLES.includes(b.role)) return no(400, 'bad_param', { message: 'role' });
			if (ROLES.indexOf(b.role) > ROLES.indexOf(you.role)) return no(403, 'not_allowed');
			pending = { ...pairingFx.pending, code: String(100000 + Math.floor(Math.random() * 900000)), expires: now() + 300, role: b.role, profile: b.profile ?? null, ask: null };
			// Someone gives the code a few seconds later.
			setTimeout(() => {
				if (pending.code) pending.ask = { ...pairingFx.pending.ask, at: now() };
			}, 6000);
			return ok(pending);
		case 'POST pairing/answer':
			if (!pending.ask || pending.ask.id !== b.ask) return no(404, 'not_found');
			if (b.allow) clients.clients.push({ id: pending.ask.id.slice(0, 8), name: pending.ask.name, role: pending.role, profile: b.take ? pending.ask.offer : pending.profile, own_profile: null, kind: 'web', app: 'web/0.11.0', address: pending.ask.address, seen: now(), created: now(), from: null, added: [], you: false });
			pending = { code: null };
			return ok({ ok: true });
		case 'POST pairing/withdraw':
			pending = { code: null };
			return ok({ ok: true });
		case 'POST control':
			if (b.on) return no(403, 'at_computer');
			state.control = { enabled: false, off_reason: 'held_off', held_off_since: now() };
			return ok({ enabled: false, off_reason: 'held_off' });
		case 'GET settings':
			return ok(fx('0.11/admin-settings.json'));
		case 'GET file': {
			const name = q.get('name');
			if (!fileName(name)) return no(400, 'bad_name');
			const f = files.get(name);
			return f ? ok(fileAnswer(name, f)) : no(404, 'not_found');
		}
		case 'GET file/backup': {
			const f = files.get(q.get('name'));
			const k = f && f.backups.find((x) => x.id === q.get('id'));
			return k ? ok({ name: q.get('name'), id: k.id, at: k.at, text: k.text }) : no(404, 'not_found');
		}
		case 'POST file/check': {
			if (!fileName(b.name)) return no(400, 'bad_name');
			const bad = mistakes(b.name, String(b.text ?? ''));
			const f = files.get(b.name);
			return ok({ sound: !bad.length, mistakes: bad, warnings: [], diff: diff(f ? f.text : '', String(b.text ?? '')) });
		}
		case 'POST file/save':
			if (!fileName(b.name)) return no(400, 'bad_name');
			return write(b.name, String(b.text ?? ''), b.base ?? null);
		case 'POST file/restore': {
			const f = files.get(b.name);
			const k = f && f.backups.find((x) => x.id === b.backup);
			return k ? write(b.name, k.text, b.base) : no(404, 'not_found');
		}
		case 'POST file/rename': {
			const f = files.get(b.name);
			if (!fileName(b.to) || !DROPIN.test(b.name) || !DROPIN.test(b.to)) return no(400, 'bad_name');
			if (!f) return no(404, 'not_found');
			if (files.has(b.to) || b.base !== hash(f.text)) return no(409, 'changed_on_disk', { hash: hash(f.text) });
			files.delete(b.name);
			files.set(b.to, f);
			generation++;
			return ok({ ok: true, hash: hash(f.text), backup: null, generation });
		}
		case 'POST file/delete': {
			const f = files.get(b.name);
			if (!DROPIN.test(String(b.name))) return no(400, 'bad_name');
			if (!f) return no(404, 'not_found');
			if (b.base !== hash(f.text)) return no(409, 'changed_on_disk', { hash: hash(f.text) });
			files.delete(b.name);
			generation++;
			return ok({ ok: true, hash: null, backup: String(Date.now()), generation });
		}
		case 'POST preview': {
			if (!Array.isArray(b.files)) return null;
			for (const f of b.files) {
				const bad = mistakes(f.name, String(f.text ?? ''));
				if (bad.length) return no(422, 'bad_settings', { mistakes: bad });
			}
			const answer = fx('0.11/admin-preview.json').response;
			if (!answer.ui.dashboards.some((d) => d.id === b.dashboard)) return no(404, 'no_such_dashboard');
			answer.data.t = Date.now();
			answer.data.widgets['mine/w0'].v = Math.round(Math.random() * 1000) / 10;
			return ok(answer);
		}
		case 'GET activity':
			return ok({
				entries: [
					{ t: Date.now() - 60000, phone: 'Firefox on Linux', client_id: '89abcdef', what: 'settings elamond.d/mine.toml (3 lines)', result: 'ok' },
					{ t: Date.now() - 3600000, phone: 'Pixel 9', client_id: '0123abcd', what: 'power reboot', result: 'control_disabled', message: 'control is off' },
					{ t: Date.now() - 7200000, phone: 'Tablet', client_id: '4567cdef', what: 'profile balanced', result: 'not_allowed', family: '0123abcd' },
					{ t: Date.now() - 9000000, phone: 'Pixel 9', client_id: '0123abcd', what: 'action Mode server', result: 'ok', message: 'exit 0' },
				],
			});
		case 'GET events':
			return ok({ events: readFileSync(join(fixtures, '0.9/events.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l)).reverse() });
		case 'GET values': {
			const source = q.get('source');
			if (!source) return ok({ sources: [{ id: 'eco', kind: 'nut' }, { id: 'app', kind: 'http' }] });
			if (source === 'eco')
				return ok({
					id: 'eco',
					kind: 'nut',
					values: [
						{ address: 'ups.eco.battery.charge', text: '100 %', role: 'glance' },
						{ address: 'ups.eco.input.voltage', text: '230 V', role: 'glance' },
						{ address: 'ups.eco.ups.status', text: 'OL', role: 'monitor' },
					],
					error: null,
					commands: [{ command: 'outlet.2.load.off', allowed: true }, { command: 'beeper.disable', allowed: false }],
					settings: [{ variable: 'battery.charge.max', value: '90', range: '50-100', allowed: true }],
				});
			if (source === 'app') return ok({ id: 'app', kind: 'http', values: [], error: 'connection refused (192.0.2.40:8080)', commands: [], settings: [] });
			return no(404, 'not_found');
		}
	}
	return null;
}
