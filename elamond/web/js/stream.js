// The live stream of the dashboard on screen (`GET /api/v1/stream`): one long request per page,
// moved to another dashboard, table order or log filter by `POST /api/v1/stream/{id}` without
// reconnecting; opened again after a drop, 1, 2, 5, then every 10 seconds; closed while the page is
// hidden (the service reads nothing for nobody) and opened again when it shows.
import { api, open, ApiError } from './api.js';
import { objects } from './ndjson.js';

/** Pauses before each new try after a drop, the last one repeated. */
export const RETRY_MS = [1000, 2000, 5000, 10000];
/** No line for this long (or 4 intervals, if longer): the connection is taken as gone. */
export const SILENCE_MS = 15000;
const MISSED_LINES = 4;
/** Changes refused in a row before what is wanted is left as it is sent. */
const MAX_REFUSALS = 2;

/** The pause before try `attempt` (from 0). @param {number} attempt */
export function retryDelay(attempt) {
	return RETRY_MS[Math.min(Math.max(attempt, 0), RETRY_MS.length - 1)];
}

/**
 * Whether "no connection" shows after `status`, having shown before or not: from a drop until the
 * stream is live again, through the tries between; not while it is out of sight or stopped.
 * @param {boolean} was @param {Status} status
 */
export function offlineAfter(was, status) {
	if (status === 'down') return true;
	if (status === 'connecting') return was;
	return false;
}

/** How long without a line before the connection is taken as gone, at a reading every `intervalMs`. @param {number} intervalMs */
export function silenceMs(intervalMs) {
	return Math.max(SILENCE_MS, MISSED_LINES * (Number.isFinite(intervalMs) ? intervalMs : 0));
}

/**
 * What the stream is asked to send: the dashboard on screen, tables in another order than their
 * own (field by table id), logs with another filter than their own (by log id).
 * @typedef {{ dashboard: string, sort: Record<string, string>, log: Record<string, { level: string, search?: string }> }} Want
 *
 * @typedef {{ kind: 'open', stream: string, ui: string | null } | { kind: 'data', data: import('./types.js').Data }
 *   | { kind: 'ui', ui: string, endpoints: string[] } | { kind: 'other' }} Line
 */

/** A line of the stream by its `type`; what isn't one the page reads is `other`. @param {any} obj @returns {Line} */
export function readLine(obj) {
	if (!obj || typeof obj !== 'object') return { kind: 'other' };
	if (obj.type === 'open' && typeof obj.stream === 'string') return { kind: 'open', stream: obj.stream, ui: typeof obj.ui === 'string' ? obj.ui : null };
	if (obj.type === 'data' && typeof obj.dashboard === 'string' && obj.widgets && typeof obj.widgets === 'object') {
		const { type, ...data } = obj;
		return { kind: 'data', data };
	}
	if (obj.type === 'ui' && typeof obj.ui === 'string') return { kind: 'ui', ui: obj.ui, endpoints: Array.isArray(obj.endpoints) ? obj.endpoints : [] };
	return { kind: 'other' };
}

/** @param {any} a @param {any} b */
function sameFilter(a, b) {
	if (a == null || b == null) return a == b;
	return a.level === b.level && (a.search || '') === (b.search || '');
}

/**
 * What a stream told `sent` must be told to send `want`, as one change, or null when it sends
 * that already. A table goes back to its own order with `null`; a log's filter is told only with
 * its dashboard on the stream (`logsOf` names a dashboard's logs), and again when the stream moves
 * to that dashboard, as its lines start anew then.
 * @param {Want} sent @param {Want} want @param {(dashboard: string) => string[]} logsOf
 * @returns {{ dashboard?: string, sort?: Record<string, string | null>, log?: Record<string, any> } | null}
 */
export function changeFor(sent, want, logsOf) {
	/** @type {{ dashboard?: string, sort?: Record<string, string | null>, log?: Record<string, any> }} */
	const body = {};
	const moved = want.dashboard !== sent.dashboard;
	if (moved) body.dashboard = want.dashboard;
	/** @type {Record<string, string | null>} */
	const sort = {};
	for (const [k, v] of Object.entries(want.sort)) if (sent.sort[k] !== v) sort[k] = v;
	for (const k of Object.keys(sent.sort)) if (!(k in want.sort)) sort[k] = null;
	if (Object.keys(sort).length) body.sort = sort;
	/** @type {Record<string, any>} */
	const log = {};
	for (const id of logsOf(want.dashboard)) {
		const w = want.log[id] ?? null;
		if (!sameFilter(w, sent.log[id] ?? null) || (moved && w)) log[id] = w;
	}
	if (Object.keys(log).length) body.log = log;
	return Object.keys(body).length ? body : null;
}

