// What the admin panel's sections share: asking the admin API, saying a refusal, confirming in a
// dialog, lists of labels and values, tables, a diff, a file:line that opens the files editor.
import { api } from '../api.js';
import { h, clear, append } from '../dom.js';
import { S, fill } from '../strings.js';
import { errorSentence, isAbort, isRefusal } from './errors.js';
import { classifyDiff } from './diff.js';
import { fileHref } from './names.js';

/** @typedef {import('./types.js').Env} Env */
/** @typedef {import('../dom.js').Child} Child */

const BASE = '/api/v1/admin/';

/**
 * A section's ways about. `destroy` gives up its requests and closes its dialogs.
 * @param {import('../types.js').Ctx} ctx @returns {Env & { destroy: () => void }}
 */
export function makeEnv(ctx) {
	const ctrl = new AbortController();
	/** @type {Set<HTMLDialogElement>} */
	const dialogs = new Set();
	let gone = false;
	return {
		ctx,
		signal: ctrl.signal,
		async call(path, body) {
			try {
				return await api(BASE + path, { body, signal: ctrl.signal });
			} catch (e) {
				// No longer paired (revoked, or the cookie expired): back to pairing, once.
				if (isRefusal(e) && e.status === 401 && !gone && !ctrl.signal.aborted) {
					gone = true;
					ctx.toast(S.admin.errors.codes.unauthorized, 'warning');
					ctx.go('#/');
				}
				throw e;
			}
		},
		quiet(e) {
			return isAbort(e) || ctrl.signal.aborted || (isRefusal(e) && e.status === 401);
		},
		fail(e) {
			if (!this.quiet(e)) ctx.toast(errorSentence(e), 'critical');
		},
		keep(dialog) {
			dialogs.add(dialog);
			dialog.addEventListener('close', () => dialogs.delete(dialog));
		},
		destroy() {
			ctrl.abort();
			for (const d of [...dialogs]) d.close();
		},
	};
}

/** "Loading…". */
export function loading() {
	return h('p', { class: 'muted adm-loading' }, S.core.loading);
}

/** A refusal in a note, with "Try again" when given a way to. @param {unknown} e @param {(() => void) | null} [retry] */
export function failure(e, retry) {
	return h('div', { class: 'note critical adm-failure', role: 'alert' }, h('p', null, errorSentence(e)), retry ? h('button', { type: 'button', onclick: () => retry() }, S.core.retry) : null);
}

/**
 * Fills `box` with what `get` brings, drawn by `draw`; "Loading…" meanwhile, the refusal and
 * "Try again" when it fails. Returns the way to load again (also handed to `draw`).
 * @template T
 * @param {HTMLElement} box @param {Env} env @param {() => Promise<T>} get
 * @param {(value: T, reload: () => Promise<void>) => void} draw
 */
export function loadInto(box, env, get, draw) {
	const run = async () => {
		clear(box);
		box.append(loading());
		try {
			const value = await get();
			clear(box);
			draw(value, run);
		} catch (e) {
			if (env.quiet(e)) return;
			clear(box);
			box.append(failure(e, run));
		}
	};
	run();
	return run;
}

/** A section's heading, with buttons beside it. @param {string} title @param {...Child} tools */
export function head(title, ...tools) {
	return h('div', { class: 'adm-head' }, h('h2', null, title), h('div', { class: 'adm-tools' }, ...tools));
}

/** A button. @param {string} text @param {() => void} onclick @param {string} [cls] */
export function button(text, onclick, cls) {
	return h('button', { type: 'button', class: cls || null, onclick: () => onclick() }, text);
}

/**
 * Runs `work` with `btn` turned off meanwhile.
 * @param {HTMLButtonElement} btn @param {() => Promise<unknown>} work
 */
export async function busy(btn, work) {
	btn.disabled = true;
	try {
		await work();
	} finally {
		btn.disabled = false;
	}
}

/** A card with a heading. @param {string | null} title @param {...Child} children */
export function card(title, ...children) {
	return h('section', { class: 'card adm-card' }, title ? h('h3', null, title) : null, ...children);
}

/** Labels and values, as a definition list; rows with a null value are left out. @param {[string, Child][]} rows */
export function kv(rows) {
	const dl = h('dl', { class: 'adm-kv' });
	for (const [label, value] of rows) {
		if (value == null || value === false) continue;
		dl.append(h('dt', null, label), h('dd', null, value === '' ? S.admin.none : value));
	}
	return dl;
}

/** A small label: a kind, a state. @param {string} text @param {string} [cls] */
export function chip(text, cls) {
	return h('span', { class: `adm-chip${cls ? ` ${cls}` : ''}` }, text);
}

/** On or off, as a chip. @param {boolean} on */
export function onOff(on) {
	return chip(on ? S.admin.on : S.admin.off, on ? 'good' : 'off');
}

/**
 * A table: a heading row and rows of cells; wrapped so a narrow screen scrolls it sideways.
 * @param {string[]} heads @param {Child[][]} rows @param {string} [cls]
 */
export function table(heads, rows, cls) {
	return h(
		'div',
		{ class: 'adm-table-wrap' },
		h(
			'table',
			{ class: `adm-table${cls ? ` ${cls}` : ''}` },
			h('thead', null, h('tr', null, ...heads.map((t) => h('th', { scope: 'col' }, t)))),
			h('tbody', null, ...rows.map((r) => h('tr', null, ...r.map((c) => h('td', null, c))))),
		),
	);
}

