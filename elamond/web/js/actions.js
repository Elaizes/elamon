// Running the computer's actions (`POST /api/v1/actions/{id}`): the values a widget leaves open
// asked for in a form, then the question an action with `confirm` asks (the profile's
// `confirm_actions` already in it), or the text typed in for one whose choice carries
// `confirm_text`; then the request, and the service's refusals in words. The page has no unlock:
// an action that unlocks a phone first is asked here as the app asks without a screen lock.
import { api, ApiError } from './api.js';
import { h, setText } from './dom.js';
import { icon } from './icons.js';
import { errorText } from './errors.js';
import { S, fill } from './strings.js';
import * as F from './format.js';

const P = S.panels;

/**
 * @typedef {import('./types.js').Action} Action
 * @typedef {import('./types.js').Widget} Widget
 * @typedef {{ id: string, kind: string, label?: string, choices?: { value: string, label?: string, confirm_text?: string }[],
 *   min?: number, max?: number, step?: number, unit?: string, default?: unknown, optional?: boolean }} Param
 * What an action's request answered: `{ ok }`, and for a command `exit_code`, `timed_out`, `output`.
 * @typedef {{ ok: boolean, exit_code?: number | null, timed_out?: boolean, output?: string }} ActionResult
 */

/** Between a widget's label and the value it sets, in what asks: "Power profile: Performance". */
const SEPARATOR = P.separator;
const EPSILON = 1e-9;

/** The words power actions ask with, as the app's. */
/** @type {Record<string, { question: string, body: string }>} */
const POWER = {
	'power.lock': { question: P.powerLock, body: P.powerLockBody },
	'power.suspend': { question: P.powerSleep, body: P.powerSleepBody },
	'power.reboot': { question: P.powerRestart, body: P.powerRestartBody },
	'power.poweroff': { question: P.powerShutDown, body: P.powerShutDownBody },
};
/** Drawn in red: they end what runs. */
const DANGER = new Set(['power.poweroff', 'power.reboot']);

// ---- parameters (the app's Params) -----------------------------------------------------------------

/** @param {Action} action @returns {Param[]} */
function paramsOf(action) {
	return Array.isArray(action.params) ? action.params : [];
}

/** The parameters the form asks for: those `fixed` leaves open that must be given. @param {Action} action @param {Record<string, unknown>} fixed */
export function asked(action, fixed) {
	return paramsOf(action).filter((p) => !Object.hasOwn(fixed, p.id) && !p.optional);
}

/** @param {number} v */
function whole(v) {
	return Number.isFinite(v) && Math.floor(v) === v;
}

/** `v` as the number `p` takes: an integer where its step and start are whole. @param {number} v @param {Param} p */
export function asNumber(v, p) {
	return whole(p.step ?? 1) && whole(p.min ?? 0) && whole(v) ? Math.trunc(v) : v;
}

/** `v` on `p`'s range and step, the nearest it takes. @param {number} v @param {Param} p */
export function snap(v, p) {
	if (p.min == null) return v;
	const max = p.max ?? Number.MAX_VALUE;
	const step = p.step && p.step > 0 ? p.step : null;
	const on = step == null ? v : p.min + Math.round((v - p.min) / step) * step;
	return Math.min(Math.max(on, p.min), max);
}

/** What the form starts with for `p`: its default, else the first choice, the range's start, or off. @param {Param} p */
export function initial(p) {
	if (p.default != null) return p.default;
	if (p.kind === 'choice') return p.choices && p.choices.length ? p.choices[0].value : null;
	if (p.kind === 'range') return p.min != null ? asNumber(p.min, p) : null;
	if (p.kind === 'bool') return false;
	return null;
}

/** Whether `v` is a value `p` takes, as the service checks it. @param {Param} p @param {unknown} v */
export function valid(p, v) {
	if (v == null) return !!p.optional || p.default != null;
	switch (p.kind) {
		case 'choice':
			return typeof v === 'string' && (p.choices || []).some((c) => c.value === v);
		case 'bool':
			return typeof v === 'boolean';
		case 'string':
			return typeof v === 'string';
		case 'integer':
			return typeof v === 'number' && whole(v);
		case 'number':
			return typeof v === 'number' && Number.isFinite(v);
		case 'range': {
			if (typeof v !== 'number' || !Number.isFinite(v)) return false;
			if (v < (p.min ?? -Infinity) - EPSILON || v > (p.max ?? Infinity) + EPSILON) return false;
			if (!(p.step && p.step > 0)) return true;
			const k = (v - (p.min ?? 0)) / p.step;
			return Math.abs(k - Math.round(k)) < EPSILON;
		}
		default:
			// A kind from a newer service: the service checks it.
			return true;
	}
}

