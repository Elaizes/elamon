// Pairing, the computer's tabs and its dashboards, drawn from `ui` and the stream; every widget
// type in widgets/. The dashboard editor and the admin panel's preview draw with `dashboardView`
// too, so a draft looks as it will.
import { api, ApiError } from './api.js';
import { h, setText, breakAfter } from './dom.js';
import { S, fill } from './strings.js';
import { icon } from './icons.js';
import * as state from './state.js';
import { columns } from './layout.js';
import { onLayout } from './arrange.js';
import { mountFrame, tabsOf } from './shell.js';
import { makeGrid } from './widgets/grid.js';
import { make } from './widgets/index.js';
import { setLevel } from './widgets/common.js';
import { Stream, keptAsked, offlineAfter } from './stream.js';
import { actionRunner } from './actions.js';
import { errorText } from './errors.js';
import { isFingerprint, colonFingerprint, printedFingerprint } from './cert.js';
import * as F from './format.js';
import { intervalOf, historyMsOf, PAIR_WAIT_MS, ADOPT_MS } from './consts.js';

/** Every widget type this page draws, as `ui`, the stream and a preview ask for them: keep it to the types widgets/ has. */
export const DRAWS = 'value,gauge,meter,status,graph,list,bars,table,button,toggle,select,slider,text,group,media,timeline,log';

const P = S.panels;

/**
 * @typedef {import('./types.js').Ui} Ui
 * @typedef {import('./types.js').Ctx} Ctx
 * @typedef {import('./types.js').Widget} Widget
 * @typedef {import('./types.js').Data} Data
 */

// ---- a dashboard -------------------------------------------------------------------------------------

/** Every widget of `widgets`, those in groups too. @param {Widget[]} widgets @returns {Widget[]} */
function allWidgets(widgets) {
	return widgets.flatMap((w) => [w, ...(Array.isArray(w.widgets) ? allWidgets(w.widgets) : [])]);
}

/** Whether `w`, or one in it, acts on the computer: drawn disabled while control is off. @param {Widget} w @returns {boolean} */
function acts(w) {
	return !!w.action || w.type === 'media' || (Array.isArray(w.widgets) && w.widgets.some(acts));
}

/** Control is off on the computer: why, and how to turn it on there. @param {string | null | undefined} reason */
function controlOff(reason) {
	const [text, code, where] =
		reason === 'held_off' ? [P.controlHeldOff, P.controlHeldOffRun, null] : reason === 'settings_unreadable' ? [P.controlUnreadable, P.controlUnreadableRun, null] : [P.controlOffBody, P.controlOffSetting, P.controlOffWhere];
	return h('section', { class: 'card w control-off', role: 'note' },
		h('div', { class: 'w-head' }, icon('lock', 'w-icon'), h('h3', { class: 'w-title' }, P.controlOff)),
		h('p', null, text), h('pre', { class: 'mono code' }, code), where ? h('p', { class: 'muted' }, where) : null);
}

/**
 * One dashboard of `ui`, drawn; `update` takes each `data` for it. Used by the machine view, the
 * admin panel's preview and the dashboard editor. Holds no state of its own outside the view, so
 * several can be mounted at once.
 * @param {Ui} ui @param {string} dashboard @param {import('./types.js').ViewHooks} [hooks]
 * @returns {import('./types.js').DashboardView & { clear: () => void }}
 */
