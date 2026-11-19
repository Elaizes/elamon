// A stand-in for elamond while the page is written: the page's files, and the API answered from
// elamond/fixtures. Development only; never built into elamond. `node test/mock.mjs [port] [flags]`,
// then http://127.0.0.1:<port>/ (code 123456 pairs). Not the service's behaviour: its tests are its own.
//
// Flags:
//   --paired       every request is taken as paired (a headless browser can show dashboards at once)
//   --rich         more dashboards: the fixtures' UPS, sources and journal, events and monitors, with
//                  graphs' histories, a timeline's long range, older journal lines and a cover
//   --control-off  control held off (`elamond control off`): action widgets drawn disabled
//   --pair-wait=MS the person at elamond pair answers after MS (default 1500)
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { join, extname, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
const web = join(here, '..');
const fixtures = join(web, '..', 'fixtures');
const args = process.argv.slice(2);
const port = Number(args.find((a) => /^\d+$/.test(a)) || 47899);
const flag = (name) => args.includes(`--${name}`);
const option = (name, fallback) => {
	const a = args.find((x) => x.startsWith(`--${name}=`));
	return a ? a.slice(name.length + 3) : fallback;
};
const PAIRED = flag('paired');
const RICH = flag('rich');
const CONTROL_OFF = flag('control-off');
const PAIR_WAIT_MS = Number(option('pair-wait', 1500));
const fx = (name) => JSON.parse(readFileSync(join(fixtures, name), 'utf8'));
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.webmanifest': 'application/manifest+json', '.json': 'application/json' };
// As the service answers its page (docs/protocol.md, "The page"): scripts and styles only from it.
const PAGE_HEADERS = {
	'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
	'X-Content-Type-Options': 'nosniff',
	'Referrer-Policy': 'no-referrer',
	'Cache-Control': 'no-cache',
};

const json = (res, status, body, headers = {}) => {
	res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', ...headers });
	res.end(JSON.stringify(body));
};
const paired = (req) => PAIRED || /(?:^|;\s*)elamon-mock=1/.test(req.headers.cookie || '');
const body = (req) => new Promise((ok) => { let b = ''; req.on('data', (c) => (b += c)); req.on('end', () => { try { ok(b ? JSON.parse(b) : {}); } catch { ok({}); } }); });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- what the computer shows ------------------------------------------------------------------------

/** Widget ids and a dashboard's id with `from/` as `to/`, for two fixtures' dashboards of one id. */
function renamed(dash, from, to) {
	const walk = (w) => ({ ...w, id: w.id.replace(`${from}/`, `${to}/`), widgets: w.widgets ? w.widgets.map(walk) : undefined });
	return { ...dash, id: to, widgets: dash.widgets.map(walk) };
}
const keysRenamed = (widgets, from, to) => Object.fromEntries(Object.entries(widgets).map(([k, v]) => [k.replace(`${from}/`, `${to}/`), v]));

function ui() {
	const u = fx('ui.json');
	u.role = 'owner';
	u.service.features = [...new Set([...u.service.features, 'admin', 'editor'])].sort();
	if (RICH) {
		const more = [fx('0.7/ui-ups.json'), fx('0.8/ui-sources.json'), fx('0.9/ui-events.json')];
		const displays = fx('0.10/ui-displays.json');
		const dashboards = [...u.dashboards];
		for (const m of more) for (const d of m.dashboards) if (!dashboards.some((x) => x.id === d.id)) dashboards.push(d);
		dashboards.push({ ...renamed(displays.dashboards.find((d) => d.id === 'control'), 'control', 'displays'), title: 'Monitors', icon: 'monitor' });
		const actions = [...u.actions];
		for (const m of [...more, displays]) for (const a of m.actions) if (!actions.some((x) => x.id === a.id)) actions.push(a);
		u.dashboards = dashboards;
		u.actions = actions;
		u.tabs = ['home', 'ups.eco', 'sources', 'control', 'events', 'displays'];
		u.service.features = [...new Set([...u.service.features, 'ups', 'display_control'])].sort();
		u.version = 'rich0000000000000';
	}
	if (CONTROL_OFF) u.control = { enabled: false, off_reason: 'held_off' };
	return u;
}

// ---- the stream ---------------------------------------------------------------------------------------

const PROCESS_NAMES = ['firefox', 'Xorg', 'pipewire', 'bash', 'elamond', 'code', 'syncthing', 'sshd', 'systemd', 'kworker/0:1', 'node', 'chromium', 'dbus-daemon', 'gvfsd', 'cron'];

/** A table of processes that moves a little each second. */
function processes(t, sortBy) {
	const rows = PROCESS_NAMES.map((name, i) => {
		const cpu = Math.max(0, Math.round((Math.sin(t / 3000 + i) * 6 + 7 - i * 0.4) * 10) / 10);
		return { key: String(4000 + i), level: cpu > 22.5 ? 'critical' : cpu > 18.75 ? 'warning' : 'normal', f: { pid: 4000 + i, name, cpu, mem: (i + 1) * 73_400_320, cmd: `/usr/bin/${name} --example`, start: 1_700_000_100 + i, gpu: 0, io: i * 1024 } };
	});
	const field = sortBy || 'cpu';
	return rows.sort((a, b) => (b.f[field] ?? 0) - (a.f[field] ?? 0));
}

/** Points of a graph over `range` seconds back from `t`, one a second. */
function history(series, rangeS, t, base) {
	const n = Math.min(rangeS, 600);
	const ts = Array.from({ length: n }, (_, i) => t - (n - 1 - i) * 1000);
	const v = Array.from({ length: series }, (_, k) => ts.map((x) => Math.round((base[k] ?? 50) * (1 + 0.25 * Math.sin(x / 20000 + k)) * 10) / 10));
	return { t: ts, v };
}

/** Each widget's data and its graphs' histories, from the fixtures, for `dashboard` at `t`. */
function lineFor(dashboard, t, first, stream) {
	const header = fx('stream-data.json').widgets['header/power'];
	const shift = (from) => t - from;
	let widgets = {};
	let base = {};
	switch (dashboard) {
		case 'home':
			widgets = fx('stream-data.json').widgets;
			break;
		case 'detail': {
			widgets = fx('stream-data-detail.json').widgets;
			const rows = processes(t, stream.sort['detail/procs']);
			widgets['detail/procs'] = { rows, total: rows.length };
			break;
		}
		case 'ups.eco': {
			const d = fx('0.7/stream-ups.json');
			widgets = d.widgets;
			const s = shift(d.t);
			widgets['ups.eco/power'].segments = widgets['ups.eco/power'].segments.map((x) => ({ ...x, from: x.from + s, to: x.to + s }));
			base = { 'ups.eco/watts': [0, 210] };
			break;
		}
		case 'sources': {
			const d = fx('0.8/stream-sources.json');
			widgets = d.widgets;
			break;
		}
		case 'control':
			widgets = { 'control/links': fx('stream-data.json').widgets['home/links'] };
			break;
		case 'journal':
		case 'unit_log.example':
		case 'events':
			break;
		case 'displays':
			widgets = keysRenamed(fx('0.10/stream-displays.json').widgets, 'control', 'displays');
			break;
		default:
			break;
	}
	// Logs: their first state a reset, then a line now and then; a filter starts them anew.
	const logs = { 'sources/journal': 'journal', 'sources/app_log': 'app', 'journal/system': 'journal', 'unit_log.example/log': 'app', 'events/events': 'events' };
	const dash = ui().dashboards.find((d) => d.id === dashboard);
	const all = (ws) => ws.flatMap((w) => [w, ...(w.widgets ? all(w.widgets) : [])]);
	for (const w of dash ? all(dash.widgets) : []) {
		if (w.type !== 'log') continue;
		const kind = logs[w.id] || 'journal';
		const filter = stream.log[w.id] || null;
		const order = ['debug', 'info', 'notice', 'warning', 'error', 'critical'];
		const least = order.indexOf((filter && filter.level) || w.level || 'info');
		const match = (l) => order.indexOf(l.level || 'info') >= least && (!filter || !filter.search || l.text.toLowerCase().includes(filter.search.toLowerCase()));
		if (first || stream.reset.has(w.id)) {
			stream.reset.delete(w.id);
			const src = kind === 'events' ? fx('0.9/stream-events.json').widgets['events/events'].lines : kind === 'app' ? fx('0.8/stream-sources.json').widgets['sources/app_log'].lines : fx('0.8/log.json').lines.concat(fx('0.8/stream-log-next.json').widgets['journal/system'].lines);
			const s = t - 1_700_000_005_123;
			const lines = src.map((l) => ({ ...l, t: l.t + s, c: l.c + s * 1000 })).filter(match);
			widgets[w.id] = { lines, reset: true, ...(kind === 'journal' ? { note: "Only this user's journal: add the user to systemd-journal to read the system's." } : {}) };
		} else {
			const fresh = Math.floor(t / 1000) % 4 === 0 ? [{ c: t * 1000, t, level: ['info', 'warning', 'error'][Math.floor(t / 4000) % 3], unit: 'example.service', id: 'example', text: `Request ${Math.floor(t / 1000) % 1000} served` }] : [];
			widgets[w.id] = { lines: fresh.filter(match) };
		}
	}
	// Values that move: the graphs, the CPU and the temperature.
	const wobble = (v, k = 0) => (typeof v === 'number' ? Math.round(v * (1 + 0.2 * Math.sin(t / 4000 + k)) * 10) / 10 : v);
	for (const [id, s] of Object.entries(widgets)) {
		if (Array.isArray(s.v)) s.v = s.v.map((x, k) => wobble(x, k));
		else if (id === 'home/cpu' || id === 'home/temp' || id === 'home/gpus.nvidia') s.v = wobble(s.v);
	}
	const line = { type: 'data', t, dashboard, widgets: { ...widgets, 'header/power': header } };
	if (first && dash) {
		line.history = {};
		for (const w of all(dash.widgets)) {
			if (w.type === 'graph' || (w.type === 'value' && w.spark)) {
				const series = w.type === 'graph' ? (w.series || [{}]).length : 1;
				const now = widgets[w.id] ? (Array.isArray(widgets[w.id].v) ? widgets[w.id].v : [widgets[w.id].v]) : [];
				line.history[w.id] = history(series, w.range || 600, t - 1000, base[w.id] || now.map((x) => (typeof x === 'number' && x > 0 ? x : 40)));
			}
		}
	}
	return line;
}

/** Open streams, by id. */
const streams = new Map();
let streamCount = 0;

function openStream(req, res, url) {
	const u = ui();
	const id = `s${(++streamCount).toString(16).padStart(7, '0')}`;
	const asked = url.searchParams.get('dashboard');
	const stream = { dashboard: u.dashboards.some((d) => d.id === asked) ? asked : u.tabs[0], sort: {}, log: {}, reset: new Set(), first: true, send: null };
	streams.set(id, stream);
	res.writeHead(200, { 'Content-Type': 'application/x-ndjson', 'Cache-Control': 'no-store' });
	res.write(JSON.stringify({ type: 'open', stream: id, ui: u.version }) + '\n');
	stream.send = () => {
		res.write(JSON.stringify(lineFor(stream.dashboard, Date.now(), stream.first, stream)) + '\n');
		stream.first = false;
	};
	stream.send();
	const timer = setInterval(stream.send, 1000);
	req.on('close', () => {
		clearInterval(timer);
		streams.delete(id);
	});
}

// ---- the long history, older lines, a cover ------------------------------------------------------------

const RANGES = { '1h': [3_600_000, 10_000], '24h': [86_400_000, 120_000], '7d': [604_800_000, 900_000], '30d': [2_592_000_000, 3_600_000], '1y': [31_536_000_000, 43_200_000], all: [63_072_000_000, 86_400_000] };

function longHistory(url) {
	const widget = url.searchParams.get('widget');
	const range = url.searchParams.get('range');
	const w = ui().dashboards.flatMap((d) => d.widgets.flatMap((x) => [x, ...(x.widgets || [])])).find((x) => x.id === widget && (x.type === 'graph' || x.type === 'timeline'));
	if (!w) return [404, { error: 'no_such_widget' }];
	if (!RANGES[range]) return [400, { error: 'bad_param', message: 'range' }];
	const [span, step] = RANGES[range];
	const now = Date.now();
	const to = Math.floor(now / step) * step;
	if (w.type === 'timeline') {
		const d = fx('0.7/history-timeline.json');
		const s = now - d.to;
		const segments = d.segments.map((x) => ({ ...x, from: x.from + s, to: x.to + s })).filter((x) => x.to > now - span);
		if (segments.length) segments[0].from = Math.max(segments[0].from, now - span);
		return [200, { widget, range, from: now - span, to: now, segments }];
	}
	const n = Math.round(span / step);
	const t = Array.from({ length: n }, (_, i) => to - (n - i) * step);
	const series = (w.series || [{}]).length;
	const v = Array.from({ length: series }, (_, k) => t.map((x, i) => (i % 97 === 13 ? null : Math.round((40 + 30 * Math.sin(x / (span / 9) + k) + 10 * Math.sin(x / (span / 61))) * 10) / 10)));
	return [200, { widget, range, from: t[0], to, step_ms: step, t, v }];
}

function olderLines(url) {
	const widget = url.searchParams.get('widget');
	if (widget === 'events/events') return [200, fx('0.9/log-events.json')];
	const before = Number(url.searchParams.get('before')) || Date.now() * 1000;
	const lines = Array.from({ length: 20 }, (_, i) => {
		const c = before - (20 - i) * 37_000_000;
		return { c, t: Math.floor(c / 1000), level: ['info', 'notice', 'warning', 'error'][i % 4], unit: 'example.service', id: 'example', text: `An older line, ${i + 1} of this page` };
	});
	return [200, { widget, lines, before: before - 21 * 37_000_000 }];
}

const COVER = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#8b5cf6"/><stop offset="1" stop-color="#0ea5e9"/></linearGradient></defs><rect width="120" height="120" fill="url(#g)"/><circle cx="60" cy="60" r="34" fill="none" stroke="#fff" stroke-width="6" opacity=".8"/><circle cx="60" cy="60" r="8" fill="#fff"/></svg>`;

// ---- actions ---------------------------------------------------------------------------------------------

function runAction(id, b) {
	const u = ui();
	const a = u.actions.find((x) => x.id === id);
	if (!a) return [404, { error: 'no_such_action' }];
	if (!u.control.enabled) return [403, { error: 'control_disabled' }];
	const params = b.params || {};
	// A station's command that cuts power, and the last screen turned off, are typed in.
	const typed = (a.params || []).flatMap((p) => (p.choices || []).filter((c) => c.value === params[p.id] && c.confirm_text));
	if (typed.length && b.confirm_text !== typed[0].confirm_text) return [403, { error: 'confirm_required', message: typed[0].confirm_text }];
	if (/^display\.(output|dpms)\./.test(id) && params.state === 'off') {
		const name = id.split('.').slice(2).join('.');
		if (b.confirm_text !== name) return [403, { error: 'confirm_required', message: name }];
	}
	if (id === 'backup' || id === 'mode' || id === 'fan') return [200, fx('action-result.json')];
	if (id.startsWith('nut.') || id.startsWith('display.')) return [200, { ok: true, output: 'done' }];
	if (id === 'process.signal' && params.signal === 'kill') return [502, { error: 'failed', message: 'not_permitted: a protected process' }];
	return [200, { ok: true }];
}

// ---- the admin API, as the other parts of the page use it ------------------------------------------------

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
		res.writeHead(200, { 'Content-Type': types[extname(file)] || 'application/octet-stream', ...PAGE_HEADERS });
		return res.end(readFileSync(file));
	}
	if (p === '/api/v1/hello') return json(res, 200, fx('0.11/hello.json'));
	if (p === '/api/v1/pair' && req.method === 'POST') {
		const b = await body(req);
		if (b.code === '000000') return json(res, 403, { error: 'no_pairing' });
		if (b.code !== '123456' && b.code !== '111111') return json(res, 403, { error: 'wrong_code' });
		// The person at elamond pair answers: y for 123456, n for 111111.
		await sleep(PAIR_WAIT_MS);
		if (b.code === '111111') return json(res, 403, { error: 'pair_declined' });
		return json(res, 200, fx('0.11/pair-web.json').response.body, { 'Set-Cookie': 'elamon-mock=1; Path=/; HttpOnly; SameSite=Strict' });
	}
	if (!paired(req)) return json(res, 401, { error: 'unauthorized' });
	if (p === '/api/v1/ui') return json(res, 200, ui());
	if (p === '/api/v1/unpair') return json(res, 200, { ok: true }, { 'Set-Cookie': 'elamon-mock=; Path=/; Max-Age=0' });
	if (p === '/api/v1/stream') return openStream(req, res, url);
	if (p.startsWith('/api/v1/stream/')) {
		const s = streams.get(decodeURIComponent(p.slice('/api/v1/stream/'.length)));
		if (!s) return json(res, 404, { error: 'no_such_stream' });
		const b = await body(req);
		if (b.dashboard && !ui().dashboards.some((d) => d.id === b.dashboard)) return json(res, 404, { error: 'no_such_dashboard' });
		for (const [k, v] of Object.entries(b.sort || {})) {
			if (v == null) delete s.sort[k];
			else s.sort[k] = v;
		}
		for (const [k, v] of Object.entries(b.log || {})) {
			if (v == null) delete s.log[k];
			else s.log[k] = v;
			s.reset.add(k);
		}
		if (b.dashboard && b.dashboard !== s.dashboard) {
			s.dashboard = b.dashboard;
			s.first = true;
		}
		setTimeout(() => s.send && s.send(), 50);
		return json(res, 200, { ok: true });
	}
	if (p === '/api/v1/snapshot') {
		const d = lineFor(url.searchParams.get('dashboard') || ui().tabs[0], Date.now(), url.searchParams.get('history') !== '0', { sort: {}, log: {}, reset: new Set() });
		delete d.type;
		return json(res, 200, d);
	}
	if (p === '/api/v1/history') return json(res, ...longHistory(url));
	if (p === '/api/v1/log') return json(res, ...olderLines(url));
	if (p === '/api/v1/media/art') {
		if (!RICH) return json(res, 404, { error: 'no_art' });
		res.writeHead(200, { 'Content-Type': 'image/svg+xml', 'Cache-Control': 'no-store' });
		return res.end(COVER);
	}
	if (p.startsWith('/api/v1/actions/')) {
		const b = req.method === 'POST' ? await body(req) : {};
		await sleep(300);
		return json(res, ...runAction(decodeURIComponent(p.slice('/api/v1/actions/'.length)), b));
	}
	if (p.startsWith('/api/v1/admin/')) {
		const route = admin[`${req.method} ${p.slice('/api/v1/admin/'.length)}`];
		const b = req.method === 'POST' ? await body(req) : {};
		return route ? json(res, 200, route(b)) : json(res, 404, { error: 'not_found' });
	}
	json(res, 404, { error: 'not_found' });
}).listen(port, '127.0.0.1', () => console.log(`mock elamond: http://127.0.0.1:${port}/ (code 123456)${PAIRED ? ', paired' : ''}${RICH ? ', rich' : ''}${CONTROL_OFF ? ', control off' : ''}`));
