// A stand-in for elamond while the page is written: the page's files, and the API answered from
// elamond/fixtures. Development only; never built into elamond. `node test/mock.mjs [port]`, then
// http://127.0.0.1:<port>/ (code 123456 pairs; `--paired` after the port: every request is paired,
// for a headless browser). Not the service's behaviour: its tests are its own.
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { join, extname, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { adminPanel } from './mock-admin.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const web = join(here, '..');
const fixtures = join(web, '..', 'fixtures');
const port = Number(process.argv[2] || 47899);
const fx = (name) => JSON.parse(readFileSync(join(fixtures, name), 'utf8'));
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.webmanifest': 'application/manifest+json', '.json': 'application/json' };

const json = (res, status, body, headers = {}) => {
	res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', ...headers });
	res.end(JSON.stringify(body));
};
const paired = (req) => process.argv.includes('--paired') || /(?:^|;\s*)elamon-mock=1/.test(req.headers.cookie || '');
const body = (req) => new Promise((ok) => { let b = ''; req.on('data', (c) => (b += c)); req.on('end', () => { try { ok(b ? JSON.parse(b) : {}); } catch { ok({}); } }); });

function ui() {
	const u = fx('ui.json');
	u.role = 'owner';
	u.service.features = [...new Set([...u.service.features, 'admin', 'editor'])].sort();
	return u;
}

const admin = {
	'GET state': () => fx('0.11/admin-state.json'),
	'GET settings': () => fx('0.11/admin-settings.json'),
	'GET clients': () => fx('0.11/admin-clients.json'),
	'GET pairing': () => fx('0.11/admin-pairing.json').pending,
	'POST pairing': () => fx('0.11/admin-pairing.json').pending,
	'GET file': () => fx('0.11/admin-file.json'),
	'POST file/check': (b) => (String(b.text || '').includes('lots') ? fx('0.11/admin-check.json').broken.response : fx('0.11/admin-check.json').sound.response),
	'POST file/save': () => fx('0.11/admin-save.json').saved.body,
	'GET editor': () => fx('0.11/editor.json'),
	'POST editor/dashboard': () => fx('0.11/editor-dashboard.json').response,
	'POST editor/profile': () => fx('0.11/editor-dashboard.json').response,
	'POST editor/revert': () => fx('0.11/editor-profile.json').revert.response,
	'POST preview': () => fx('0.11/admin-preview.json').response,
	'GET activity': () => ({ entries: [] }),
	'GET events': () => ({ events: [] }),
	'GET values': () => ({ sources: [] }),
};

// The dashboard editor, kept as it is written: the description changes with each write, a `base` that
// isn't the editor file's hash now is 409, and a few mistakes the service would find are 422.
const editor = { desc: fx('0.11/editor.json'), generation: 4 };
const hex = () => [...Array(16)].map(() => '0123456789abcdef'[Math.floor(Math.random() * 16)]).join('');
const DASHBOARD_KEYS = ['title', 'icon', 'role', 'for_each', 'widgets'];

/** Some of the mistakes elamond finds in a dashboard of the editor's: enough to see them shown. */
function mistakesOf(id, dash) {
	const out = [];
	const known = new Set(editor.desc.values.map((v) => v.address));
	for (const k of Object.keys(dash || {})) {
		if (!DASHBOARD_KEYS.includes(k)) out.push({ file: editor.desc.file, line: null, path: `dashboards.${id}.${k}`, message: `a dashboard has no key ${k} (${DASHBOARD_KEYS.join(', ')})` });
	}
	if (!dash || typeof dash.title !== 'string' || !dash.title) out.push({ file: editor.desc.file, line: null, path: `dashboards.${id}.title`, message: 'a dashboard needs a title' });
	const walk = (list, at) => (Array.isArray(list) ? list : []).forEach((w, i) => {
		const path = `${at}.widgets[${i}]`;
		for (const k of ['max', 'min']) {
			if (typeof w[k] === 'string' && !known.has(w[k])) out.push({ file: editor.desc.file, line: null, path: `${path}.${k}`, message: `expected a number or an address, like "cpu.usage", found "${w[k]}"` });
		}
		if (w.action !== undefined && !String(w.action).includes('{') && !editor.desc.actions.some((a) => a.id === w.action)) out.push({ file: editor.desc.file, line: null, path: `${path}.action`, message: `no action ${JSON.stringify(w.action)}` });
		walk(w.widgets, path);
	});
	walk(dash && dash.widgets, `dashboards.${id}`);
	return out;
}

/** The `ui` a draft makes, roughly as elamond resolves it: ids by place, spans written out. */
function resolve(id, dash) {
	const types = new Map(editor.desc.types.map((t) => [t.type, t]));
	const walk = (list, prefix) => (Array.isArray(list) ? list : []).map((w, i) => {
		const out = { ...w, id: `${prefix}${w.id || i}`, span: w.span ?? types.get(w.type)?.span ?? 4 };
		if (Array.isArray(w.widgets)) out.widgets = walk(w.widgets, `${out.id}/`);
		return out;
	});
	return { id, title: dash.title || id, icon: dash.icon, widgets: walk(dash.widgets, `${id}/`) };
}

function editorPreview(b) {
	const dashboards = (b.editor && b.editor.dashboards) || {};
	const mistakes = Object.entries(dashboards).flatMap(([id, d]) => mistakesOf(id, d));
	if (mistakes.length) return [422, { error: 'bad_settings', mistakes }];
	const id = b.dashboard;
	const dash = dashboards[id] || (editor.desc.dashboards.find((d) => d.id === id) || {}).def;
	if (!dash) return [404, { error: 'no_such_dashboard' }];
	const drawn = resolve(id, dash);
	const now = new Map(editor.desc.values.map((v) => [v.address, v.now]));
	const widgets = {};
	const fill = (list) => list.forEach((w) => {
		widgets[w.id] = { v: Math.round(Math.random() * 1000) / 10, text: now.get(w.value) || (w.text ? String(w.text).replace(/\{([^}:]+)[^}]*\}/g, (m, a) => now.get(a) || '—') : undefined), level: 'normal' };
		if (w.widgets) fill(w.widgets);
	});
	fill(drawn.widgets);
	const u = ui();
	const profile = editor.desc.profiles.find((p) => p.id === editor.desc.profile);
	const tabs = (profile && profile.def.dashboards) || u.tabs;
	return [200, { ui: { ...u, version: hex(), tabs: tabs.includes(id) ? tabs : [...tabs, id], dashboards: [drawn, ...u.dashboards.filter((d) => d.id !== id)] }, data: { t: Date.now(), dashboard: id, widgets } }];
}