/** A value of `p` as it reads: a choice by its label, a number with its unit, a switch yes or no. @param {Param} p @param {unknown} v */
export function valueWords(p, v) {
	if (v == null) return null;
	switch (p.kind) {
		case 'choice': {
			const c = (p.choices || []).find((x) => x.value === v);
			return c ? c.label || c.value : String(v);
		}
		case 'bool':
			return typeof v === 'boolean' ? F.bool(v) : null;
		case 'range':
		case 'integer':
		case 'number':
			return typeof v === 'number' ? F.withUnit(F.round(v, whole(v) ? 0 : 1), p.unit) : null;
		default:
			return typeof v === 'string' && v ? v : null;
	}
}

/** `phrase` with the values given the asked parameters: "Mode: Fast"; several, each after its name. @param {string} phrase @param {Param[]} params @param {Record<string, unknown>} values */
export function phraseWith(phrase, params, values) {
	const named = params.map((p) => [p, valueWords(p, values[p.id])]).filter(([, w]) => w != null);
	if (!named.length) return phrase;
	const words = params.length === 1 ? named[0][1] : named.map(([p, w]) => `${/** @type {Param} */ (p).label || /** @type {Param} */ (p).id} ${w}`).join(', ');
	return phrase + SEPARATOR + words;
}

/** What asks before a switch sets `p` to `value`: its label and the choice ("DP-1: Off"), the label alone where the same. @param {string} label @param {Param | undefined} p @param {unknown} value */
export function togglePhrase(label, p, value) {
	if (!p || p.kind !== 'choice') return label;
	const words = valueWords(p, value);
	if (!words || words.trim().toLowerCase() === label.trim().toLowerCase()) return label;
	return label + SEPARATOR + words;
}

/** The choice whose `confirm_text` is to be typed before `action` runs with `params`. @param {Action} action @param {Record<string, unknown>} params */
export function typedChoice(action, params) {
	for (const p of paramsOf(action)) {
		const v = params[p.id];
		const c = typeof v === 'string' ? (p.choices || []).find((x) => x.value === v && typeof x.confirm_text === 'string') : null;
		if (c) return c;
	}
	return null;
}

// ---- dialogs -----------------------------------------------------------------------------------------

/**
 * A modal `<dialog>`: `body` under a title, Cancel and the confirming button; resolves with what
 * `result()` gives on confirming, or null. `ready()` says whether the button may be pressed. A
 * `loud` one is framed in red (what it does can't be undone from here); `keep` is handed the
 * dialog, for whoever closes it with what it belongs to.
 * @template T
 * @param {{ title: string, icon?: string | null, body: Node[], button: string, danger?: boolean, ready?: () => boolean, result: () => T, focus?: HTMLElement,
 *   loud?: boolean, keep?: (dialog: HTMLDialogElement) => void }} o
 * @returns {{ promise: Promise<T | null>, update: () => void }}
 */
function dialog(o) {
	const ok = h('button', { class: o.danger ? 'danger' : 'primary', type: 'submit', value: 'ok' }, o.button);
	const cancel = h('button', { type: 'button', value: 'cancel' }, S.core.cancel);
	const titleId = `dlg-${Math.random().toString(36).slice(2)}`;
	const form = h('form', { method: 'dialog', class: 'dialog-form' },
		h('h2', { id: titleId, class: 'dialog-title' }, o.icon ? icon(o.icon, 'dialog-icon') : null, o.title),
		...o.body,
		h('div', { class: 'actions' }, cancel, ok));
	const dlg = h('dialog', { class: o.loud ? 'panels-dialog loud' : 'panels-dialog', 'aria-labelledby': titleId }, form);
	const update = () => {
		ok.disabled = o.ready ? !o.ready() : false;
	};
	const promise = new Promise((resolve) => {
		let done = false;
		const finish = (/** @type {T | null} */ v) => {
			if (done) return;
			done = true;
			dlg.close();
			dlg.remove();
			resolve(v);
		};
		form.addEventListener('submit', (e) => {
			e.preventDefault();
			if (o.ready && !o.ready()) return;
			finish(o.result());
		});
		form.addEventListener('input', update);
		cancel.addEventListener('click', () => finish(null));
		dlg.addEventListener('cancel', (e) => {
			e.preventDefault();
			finish(null);
		});
		dlg.addEventListener('close', () => finish(null));
	});
	document.body.append(dlg);
	o.keep?.(dlg);
	update();
	dlg.showModal();
	(o.focus || (o.danger ? cancel : ok)).focus();
	return { promise, update };
}

