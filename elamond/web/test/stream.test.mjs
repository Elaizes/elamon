import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { readLine, changeFor, applied, retryDelay, silenceMs, SILENCE_MS, keptAsked, Stream } from '../js/stream.js';
import { intervalOf, historyMsOf, DEFAULT_INTERVAL_MS, DEFAULT_HISTORY_S } from '../js/consts.js';

const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/${name}`, import.meta.url), 'utf8'));

test('lines by their type', () => {
	assert.deepEqual(readLine(fx('stream-open.json')), { kind: 'open', stream: 's1a2b3c4', ui: 'b822fda4c6e273d8' });
	const data = readLine(fx('stream-data.json'));
	assert.equal(data.kind, 'data');
	assert.equal(data.data.dashboard, 'home');
	assert.equal('type' in data.data, false);
	assert.ok(data.data.history['home/net']);
	const ui = readLine(fx('stream-ui.json'));
	assert.equal(ui.kind, 'ui');
	assert.equal(ui.ui, '23f29e9f674fb397');
	assert.deepEqual(readLine({ type: 'later' }), { kind: 'other' });
	assert.deepEqual(readLine(null), { kind: 'other' });
	assert.deepEqual(readLine({ type: 'data' }), { kind: 'other' });
});

test('retry after a drop: 1, 2, 5, then every 10 s', () => {
	assert.deepEqual([0, 1, 2, 3, 4, 9].map(retryDelay), [1000, 2000, 5000, 10000, 10000, 10000]);
});

const logsOf = (d) => ({ sources: ['sources/journal', 'sources/app_log'], journal: ['journal/system'] })[d] || [];
const want = (dashboard, sort = {}, log = {}) => ({ dashboard, sort, log });

test('one change: the dashboard, the orders, and the filters of its logs', () => {
	assert.equal(changeFor(want('home'), want('home'), logsOf), null);
	assert.deepEqual(changeFor(want('home'), want('detail'), logsOf), { dashboard: 'detail' });
	const sub = fx('subscribe.json');
	assert.deepEqual(changeFor(want('home'), want('detail', { 'detail/procs': 'mem' }), logsOf), { dashboard: sub.dashboard, sort: sub.sort });
	// Back to its own order.
	assert.deepEqual(changeFor(want('detail', { 'detail/procs': 'mem' }), want('detail'), logsOf), { sort: { 'detail/procs': null } });
	// A log's filter, and one back to its own, as subscribe-log.json has them.
	const sent = want('sources', {}, { 'sources/journal': { level: 'warning' } });
	const asked = want('sources', {}, { 'sources/app_log': { level: 'error', search: 'timeout' } });
	assert.deepEqual(changeFor(sent, asked, logsOf), fx('0.8/subscribe-log.json'));
	// A filter of a log not on the dashboard waits for it; moving there tells it.
	assert.equal(changeFor(want('home'), want('home', {}, { 'journal/system': { level: 'error' } }), logsOf), null);
	assert.deepEqual(changeFor(want('home', {}, { 'journal/system': { level: 'error' } }), want('journal', {}, { 'journal/system': { level: 'error' } }), logsOf), {
		dashboard: 'journal',
		log: { 'journal/system': { level: 'error' } },
	});
});

test('what is sent once a change is taken', () => {
	const sent = applied(want('home', { a: 'x' }, { l: { level: 'error' } }), { dashboard: 'd', sort: { a: null, b: 'y' }, log: { l: null, m: { level: 'info', search: 'q' } } });
	assert.deepEqual(sent, want('d', { b: 'y' }, { m: { level: 'info', search: 'q' } }));
});

/** A body of NDJSON lines, then its end. */
function body(lines) {
	const enc = new TextEncoder();
	return new ReadableStream({
		start(c) {
			for (const l of lines) c.enqueue(enc.encode(JSON.stringify(l) + '\n'));
			c.close();
		},
	});
}

test('a stream: its lines, a change told by its id, and a drop said', async (t) => {
	const listeners = {};
	globalThis.document = { hidden: false, addEventListener: (k, f) => (listeners[k] = f), removeEventListener: () => {} };
	const requests = [];
	globalThis.fetch = async (url, init) => {
		requests.push({ url, method: init.method || 'GET', body: init.body ? JSON.parse(init.body) : null, client: init.headers['Elamon-Client'] });
		if (String(url).startsWith('/api/v1/stream?')) return new Response(body([fx('stream-open.json'), fx('stream-data.json')]), { status: 200 });
		return new Response('{"ok":true}', { status: 200 });
	};
	t.mock.timers.enable({ apis: ['setTimeout'] });
	const lines = [];
	const statuses = [];
	const s = new Stream(want('home'), { widgets: 'value,gauge', intervalMs: () => 1000, logsOf }, { line: (l) => lines.push(l.kind), status: (st) => statuses.push(st) });
	s.start();
	s.set({ dashboard: 'detail', sort: { 'detail/procs': 'mem' } });
	for (let i = 0; i < 20 && !statuses.includes('down'); i++) await new Promise((r) => setImmediate(r));
	assert.deepEqual(lines, ['open', 'data']);
	assert.deepEqual(statuses, ['connecting', 'live', 'down']);
	assert.equal(requests[0].url, '/api/v1/stream?dashboard=home&widgets=value,gauge');
	assert.match(requests[0].client, /^web\//);
	const change = requests.find((r) => r.method === 'POST');
	assert.equal(change.url, '/api/v1/stream/s1a2b3c4');
	assert.deepEqual(change.body, { dashboard: 'detail', sort: { 'detail/procs': 'mem' } });
	// Hidden: closed, and no try until it shows again.
	globalThis.document.hidden = true;
	listeners.visibilitychange();
	assert.equal(s.status, 'paused');
	t.mock.timers.tick(20000);
	assert.equal(requests.filter((r) => r.url.startsWith('/api/v1/stream?')).length, 1);
	globalThis.document.hidden = false;
	listeners.visibilitychange();
	for (let i = 0; i < 20 && requests.filter((r) => r.url.startsWith('/api/v1/stream?')).length < 2; i++) await new Promise((r) => setImmediate(r));
	// Opened again on the dashboard wanted now.
	assert.equal(requests.filter((r) => r.url.startsWith('/api/v1/stream?'))[1].url, '/api/v1/stream?dashboard=detail&widgets=value,gauge');
	s.stop();
	assert.equal(s.status, 'stopped');
});

test('401 from the stream: back to pairing', async () => {
	globalThis.document = { hidden: false, addEventListener: () => {}, removeEventListener: () => {} };
	globalThis.fetch = async () => new Response('{"error":"unauthorized"}', { status: 401 });
	let told = false;
	const s = new Stream(want('home'), { widgets: 'value', intervalMs: () => 1000, logsOf }, { unauthorized: () => (told = true) });
	s.start();
	for (let i = 0; i < 20 && !told; i++) await new Promise((r) => setImmediate(r));
	assert.equal(told, true);
	assert.equal(s.status, 'stopped');
});

test("the watchdog waits by the interval of the ui there is now", () => {
	const ui = fx('ui.json');
	assert.equal(intervalOf(ui), ui.profile.interval_ms);
	assert.equal(intervalOf({ ...ui, profile: { ...ui.profile, interval_ms: 9000 } }), 9000);
	assert.equal(intervalOf({ service: { interval_ms: 2000 } }), 2000);
	assert.equal(intervalOf(null), DEFAULT_INTERVAL_MS);
	assert.equal(historyMsOf(ui), ui.service.history_seconds * 1000);
	assert.equal(historyMsOf({}), DEFAULT_HISTORY_S * 1000);
	assert.equal(silenceMs(1000), SILENCE_MS);
	assert.equal(silenceMs(9000), 4 * 9000);
	// Read at each line: a profile's new interval counts without a new stream.
	let interval = 1000;
	const s = new Stream({ dashboard: 'home', sort: {}, log: {} }, { widgets: 'value', intervalMs: () => interval, logsOf: () => [] }, {});
	assert.equal(silenceMs(s.opts.intervalMs()), SILENCE_MS);
	interval = 10000;
	assert.equal(silenceMs(s.opts.intervalMs()), 40000);
});

test("what was asked of the stream is kept only while ui has its table, field and log", () => {
	const ui = fx('ui.json');
	const logs = fx('0.8/ui-sources.json');
	const dashboards = [...ui.dashboards, ...logs.dashboards];
	const asked = {
		sort: { 'detail/procs': 'mem', 'detail/gone': 'mem', 'home/old': 'cpu' },
		log: { 'sources/journal': { level: 'error' }, 'sources/old_log': { level: 'info' } },
	};
	const kept = keptAsked(asked, dashboards);
	assert.deepEqual(kept.sort, { 'detail/procs': 'mem' });
	assert.deepEqual(kept.log, { 'sources/journal': { level: 'error' } });
	assert.deepEqual(kept.gone.sort(), ['detail/gone', 'home/old', 'sources/old_log']);
	// A field the table no longer offers goes, the table staying: back to its own order.
	const fewer = dashboards.map((d) => ({ ...d, widgets: d.widgets.map((w) => (w.id === 'detail/procs' ? { ...w, sorts: ['cpu'] } : w)) }));
	assert.deepEqual(keptAsked(asked, fewer).sort, {});
	assert.ok(!keptAsked(asked, fewer).gone.includes('detail/procs'));
});

test('a refused change is handed back, and a widget gone is told nothing', async () => {
	globalThis.document = { hidden: false, addEventListener: () => {}, removeEventListener: () => {} };
	const posts = [];
	let close;
	globalThis.fetch = async (url, init) => {
		if (String(url).startsWith('/api/v1/stream?')) {
			const enc = new TextEncoder();
			return new Response(new ReadableStream({ start: (c) => { c.enqueue(enc.encode(JSON.stringify(fx('stream-open.json')) + '\n')); close = () => c.close(); } }), { status: 200 });
		}
		const b = JSON.parse(init.body);
		posts.push(b);
		if (b.sort && b.sort['detail/bad']) return new Response(JSON.stringify({ error: 'bad_sort', message: 'detail/bad' }), { status: 400 });
		return new Response('{"ok":true}', { status: 200 });
	};
	const refused = [];
	const s = new Stream(want('home'), { widgets: 'value', intervalMs: () => 1000, logsOf }, { refused: (e, body) => refused.push([e.code, e.detail, body]) });
	s.start();
	for (let i = 0; i < 20 && !s.id; i++) await new Promise((r) => setImmediate(r));
	s.set({ sort: { 'detail/procs': 'mem', 'detail/bad': 'x' } });
	for (let i = 0; i < 20 && !refused.length; i++) await new Promise((r) => setImmediate(r));
	assert.deepEqual(refused, [['bad_sort', 'detail/bad', { sort: { 'detail/procs': 'mem', 'detail/bad': 'x' } }]]);
	// Refused whole: nothing of it is taken as sent.
	assert.deepEqual(s.sent.sort, {});
	// The page asks again without the refused one.
	s.set({ sort: { 'detail/procs': 'mem' } });
	for (let i = 0; i < 20 && posts.length < 2; i++) await new Promise((r) => setImmediate(r));
	assert.deepEqual(posts[1], { sort: { 'detail/procs': 'mem' } });
	for (let i = 0; i < 20 && s.telling; i++) await new Promise((r) => setImmediate(r));
	// A table gone with a new ui: dropped on both sides, and no `null` sent for it.
	s.forget(['detail/procs']);
	s.set({ sort: {} });
	for (let i = 0; i < 5; i++) await new Promise((r) => setImmediate(r));
	assert.equal(posts.length, 2);
	s.stop();
	close && close();
});