function editorWrite(what, b) {
	const d = editor.desc;
	if (b.base !== d.hash) return [409, { error: 'changed_on_disk', hash: d.hash }];
	if (what === 'dashboard') {
		const mistakes = mistakesOf(b.id, b.dashboard);
		if (mistakes.length) return [422, { error: 'bad_settings', mistakes }];
		const entry = d.dashboards.find((x) => x.id === b.id);
		if (entry) Object.assign(entry, { title: b.dashboard.title, def: b.dashboard, editor: true, from: [...entry.from.filter((f) => f.file !== d.file), { file: d.file, line: 5 }] });
		else d.dashboards.push({ id: b.id, title: b.dashboard.title, def: b.dashboard, from: [{ file: d.file, line: 5 }], editor: true, own: true });
	} else if (what === 'profile') {
		const p = d.profiles.find((x) => x.id === b.id);
		if (!p) return [422, { error: 'bad_settings', mistakes: [{ file: d.file, line: null, path: `profiles.${b.id}`, message: 'no such profile' }] }];
		const missing = (b.profile.dashboards || []).findIndex((x) => !d.dashboards.some((y) => y.id === x));
		if (missing >= 0) return [422, { error: 'bad_settings', mistakes: [{ file: d.file, line: null, path: `profiles.${b.id}.dashboards[${missing}]`, message: `no dashboard ${b.profile.dashboards[missing]}` }] }];
		Object.assign(p, { def: b.profile, editor: true });
	} else if (what === 'revert') {
		const original = fx('0.11/editor.json');
		if (b.dashboard) {
			const o = original.dashboards.find((x) => x.id === b.dashboard && !x.own);
			const i = d.dashboards.findIndex((x) => x.id === b.dashboard);
			if (i < 0) return [404, { error: 'not_found' }];
			if (o) d.dashboards[i] = { ...o, editor: false, from: o.from.filter((f) => f.file !== d.file) };
			else d.dashboards.splice(i, 1);
		} else {
			const o = original.profiles.find((x) => x.id === b.profile);
			const p = d.profiles.find((x) => x.id === b.profile);
			if (!p || !o) return [404, { error: 'not_found' }];
			Object.assign(p, { def: o.def, editor: false });
		}
	} else if (what === 'delete') {
		const i = d.dashboards.findIndex((x) => x.id === b.dashboard && x.own);
		if (i < 0) return [404, { error: 'not_found' }];
		d.dashboards.splice(i, 1);
		for (const p of d.profiles) p.def.dashboards = (p.def.dashboards || []).filter((x) => x !== b.dashboard);
	}
	d.hash = hex();
	editor.generation += 1;
	return [200, { ok: true, hash: d.hash, backup: String(Date.now()), generation: editor.generation }];
}