/**
 * Asks a yes-or-no question: before an action, before what the admin panel or the editor can't
 * undo. Resolves true when it is confirmed, null otherwise. `body` is a paragraph, or several
 * (strings each a paragraph).
 * @param {string} title @param {string | null | import('./dom.js').Child[]} body @param {string} button @param {boolean} danger
 * @param {string | null | undefined} iconName @param {{ loud?: boolean, keep?: (dialog: HTMLDialogElement) => void }} [more]
 */
export function confirmDialog(title, body, button, danger, iconName, more = {}) {
	const parts = body == null ? [] : Array.isArray(body) ? body : [body];
	/** @type {Node[]} */
	const nodes = [];
	for (const p of parts) {
		if (p == null || p === false) continue;
		nodes.push(typeof p === 'string' || typeof p === 'number' ? h('p', null, String(p)) : p);
	}
	return dialog({ title, icon: iconName, body: nodes, button, danger, result: () => true, loud: more.loud, keep: more.keep }).promise;
}

/** Asks for `text` to be typed, exactly. @param {Action} action @param {{ value: string, confirm_text?: string }} choice @param {string} button */
function typedDialog(action, choice, button) {
	const text = /** @type {string} */ (choice.confirm_text);
	const words =
		action.kind === 'display'
			? [P.typedMonitorTitle, P.typedMonitorBody, P.typedMonitorField]
			: text === choice.value
				? [P.typedTitle, P.typedBody, P.typedField]
				: [P.typedOtherTitle, P.typedOtherBody, P.typedOtherField];
	const field = h('input', { type: 'text', autocomplete: 'off', autocapitalize: 'off', spellcheck: 'false', class: 'mono', 'aria-label': words[2] });
	return dialog({
		title: words[0],
		icon: action.icon,
		body: [h('p', null, fill(words[1], text)), h('label', null, words[2], field)],
		button,
		danger: true,
		ready: () => field.value === text,
		result: () => field.value,
		focus: field,
	}).promise;
}

/** Asks for the values an action needs that the widget leaves open. @param {Action} action @param {Param[]} params @param {string} title @returns {Promise<Record<string, unknown> | null>} */
function paramDialog(action, params, title) {
	/** @type {Record<string, unknown>} */
	const values = {};
	/** @type {Node[]} */
	const body = [];
	for (const p of params) {
		values[p.id] = initial(p);
		const name = p.label || p.id;
		const named = !(params.length === 1 && name.trim().toLowerCase() === title.trim().toLowerCase());
		if (p.kind === 'choice') {
			const group = h('div', { class: 'chips', role: 'radiogroup', 'aria-label': name });
			for (const c of p.choices || []) {
				const b = h('button', { type: 'button', class: 'chip', role: 'radio', 'aria-checked': String(values[p.id] === c.value) }, c.label || c.value);
				b.addEventListener('click', () => {
					values[p.id] = c.value;
					for (const x of group.children) x.setAttribute('aria-checked', String(x === b));
					d.update();
				});
				group.append(b);
			}
			if (named) body.push(h('p', { class: 'param-name' }, name));
			body.push(group);
		} else if (p.kind === 'bool') {
			const box = h('input', { type: 'checkbox', role: 'switch' });
			box.checked = values[p.id] === true;
			box.addEventListener('change', () => (values[p.id] = box.checked));
			body.push(h('label', { class: 'param-bool' }, box, ` ${name}`));
		} else if (p.kind === 'range') {
			const min = p.min ?? 0;
			const max = Math.max(p.max ?? 100, min + 1);
			const shown = h('span', { class: 'param-value' });
			const range = h('input', { type: 'range', min: String(min), max: String(max), step: String(p.step && p.step > 0 ? p.step : 'any'), 'aria-label': name });
			range.value = String(values[p.id] ?? min);
			const set = () => {
				const v = asNumber(snap(Number(range.value), p), p);
				values[p.id] = v;
				setText(shown, valueWords(p, v) || '');
			};
			range.addEventListener('input', set);
			set();
			body.push(h('label', { class: 'param-range' }, h('span', null, name, ' ', shown), range));
		} else {
			const numeric = p.kind === 'integer' || p.kind === 'number';
			const input = h('input', { type: numeric ? 'number' : 'text', step: p.kind === 'integer' ? '1' : numeric ? 'any' : null, inputmode: numeric ? 'decimal' : null });
			input.value = values[p.id] == null ? '' : String(values[p.id]);
			input.addEventListener('input', () => {
				const t = input.value.trim();
				values[p.id] = p.kind === 'integer' ? (/^-?\d+$/.test(t) ? Number(t) : null) : p.kind === 'number' ? (t !== '' && Number.isFinite(Number(t.replace(',', '.'))) ? Number(t.replace(',', '.')) : null) : input.value;
				input.setAttribute('aria-invalid', String(!valid(p, values[p.id])));
			});
			body.push(h('label', null, [name, p.unit].filter(Boolean).join(' · '), input));
		}
	}
	const d = dialog({ title, icon: action.icon, body, button: P.continue, ready: () => params.every((p) => valid(p, values[p.id])), result: () => values });
	return d.promise;
}