/** Plain text of anything the service sent, for a cell: objects as JSON. @param {unknown} v @returns {string} */
export function plain(v) {
	if (v == null) return '';
	if (typeof v === 'object') return JSON.stringify(v);
	return String(v);
}

/** A file and line that opens the files editor there. @param {string} file @param {number | null | undefined} line */
export function whereLink(file, line) {
	return h('a', { class: 'mono adm-where', href: fileHref(file, line ?? null) }, line ? `${file}:${line}` : file);
}

/**
 * The settings' mistakes, each its file:line, the path it names and what is wrong, opening the
 * files editor at that line. @param {import('./types.js').Mistake[] | null | undefined} mistakes
 */
export function mistakeLinks(mistakes) {
	return h(
		'ul',
		{ class: 'adm-mistakes' },
		...(mistakes || []).map((m) =>
			h(
				'li',
				null,
				h(
					'a',
					{ class: 'adm-mistake', href: fileHref(m.file, m.line ?? null) },
					h('span', { class: 'mono adm-where' }, m.line ? `${m.file}:${m.line}` : m.file),
					m.path ? h('span', { class: 'mono adm-path' }, m.path) : null,
					h('span', { class: 'adm-msg' }, m.message),
				),
			),
		),
	);
}

/**
 * A diff (the service's `diff` lines): each line with its numbers on disk and in the draft, added
 * and taken away coloured. `onLine`, when given, makes a line with a number in the draft go there.
 * @param {string[]} lines @param {((line: number) => void) | null} [onLine]
 */
export function diffView(lines, onLine) {
	const box = h('div', { class: 'adm-diff mono', role: 'list' });
	for (const l of classifyDiff(lines)) {
		const marker = l.kind === 'add' ? S.admin.marks.add : l.kind === 'del' ? S.admin.marks.del : '';
		const row = h(
			'div',
			{ class: `adm-dl adm-dl-${l.kind}`, role: 'listitem' },
			h('span', { class: 'adm-dn', 'aria-hidden': 'true' }, l.old ?? ''),
			h('span', { class: 'adm-dn', 'aria-hidden': 'true' }, l.new ?? ''),
			h('span', { class: 'adm-dm', 'aria-label': S.admin.diff[l.kind] }, l.kind === 'hunk' || l.kind === 'meta' ? '' : marker),
			h('span', { class: 'adm-dt' }, l.text),
		);
		const at = l.new;
		if (onLine && at != null) {
			row.classList.add('adm-dl-go');
			row.tabIndex = 0;
			row.title = fill(S.admin.files.goLineN, at);
			row.addEventListener('click', () => onLine(at));
			row.addEventListener('keydown', (ev) => {
				if (ev.key === 'Enter') onLine(at);
			});
		}
		box.append(row);
	}
	return box;
}

/**
 * A dialog: a heading, its body and its buttons; closed by Escape, by a button, or with the section.
 * `buttons` return true to keep it open.
 * @param {Env} env @param {{ title: string, body: Child[], buttons: { text: string, cls?: string, run?: () => unknown }[], wide?: boolean, loud?: boolean }} o
 * @returns {HTMLDialogElement}
 */
export function dialog(env, o) {
	const dlg = h('dialog', { class: `adm-dialog${o.wide ? ' wide' : ''}${o.loud ? ' loud' : ''}` });
	const actions = h('div', { class: 'actions' });
	for (const b of o.buttons) {
		const btn = h('button', { type: 'button', class: b.cls || null }, b.text);
		btn.addEventListener('click', async () => {
			let keep = false;
			if (b.run) {
				btn.disabled = true;
				try {
					keep = (await b.run()) === true;
				} finally {
					btn.disabled = false;
				}
			}
			if (!keep) dlg.close();
		});
		actions.append(btn);
	}
	append(dlg, h('h2', null, o.title), ...o.body.map((c) => (typeof c === 'string' ? h('p', null, c) : c)), actions);
	dlg.addEventListener('close', () => dlg.remove());
	document.body.append(dlg);
	env.keep(dlg);
	dlg.showModal();
	return dlg;
}

/**
 * Asks before something is done. Resolves true when it is confirmed.
 * @param {Env} env @param {{ title: string, body: Child[], confirm: string, danger?: boolean, loud?: boolean }} o
 * @returns {Promise<boolean>}
 */
export function confirmed(env, o) {
	return new Promise((resolve) => {
		let yes = false;
		const dlg = dialog(env, {
			title: o.title,
			body: o.body,
			loud: o.loud,
			buttons: [
				{ text: S.core.cancel },
				{
					text: o.confirm,
					cls: o.danger ? 'danger' : 'primary',
					run: () => {
						yes = true;
					},
				},
			],
		});
		dlg.addEventListener('close', () => resolve(yes));
	});
}

/** A check box with its label. @param {string} text @param {boolean} [checked] */
export function checkbox(text, checked = false) {
	const input = h('input', { type: 'checkbox', checked });
	return { input, el: h('label', { class: 'adm-check' }, input, h('span', null, text)) };
}

/**
 * Calls `fn` once `ms` have passed since it was last asked to.
 * @param {() => void} fn @param {number} ms
 */
export function debounce(fn, ms) {
	let t = 0;
	const later = () => {
		clearTimeout(t);
		t = window.setTimeout(fn, ms);
	};
	later.cancel = () => clearTimeout(t);
	return later;
}