export function dashboardView(ui, dashboard, hooks = {}) {
	const dash = (ui.dashboards || []).find((d) => d.id === dashboard);
	const el = h('section', { class: 'dashboard', 'data-dashboard': dashboard });
	if (!dash) {
		el.append(h('p', { class: 'note' }, S.core.notFound));
		return { el, update() {}, destroy() {}, clear() {} };
	}
	el.setAttribute('aria-label', dash.title || dashboard);
	const live = !hooks.preview && typeof hooks.runAction === 'function';
	const actions = new Map((ui.actions || []).map((a) => [a.id, a]));
	const dashboards = new Set((ui.dashboards || []).map((d) => d.id));
	/** @type {import('./widgets/common.js').Env} */
	const env = {
		ui,
		dashboard,
		isTab: (ui.tabs || []).includes(dashboard),
		live,
		action: (id) => (id ? actions.get(id) || null : null),
		controlOn: !ui.control || ui.control.enabled !== false,
		intervalMs: intervalOf(ui),
		historyMs: historyMsOf(ui),
		memory: hooks.memory || new Map(),
		asked: { sort: { ...(hooks.asked && hooks.asked.sort) }, log: { ...(hooks.asked && hooks.asked.log) } },
		make,
		canOpen: (id) => !!id && dashboards.has(id),
		open: (id) => {
			if (live && hooks.open && dashboards.has(id)) hooks.open(id);
		},
		run: (a, p, w, phrase) => (live && hooks.runAction ? hooks.runAction(a, p, w, phrase) : Promise.resolve(null)),
		change: (body) => {
			if (live && hooks.change) hooks.change(body);
		},
	};
	const widgets = Array.isArray(dash.widgets) ? dash.widgets : [];
	if (!env.controlOn && widgets.some(acts)) el.append(controlOff(ui.control && ui.control.off_reason));
	const grid = makeGrid(widgets, env);
	if (!widgets.length && !hooks.preview) el.append(h('p', { class: 'note' }, P.empty));
	el.append(grid.el);
	let cols = 0;
	// The columns for the grid's width and the layout's pick (html's data-columns, arrange.js).
	const relayout = () => {
		const c = columns(el.clientWidth || window.innerWidth, /** @type {any} */ (document.documentElement.dataset.columns || 'auto'));
		if (c === cols) return;
		cols = c;
		grid.layout(c);
	};
	const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(relayout) : null;
	ro?.observe(el);
	const layoutOff = onLayout(relayout);
	relayout();
	return {
		el,
		update(data) {
			if (!data || (data.dashboard && data.dashboard !== dashboard)) return;
			grid.update(data);
		},
		clear: () => grid.clear(),
		destroy() {
			ro?.disconnect();
			layoutOff();
			grid.destroy();
		},
	};
}

/** The header dashboard's widgets as chips under the computer's name, each its text in its level's colour. @param {Ui} ui */
function headerChips(ui) {
	const dash = ui.header ? (ui.dashboards || []).find((d) => d.id === ui.header) : null;
	const widgets = dash ? allWidgets(dash.widgets || []).filter((w) => w.type !== 'group') : [];
	const el = h('ul', { class: 'header-chips', 'aria-label': P.chips });
	const chips = widgets.map((w) => {
		const text = h('span', { class: 'pill-text' });
		const dot = h('span', { class: 'dot', 'aria-hidden': 'true' });
		const li = h('li', { class: 'pill chip-pill' }, dot, text);
		li.hidden = true;
		el.append(li);
		return { w, li, text, dot };
	});
	/** @param {Data | null} data */
	const show = (data) => {
		for (const { w, li, text, dot } of chips) {
			const s = data && data.widgets ? data.widgets[w.id] : undefined;
			const t = s ? (typeof s.text === 'string' ? s.text : F.format(s.v, w.format, w.unit)) : null;
			const shown = t ?? (typeof w.text === 'string' && data ? w.text : null);
			li.hidden = !shown || !shown.trim();
			setText(text, shown || '');
			const level = (s && s.level) || w.level;
			setLevel(li, level, s && s.blend);
			dot.hidden = !level;
			if (w.label) li.title = w.label;
		}
	};
	show(null);
	return { el, update: show, clear: () => show(null) };
}

// ---- pairing ---------------------------------------------------------------------------------------------

/**
 * The pairing screen. @param {HTMLElement} root @param {Ctx} ctx
 * @param {string | null} code a code from the page's address (`#pair=123456`), to fill in
 */