/** `sent` once the stream took `body`. @param {Want} sent @param {ReturnType<typeof changeFor>} body @returns {Want} */
export function applied(sent, body) {
	if (!body) return sent;
	const sort = { ...sent.sort };
	for (const [k, v] of Object.entries(body.sort || {})) {
		if (v == null) delete sort[k];
		else sort[k] = v;
	}
	const log = { ...sent.log };
	for (const [k, v] of Object.entries(body.log || {})) {
		if (v == null) delete log[k];
		else log[k] = v;
	}
	return { dashboard: body.dashboard ?? sent.dashboard, sort, log };
}

/**
 * What was asked of the stream that `dashboards` (a new `ui`'s) still have: a table's order by a
 * field the table still offers (its own `sort` or one of `sorts`), a log's filter while the log is
 * there. `gone` are the widgets no longer there at all: the stream is told nothing of them.
 * @param {{ sort: Record<string, string>, log: Record<string, any> }} asked
 * @param {{ widgets?: any[] }[]} dashboards
 * @returns {{ sort: Record<string, string>, log: Record<string, any>, gone: string[] }}
 */
export function keptAsked(asked, dashboards) {
	/** @type {Map<string, any>} */
	const tables = new Map();
	/** @type {Set<string>} */
	const logs = new Set();
	/** @param {any[] | undefined} list */
	const walk = (list) => {
		for (const w of Array.isArray(list) ? list : []) {
			if (!w || typeof w.id !== 'string') continue;
			if (w.type === 'table') tables.set(w.id, w);
			else if (w.type === 'log') logs.add(w.id);
			walk(w.widgets);
		}
	};
	for (const d of dashboards || []) walk(d && d.widgets);
	/** @type {Record<string, string>} */
	const sort = {};
	/** @type {Record<string, any>} */
	const log = {};
	/** @type {string[]} */
	const gone = [];
	for (const [id, field] of Object.entries(asked.sort)) {
		const t = tables.get(id);
		if (!t) gone.push(id);
		else if ((t.sort && t.sort.field === field) || (Array.isArray(t.sorts) && t.sorts.includes(field))) sort[id] = field;
	}
	for (const [id, filter] of Object.entries(asked.log)) {
		if (logs.has(id)) log[id] = filter;
		else gone.push(id);
	}
	return { sort, log, gone };
}

/** @param {Want} w @returns {Want} */
function copy(w) {
	return { dashboard: w.dashboard, sort: { ...w.sort }, log: { ...w.log } };
}

/**
 * `refused` is handed the change that was refused whole, to drop what it asked for.
 * @typedef {'connecting' | 'live' | 'down' | 'paused' | 'stopped'} Status
 * @typedef {{ line?: (line: Line) => void, status?: (status: Status) => void, unauthorized?: () => void,
 *   refused?: (e: ApiError, body: NonNullable<ReturnType<typeof changeFor>>) => void }} Handlers
 */

/** The stream of one page: start it once, tell it what to send, stop it when the page goes. */
export class Stream {
	/**
	 * @param {Want} want what to send from the start
	 * @param {{ widgets: string, intervalMs: () => number, logsOf: (dashboard: string) => string[] }} opts
	 * `intervalMs` is read at each line, so a profile's new interval counts from the next one
	 * @param {Handlers} handlers
	 */
	constructor(want, opts, handlers) {
		this.want = copy(want);
		/** What the open stream sends: told since it opened. */
		this.sent = copy(want);
		this.opts = opts;
		this.handlers = handlers;
		/** @type {string | null} */
		this.id = null;
		/** @type {Status} */
		this.status = 'stopped';
		this.attempt = 0;
		this.run = 0;
		this.telling = false;
		this.again = false;
		/** Changes refused in a row: past a few, what is wanted stays as it is sent. */
		this.refusals = 0;
		/** @type {AbortController | null} */
		this.ctrl = null;
		/** @type {ReturnType<typeof setTimeout> | null} */
		this.timer = null;
		/** @type {ReturnType<typeof setTimeout> | null} */
		this.watchdog = null;
		this.onVisibility = () => (document.hidden ? this.pause() : this.resume());
	}

	start() {
		document.addEventListener('visibilitychange', this.onVisibility);
		if (document.hidden) this.setStatus('paused');
		else this.connect();
	}

	stop() {
		document.removeEventListener('visibilitychange', this.onVisibility);
		this.halt();
		this.setStatus('stopped');
	}

	/** The page went out of sight: nothing is read for nobody. */
	pause() {
		if (this.status === 'stopped' || this.status === 'paused') return;
		this.halt();
		this.setStatus('paused');
	}

	resume() {
		if (this.status !== 'paused') return;
		this.attempt = 0;
		this.connect();
	}