/** What a command printed, in a dialog. @param {string} name @param {ActionResult} r */
export function showOutput(name, r) {
	const out = h('pre', { class: 'output mono' }, r.output && r.output.trim() ? r.output : P.noOutput);
	const close = h('button', { type: 'button', class: 'primary' }, S.core.close);
	const dlg = h('dialog', { class: 'panels-dialog output-dialog', 'aria-label': name }, h('h2', { class: 'dialog-title' }, name, ' ', outcomePill(r)), out, h('div', { class: 'actions' }, close));
	close.addEventListener('click', () => dlg.close());
	dlg.addEventListener('close', () => dlg.remove());
	document.body.append(dlg);
	dlg.showModal();
	close.focus();
}

/** How a command ended, as a pill: done, its exit code, or timed out. @param {ActionResult} r */
export function outcomePill(r) {
	const [text, cls] = r.timed_out ? [P.timedOut, 'lv-warning'] : r.exit_code == null || r.exit_code === 0 ? [P.done, 'lv-normal good'] : [fill(P.exitCode, r.exit_code), 'lv-critical'];
	return h('span', { class: `pill outcome ${cls}` }, h('span', { class: 'dot', 'aria-hidden': 'true' }), text);
}

/** Whether an answer tells how a command ended (an action the settings define). @param {any} r */
export function isCommandResult(r) {
	return !!r && typeof r === 'object' && ('exit_code' in r || 'timed_out' in r);
}

// ---- running -------------------------------------------------------------------------------------------

/**
 * The page's way to run actions: asks what is to be asked, sends the request, and says what went
 * wrong in words. Resolves with the answer, or null when nothing ran (cancelled, refused, failed).
 * @param {{ ui: () => import('./types.js').Ui | null, toast: (text: string, level?: 'normal' | 'warning' | 'critical') => void, unauthorized: () => void }} page
 */
export function actionRunner(page) {
	/**
	 * @param {Action} action @param {Record<string, unknown>} fixed @param {Widget | null} w @param {string} [phrase]
	 * @returns {Promise<ActionResult | null>}
	 */
	return async function run(action, fixed, w, phrase) {
		const name = phrase || (w && w.label) || action.name || action.id;
		let params = { ...fixed };
		const open = asked(action, fixed);
		let said = name;
		if (open.length) {
			const given = await paramDialog(action, open, action.name || name);
			if (!given) return null;
			params = { ...params, ...Object.fromEntries(Object.entries(given).filter(([, v]) => v != null)) };
			said = phraseWith(name, open, given);
		}
		const typed = typedChoice(action, params);
		const button = said.split(SEPARATOR)[0] || action.name;
		const ui = page.ui();
		const confirmOn = !ui || !ui.profile || ui.profile.confirm_actions !== false;
		/** @type {string | null} */
		let text = null;
		if (typed) {
			const t = await typedDialog(action, typed, button);
			if (t == null) return null;
			text = t;
		} else if (action.confirm && confirmOn) {
			const power = POWER[action.id];
			const kill = params.signal === 'kill';
			const body = power ? power.body : action.id === 'process.signal' ? (kill ? P.signalKillBody : P.signalQuitBody) : null;
			const yes = await confirmDialog(power ? power.question : fill(P.question, said), body, button, DANGER.has(action.id) || kill, action.icon);
			if (!yes) return null;
		}
		for (let attempt = 0; ; attempt++) {
			try {
				/** @type {any} */
				const req = { params };
				if (text != null) req.confirm_text = text;
				const r = await api(`/api/v1/actions/${encodeURIComponent(action.id)}`, { body: req });
				return r && typeof r === 'object' ? r : { ok: true };
			} catch (e) {
				// The computer decides as the action comes that it is to be typed in (the last screen
				// showing a picture), and says what: asked once here, and sent again with it.
				if (e instanceof ApiError && e.code === 'confirm_required' && e.detail && attempt === 0) {
					const detail = e.detail;
					const choice = { value: Object.values(params).some((v) => v === detail) ? detail : '', confirm_text: detail };
					const t = await typedDialog(action, choice, button);
					if (t == null) return null;
					text = t;
					continue;
				}
				if (e instanceof ApiError && e.status === 401) page.unauthorized();
				page.toast(errorText(e), 'critical');
				return null;
			}
		}
	};
}

/** Whether an action ends what runs, for a button drawn in red. @param {string | null | undefined} id */
export function isDanger(id) {
	return !!id && DANGER.has(id);
}