createServer(async (req, res) => {
	const url = new URL(req.url, 'http://x');
	const p = url.pathname;
	if (!p.startsWith('/api/') && !p.startsWith('/mock/')) {
		const file = p === '/' ? join(web, 'index.html') : p.startsWith('/web/') ? normalize(join(web, p.slice(5))) : null;
		if (!file || !file.startsWith(web) || file.includes(`${'/test/'}`) || !existsSync(file) || statSync(file).isDirectory()) return json(res, 404, { error: 'not_found' });
		res.writeHead(200, { 'Content-Type': types[extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-cache' });
		return res.end(readFileSync(file));
	}
	if (p === '/mock/editor-touched' && req.method === 'POST') {
		editor.desc.hash = hex();
		return json(res, 200, { hash: editor.desc.hash });
	}
	if (p === '/api/v1/hello') return json(res, 200, fx('0.11/hello.json'));
	if (p === '/api/v1/pair' && req.method === 'POST') {
		const b = await body(req);
		if (b.code !== '123456') return json(res, 403, { error: 'wrong_code' });
		return json(res, 200, fx('0.11/pair-web.json').response.body, { 'Set-Cookie': 'elamon-mock=1; Path=/; HttpOnly; SameSite=Strict' });
	}
	if (!paired(req)) return json(res, 401, { error: 'unauthorized' });
	if (p === '/api/v1/ui') return json(res, 200, ui());
	if (p === '/api/v1/unpair') return json(res, 200, { ok: true }, { 'Set-Cookie': 'elamon-mock=; Path=/; Max-Age=0' });
	if (p === '/api/v1/stream') {
		res.writeHead(200, { 'Content-Type': 'application/x-ndjson', 'Cache-Control': 'no-store' });
		res.write(JSON.stringify({ ...fx('stream-open.json'), ui: ui().version }) + '\n');
		const send = () => res.write(JSON.stringify({ ...fx('stream-data.json'), t: Date.now() }) + '\n');
		send();
		const timer = setInterval(send, 1000);
		req.on('close', () => clearInterval(timer));
		return;
	}
	if (p.startsWith('/api/v1/stream/')) return json(res, 200, { ok: true });
	if (p === '/api/v1/snapshot') { const d = fx('stream-data.json'); delete d.type; return json(res, 200, d); }
	if (p.startsWith('/api/v1/actions/')) return json(res, 200, fx('action-result.json'));
	if (p.startsWith('/api/v1/admin/')) {
		const route = admin[`${req.method} ${p.slice('/api/v1/admin/'.length)}`];
		const b = req.method === 'POST' ? await body(req) : {};
		const sub = p.slice('/api/v1/admin/'.length);
		if (req.method === 'GET' && sub === 'editor') return json(res, 200, editor.desc);
		if (req.method === 'POST' && sub === 'preview' && b.editor) return json(res, ...editorPreview(b));
		if (req.method === 'POST' && sub.startsWith('editor/')) return json(res, ...editorWrite(sub.slice('editor/'.length), b));
		const panel = adminPanel(req.method, p.slice('/api/v1/admin/'.length), b, url.searchParams);
		if (panel) return json(res, panel.status, panel.body);
		return route ? json(res, 200, route(b)) : json(res, 404, { error: 'not_found' });
	}
	json(res, 404, { error: 'not_found' });
}).listen(port, '127.0.0.1', () => console.log(`mock elamond: http://127.0.0.1:${port}/ (code 123456)`));