export function mountPair(root, ctx, code) {
	const fp = ctx.hello && ctx.hello.fingerprint;
	const codeField = h('input', { id: 'pair-code', name: 'code', inputmode: 'numeric', pattern: '[0-9]{6}', maxlength: '6', autocomplete: 'one-time-code', required: true, 'aria-describedby': 'pair-code-hint', class: 'pair-code mono' });
	codeField.value = code || '';
	const deviceField = h('input', { id: 'pair-device', name: 'device', maxlength: '64', autocomplete: 'off', 'aria-describedby': 'pair-device-hint' });
	const submit = h('button', { type: 'submit', class: 'primary' }, P.pairSubmit);
	const waiting = h('p', { class: 'note pair-waiting', role: 'status' }, h('span', { class: 'spinner', 'aria-hidden': 'true' }), ' ', P.pairWaiting);
	waiting.hidden = true;
	const error = h('p', { class: 'note critical', role: 'alert' });
	error.hidden = true;
	const form = h('form', { class: 'pair-form', novalidate: true },
		h('label', { for: 'pair-code' }, P.pairCode), codeField, h('p', { id: 'pair-code-hint', class: 'hint muted' }, P.pairCodeHint),
		h('label', { for: 'pair-device' }, P.pairDevice), deviceField, h('p', { id: 'pair-device-hint', class: 'hint muted' }, P.pairDeviceHint),
		h('div', { class: 'pair-actions' }, submit), waiting, error);
	const certificate = isFingerprint(fp)
		? h('section', { class: 'pair-certificate', 'aria-labelledby': 'pair-cert-title' },
				h('h2', { id: 'pair-cert-title' }, P.pairCertificate),
				h('p', { class: 'mono fingerprint' }, breakAfter(colonFingerprint(fp), ':')),
				h('p', { class: 'muted' }, fill(P.pairPrinted, printedFingerprint(fp))),
				h('p', null, P.pairCompare))
		: h('p', { class: 'note warning' }, P.pairNoFingerprint);
	const name = ctx.hello && ctx.hello.name;
	root.append(h('main', { class: 'pair card' }, h('h1', null, P.pairTitle, name ? h('span', { class: 'muted' }, ` · ${name}`) : null), h('p', null, P.pairIntro), form, certificate));
	let ctrl = /** @type {AbortController | null} */ (null);
	form.addEventListener('submit', async (e) => {
		e.preventDefault();
		const digits = codeField.value.trim();
		if (!/^\d{6}$/.test(digits)) {
			error.hidden = false;
			setText(error, P.pairCodeInvalid);
			codeField.setAttribute('aria-invalid', 'true');
			codeField.focus();
			return;
		}
		codeField.removeAttribute('aria-invalid');
		error.hidden = true;
		waiting.hidden = false;
		submit.disabled = codeField.disabled = deviceField.disabled = true;
		ctrl = new AbortController();
		const timer = setTimeout(() => ctrl && ctrl.abort(), PAIR_WAIT_MS);
		try {
			await api('/api/v1/pair', { body: { code: digits, device: deviceField.value.trim() }, signal: ctrl.signal });
			clearTimeout(timer);
			await ctx.reloadUi();
			ctx.go('#/');
		} catch (err) {
			clearTimeout(timer);
			if (ctrl && ctrl.signal.aborted && !(err instanceof DOMException && err.name === 'AbortError')) return;
			waiting.hidden = true;
			submit.disabled = codeField.disabled = deviceField.disabled = false;
			error.hidden = false;
			setText(error, err instanceof ApiError || err instanceof TypeError || (err instanceof DOMException && err.name === 'AbortError') ? errorText(err) : fill(P.pairFailed, String(err)));
			codeField.focus();
		}
	});
	if (!code) codeField.focus();
	return () => {
		ctrl?.abort();
	};
}

// ---- the computer -----------------------------------------------------------------------------------------

/**
 * One stream and what this browser asked of it, kept while the page shows the computer: a page
 * that goes (the address changed) leaves it for a moment, for the next to take over.
 * `offline` is whether "no connection" shows: from a drop until the stream is live again.
 * @typedef {{ stream: Stream, asked: { sort: Record<string, string>, log: Record<string, { level: string, search?: string }> },
 *   memory: Map<string, any>, sink: import('./stream.js').Handlers, timer: ReturnType<typeof setTimeout> | null,
 *   status: import('./stream.js').Status, offline: boolean }} Session
 */
/** @type {Session | null} */
let session = null;

/**
 * The computer: the dashboard on screen in the frame (shell.js), live from the stream, the header's
 * chips and the connection's state in the frame's bar.
 * @param {HTMLElement} root @param {Ctx} ctx @param {string | null} tab
 * @returns {() => void} what tears it down
 */