	/** Asks for another dashboard, order or filter: the whole of what is wanted. @param {Partial<Want>} want */
	set(want) {
		this.want = { ...this.want, ...want };
		this.tell();
	}

	/** Widgets no longer there (a new `ui`): their orders and filters are dropped, and the stream told nothing of them. @param {string[]} ids */
	forget(ids) {
		for (const w of [this.want, this.sent]) {
			for (const id of ids) {
				delete w.sort[id];
				delete w.log[id];
			}
		}
	}

	halt() {
		this.run++;
		this.id = null;
		if (this.ctrl) this.ctrl.abort();
		this.ctrl = null;
		if (this.timer) clearTimeout(this.timer);
		this.timer = null;
		if (this.watchdog) clearTimeout(this.watchdog);
		this.watchdog = null;
	}

	/** @param {Status} s */
	setStatus(s) {
		if (this.status === s) return;
		this.status = s;
		this.handlers.status?.(s);
	}

	connect() {
		this.halt();
		const run = this.run;
		const ctrl = new AbortController();
		this.ctrl = ctrl;
		this.setStatus('connecting');
		this.read(run, ctrl).then((again) => {
			if (run !== this.run) return;
			this.id = null;
			if (this.watchdog) clearTimeout(this.watchdog);
			if (!again) return;
			this.setStatus('down');
			this.timer = setTimeout(() => {
				if (run === this.run) this.connect();
			}, retryDelay(this.attempt++));
		});
	}

	/** Reads one stream to its end; whether to try again. @param {number} run @param {AbortController} ctrl @returns {Promise<boolean>} */
	async read(run, ctrl) {
		const kick = () => {
			if (this.watchdog) clearTimeout(this.watchdog);
			this.watchdog = setTimeout(() => ctrl.abort(), silenceMs(this.opts.intervalMs()));
		};
		try {
			kick();
			const dashboard = this.want.dashboard;
			const res = await open(`/api/v1/stream?dashboard=${encodeURIComponent(dashboard)}&widgets=${this.opts.widgets}`, ctrl.signal);
			if (run !== this.run || !res.body) return run === this.run;
			this.sent = { dashboard, sort: {}, log: {} };
			for await (const obj of objects(res.body)) {
				if (run !== this.run) return false;
				kick();
				const line = readLine(obj);
				if (line.kind === 'open') {
					this.id = line.stream;
					this.setStatus('live');
					this.tell();
				} else if (line.kind === 'data') {
					this.attempt = 0;
				}
				this.handlers.line?.(line);
			}
		} catch (e) {
			if (run !== this.run) return false;
			if (e instanceof ApiError && e.status === 401) {
				this.stop();
				this.handlers.unauthorized?.();
				return false;
			}
		}
		return run === this.run;
	}

	/** Tells the open stream what it doesn't send yet, one change at a time. */
	async tell() {
		if (!this.id) return;
		if (this.telling) {
			this.again = true;
			return;
		}
		const body = changeFor(this.sent, this.want, this.opts.logsOf);
		if (!body) return;
		const id = this.id;
		const run = this.run;
		this.telling = true;
		try {
			await api(`/api/v1/stream/${encodeURIComponent(id)}`, { body });
			if (run === this.run) this.sent = applied(this.sent, body);
			this.refusals = 0;
		} catch (e) {
			// A change to a stream that has since been opened again says nothing of the new one,
			// which is told below what is wanted, as it asked meanwhile.
			if (run === this.run) this.notTold(e, body);
		} finally {
			this.telling = false;
		}
		if (this.again) {
			this.again = false;
			this.tell();
		}
	}

	/** A change the open stream didn't take. @param {unknown} e @param {NonNullable<ReturnType<typeof changeFor>>} body */
	notTold(e, body) {
		if (e instanceof ApiError && e.status === 401) {
			this.stop();
			this.handlers.unauthorized?.();
		} else if (e instanceof ApiError && e.code === 'no_such_stream') {
			// The stream ended meanwhile: a new one is opened with what is wanted.
			this.connect();
		} else if (e instanceof ApiError) {
			// Refused whole: nothing of it applies, so the orders and filters wanted go back to those
			// sent, and a new dashboard is asked for again on its own.
			this.refusals++;
			const keep = e.code !== 'no_such_dashboard' && this.refusals < MAX_REFUSALS;
			this.want = { ...copy(this.sent), dashboard: keep ? this.want.dashboard : this.sent.dashboard };
			this.again = keep && this.want.dashboard !== this.sent.dashboard;
			this.handlers.refused?.(e, body);
		}
		// Anything else: the computer can't be reached, and the stream drops and opens again with what is wanted.
	}
}
