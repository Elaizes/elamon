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

createServer(async (req, res) => {
	const url = new URL(req.url, 'http://x');
	const p = url.pathname;
	if (!p.startsWith('/api/')) {
		const file = p === '/' ? join(web, 'index.html') : p.startsWith('/web/') ? normalize(join(web, p.slice(5))) : null;
		if (!file || !file.startsWith(web) || file.includes(`${'/test/'}`) || !existsSync(file) || statSync(file).isDirectory()) return json(res, 404, { error: 'not_found' });
		res.writeHead(200, { 'Content-Type': types[extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-cache' });
		return res.end(readFileSync(file));
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
		const panel = adminPanel(req.method, p.slice('/api/v1/admin/'.length), b, url.searchParams);
		if (panel) return json(res, panel.status, panel.body);
		return route ? json(res, 200, route(b)) : json(res, 404, { error: 'not_found' });
	}
	json(res, 404, { error: 'not_found' });
}).listen(port, '127.0.0.1', () => console.log(`mock elamond: http://127.0.0.1:${port}/ (code 123456)`));