export function mountMachine(root, ctx, tab) {
	let ui = /** @type {Ui} */ (ctx.ui());
	const tabsNow = () => tabsOf(ui);
	const known = (/** @type {string | null | undefined} */ id) => !!id && (ui.dashboards || []).some((d) => d.id === id);
	const remembered = state.load('tab', null);
	/** The dashboard on screen. */
	let current = known(tab) ? /** @type {string} */ (tab) : tabsNow().includes(remembered) ? remembered : tabsNow()[0] || null;
	/** The tab a dashboard on screen was opened from (none for a tab). */
	let parent = /** @type {string | null} */ (null);
	/** Pages this view put in the browser's history, which Back can return through. */
	let pushed = 0;
	let gone = false;

	const banner = h('p', { class: 'note warning banner', role: 'status' });
	banner.hidden = true;
	const pageHead = h('div', { class: 'page-head' });
	const main = h('main', { class: 'machine-main', tabindex: '-1' });
	/** @type {import('./shell.js').Frame | null} */
	let frame = null;

	/** @type {import('./types.js').DashboardView & { clear: () => void } | null} */
	let view = null;
	/** @type {ReturnType<typeof headerChips> | null} */
	let chips = null;
	/** The last line of the dashboard on screen, to draw it again at once when the settings change. @type {Data | null} */
	let lastData = null;

	const run = actionRunner({ ui: () => ui, toast: ctx.toast, unauthorized: () => toPairing() });

	/** @type {Session} */
	const s = session && session.timer ? adopt(session) : start();

	/** @param {Session} old */
	function adopt(old) {
		if (old.timer) clearTimeout(old.timer);
		old.timer = null;
		return old;
	}

	/** @returns {Session} */
	function start() {
		/** @type {Session} */
		const fresh = {
			stream: /** @type {any} */ (null),
			asked: { sort: {}, log: {} },
			memory: new Map(),
			sink: {},
			timer: null,
			status: 'stopped',
			offline: false,
		};
		const logsOf = (/** @type {string} */ d) => {
			const u = ctx.ui();
			const dash = u ? (u.dashboards || []).find((x) => x.id === d) : null;
			return dash ? allWidgets(dash.widgets || []).filter((w) => w.type === 'log').map((w) => w.id) : [];
		};
		fresh.stream = new Stream(
			{ dashboard: current || '', sort: {}, log: {} },
			// The interval of the `ui` there is now: a new one with another profile counts at once.
			{ widgets: DRAWS, intervalMs: () => intervalOf(ctx.ui()), logsOf },
			{
				line: (l) => fresh.sink.line?.(l),
				status: (st) => {
					fresh.status = st;
					fresh.offline = offlineAfter(fresh.offline, st);
					fresh.sink.status?.(st);
				},
				unauthorized: () => fresh.sink.unauthorized?.(),
				refused: (e, body) => fresh.sink.refused?.(e, body),
			},
		);
		session = fresh;
		if (current) fresh.stream.start();
		return fresh;
	}

	s.sink = {
		line(l) {
			if (gone) return;
			if (l.kind === 'open' || l.kind === 'ui') {
				if (l.ui && l.ui !== ui.version) refreshUi();
			} else if (l.kind === 'data') {
				chips?.update(l.data);
				if (l.data.dashboard === current) {
					lastData = l.data;
					view?.update(l.data);
				}
			}
		},
		status(st) {
			if (gone) return;
			showStatus(st);
		},
		unauthorized: () => toPairing(),
		refused(e, body) {
			if (e.code === 'no_such_dashboard') return void refreshUi();
			ctx.toast(errorText(e), 'warning');
			// What was refused isn't asked again: a table's order (`bad_sort` names the table), the
			// logs' filters of the change (`bad_param` `log`). The rest is asked for anew.
			let dropped = false;
			if (e.code === 'bad_sort') {
				for (const id of e.detail ? [e.detail] : Object.keys(body.sort || {})) if (id in s.asked.sort) dropped = delete s.asked.sort[id];
			} else if (e.code === 'bad_param' && e.detail === 'log') {
				for (const id of Object.keys(body.log || {})) if (id in s.asked.log) dropped = delete s.asked.log[id];
			}
			if (!dropped) return;
			s.stream.set({ sort: { ...s.asked.sort }, log: { ...s.asked.log } });
			if (!gone) redraw();
		},
	};

	/** Keeps of what was asked of the stream only what `ui` still has. */
	function pruneAsked() {
		const kept = keptAsked(s.asked, ui.dashboards || []);
		s.asked.sort = kept.sort;
		s.asked.log = kept.log;
		s.stream.forget(kept.gone);
	}

	/** The dashboard drawn again, with the last line at once: its tables' chips as asked now. */
	function redraw() {
		drawView();
		if (lastData && lastData.dashboard === current) view?.update({ ...lastData, history: undefined });
	}

	/** @param {import('./stream.js').Status} st */
	function showStatus(st) {
		// Shown from a drop through the tries that follow, until the stream is live again.
		banner.hidden = !s.offline;
		setText(banner, s.offline ? S.core.noConnection : '');
		// No stale numbers: what was shown goes when the connection does, or the page hides.
		if (st === 'down' || st === 'paused') {
			lastData = null;
			view?.clear();
			chips?.clear();
		}
	}

	async function toPairing() {
		if (gone) return;
		try {
			await ctx.reloadUi();
		} catch {}
		ctx.go('#/');
	}

	async function refreshUi() {
		let u;
		try {
			u = await ctx.reloadUi();
		} catch {
			return;
		}
		if (gone) return;
		if (!u) return toPairing();
		ui = u;
		pruneAsked();
		if (!known(current)) {
			current = tabsNow()[0] || null;
			parent = null;
		}
		render();
		if (lastData && lastData.dashboard === current) {
			view?.update({ ...lastData, history: undefined });
			chips?.update(lastData);
		}
		follow();
	}

	/** The stream follows the dashboard on screen; with none (no tabs left), it is closed. */
	function follow() {
		if (!current) {
			s.stream.stop();
			return;
		}
		s.stream.set({ dashboard: current, sort: { ...s.asked.sort }, log: { ...s.asked.log } });
		if (s.status === 'stopped') s.stream.start();
	}

	/** The frame for this `ui`, anew: its tabs, its tools, the chips; the page's parts in it. */
	function drawFrame() {
		frame?.destroy();
		root.replaceChildren();
		frame = mountFrame(root, ctx, {
			at: 'tab',
			pick: (id) => show(id, null, 'push'),
			forgotten: () => {
				s.stream.stop();
				if (session === s) session = null;
			},
		});
		chips = headerChips(ui);
		frame.chips.append(chips.el);
		frame.status.append(banner);
		frame.body.append(pageHead, main);
	}

	function drawView() {
		view?.destroy();
		view = null;
		pageHead.replaceChildren();
		if (!current) {
			main.replaceChildren(h('p', { class: 'note' }, P.noTabs));
			return;
		}
		const d = (ui.dashboards || []).find((x) => x.id === current);
		const name = (ctx.hello && ctx.hello.name) || S.core.title;
		document.title = d && d.title ? `${d.title} · ${name}` : name;
		frame?.select(parent || current);
		frame?.title(((parent && (ui.dashboards || []).find((x) => x.id === parent)) || d || { title: current }).title || current);
		if (parent || !tabsNow().includes(current)) {
			const back = h('button', { type: 'button', class: 'quiet back' }, icon('back', 'w-icon'), ` ${P.back}`);
			back.addEventListener('click', () => {
				if (pushed > 0) history.back();
				else show(parent || tabsNow()[0] || '', null, 'replace');
			});
			pageHead.append(back, h('h2', { class: 'page-title' }, (d && d.title) || current));
		}
		view = dashboardView(ui, current, {
			runAction: run,
			change(body) {
				const b = /** @type {{ sort?: Record<string, string | null>, log?: Record<string, any> }} */ (body);
				for (const [k, v] of Object.entries(b.sort || {})) {
					if (v == null) delete s.asked.sort[k];
					else s.asked.sort[k] = v;
				}
				for (const [k, v] of Object.entries(b.log || {})) {
					if (v == null) delete s.asked.log[k];
					else s.asked.log[k] = v;
				}
				s.stream.set({ sort: { ...s.asked.sort }, log: { ...s.asked.log } });
			},
			open: (id) => show(id, parent || current, 'push'),
			asked: s.asked,
			memory: s.memory,
		});
		main.replaceChildren(view.el);
	}

	function render() {
		drawFrame();
		drawView();
		showStatus(s.status);
	}

	/**
	 * Shows the dashboard `id`, opened from `from` (null for a tab), the address following as `how` says.
	 * @param {string} id @param {string | null} from @param {'push' | 'replace' | 'none'} how
	 */
	function show(id, from, how) {
		if (!known(id)) return;
		const isTab = tabsNow().includes(id) && !from;
		current = id;
		parent = isTab ? null : from;
		lastData = null;
		if (isTab) state.save('tab', id);
		const hash = `#/tab/${encodeURIComponent(id)}`;
		if (how === 'push' && location.hash !== hash) {
			history.pushState(null, '', hash);
			pushed++;
		} else if (how === 'replace') history.replaceState(null, '', hash);
		if (how === 'replace' && pushed > 0) pushed--;
		drawView();
		s.stream.set({ dashboard: id });
		if (s.status === 'stopped') s.stream.start();
		main.focus({ preventScroll: true });
	}

	// A dashboard named in the address that isn't a tab is a page opened over one.
	if (current && !tabsNow().includes(current)) parent = tabsNow().includes(remembered) ? remembered : tabsNow()[0] || null;
	if (current && location.hash !== `#/tab/${encodeURIComponent(current)}`) history.replaceState(null, '', `#/tab/${encodeURIComponent(current)}`);
	if (current && tabsNow().includes(current) && !parent) state.save('tab', current);
	// A stream taken over from the page before may have been asked for what this `ui` no longer has.
	pruneAsked();
	render();
	follow();

	return () => {
		gone = true;
		frame?.destroy();
		view?.destroy();
		view = null;
		s.sink = {};
		if (session === s) {
			s.timer = setTimeout(() => {
				s.stream.stop();
				if (session === s) session = null;
			}, ADOPT_MS);
		} else s.stream.stop();
	};
}
