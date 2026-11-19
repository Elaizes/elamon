// Pairing from the page (`admin/pairing`): a code for a role up to the asker's and a profile, shown
// big with its expiry and two QR codes (the app's scanner, a phone's browser); asked again every
// second or two while open and in sight, so whoever gave the code is let in or turned away here.
import { h, clear, append } from '../dom.js';
import { S, fill } from '../strings.js';
import { roleName, roleChoice } from '../roles.js';
import { head, button, card, chip, kv, failure, loading, checkbox } from './ui.js';
import { qrMatrix, qrScale } from './qr.js';
import { countdown, ago, nowS } from './time.js';
import { groupCode } from './text.js';
import { errorSentence } from './errors.js';
import { PAIRING_POLL_MS, TICK_MS, QR_QUIET, QR_PIXELS, QR_DARK, QR_LIGHT } from '../consts.js';

/** @typedef {import('./types.js').Pending} Pending */
/** @typedef {import('./types.js').AdminClients} AdminClients */

/** @type {import('./index.js').Mount} */
export function mountPairing(main, env) {
	const T = S.admin.pairing;
	let destroyed = false;
	let poll = 0;
	let asking = false;
	let tick = 0;
	/** What is drawn: the form, or a code (by the code and its links, so it is drawn again only when they change). */
	let shown = '';
	/** The ask drawn, by its id ('' for none yet: the waiting card); null when neither is drawn. @type {string | null} */
	let askShown = null;
	/** @type {AdminClients | null} */
	let meta = null;
	/** The role a code is for when none is asked for: the settings' `default_role`, where they say it. @type {string | null} */
	let defaultRole = null;
	/** @type {Pending | null} */
	let pending = null;
	/** A note for the form after a code went away: answered, withdrawn, used or expired. */
	let after = '';

	const box = h('div', { class: 'adm-pairing' });
	const status = h('p', { class: 'muted adm-poll', 'aria-live': 'polite' });
	main.append(head(S.admin.nav.pairing), box, status);
	box.append(loading());

	/** @param {Pending | null} p */
	const show = (p) => {
		pending = p;
		if (!p || !p.code) {
			if (shown !== 'form') {
				if (shown && !after) after = T.gone;
				drawForm();
			}
			return;
		}
		const key = `${p.code} ${p.app?.link || ''} ${p.web?.link || ''}`;
		if (key !== shown) drawPending(p, key);
		drawAsk(p);
		drawCount();
	};

	// Asked while the page is in sight only; again at once when it shows.
	const ask = async () => {
		poll = 0;
		if (destroyed || asking || document.hidden) return;
		asking = true;
		try {
			show(await env.call('pairing'));
			status.textContent = '';
		} catch (e) {
			if (env.quiet(e)) return;
			status.textContent = fill(T.pollFailed, errorSentence(e));
		} finally {
			asking = false;
		}
		if (!destroyed && !document.hidden) poll = window.setTimeout(ask, PAIRING_POLL_MS);
	};
	const onVisible = () => {
		if (document.hidden) {
			clearTimeout(poll);
			poll = 0;
		} else if (meta && !poll) ask();
	};
	document.addEventListener('visibilitychange', onVisible);

	const start = async () => {
		try {
			const [clients, settings] = await Promise.all([env.call('clients'), env.call('settings').catch(() => null)]);
			meta = /** @type {AdminClients} */ (clients);
			const role = settings && settings.settings ? settings.settings.default_role : null;
			defaultRole = typeof role === 'string' ? role : null;
		} catch (e) {
			if (env.quiet(e)) return;
			clear(box);
			box.append(failure(e, start));
			return;
		}
		ask();
	};
	start();
	tick = window.setInterval(() => drawCount(), TICK_MS);

	// ---------------------------------------------------------------- the form
	function drawForm() {
		shown = 'form';
		askShown = null;
		clear(box);
		const m = /** @type {AdminClients} */ (meta);
		const give = m.can_give || [];
		// No role asked for, the service gives the settings' default: chosen first, unless it is
		// one this client may not give.
		const byDefault = defaultRole == null || give.includes(defaultRole);
		const role = h(
			'select',
			{ id: 'adm-pair-role' },
			byDefault ? h('option', { value: '', selected: true }, defaultRole ? fill(T.roleDefault, roleName(defaultRole)) : T.roleDefaultUnknown) : null,
			...give.map((r, i) => h('option', { value: r, selected: !byDefault && i === give.length - 1 }, roleChoice(r))),
		);
		const profile = h('select', { id: 'adm-pair-profile' }, h('option', { value: '' }, S.admin.clients.defaultProfile), ...(m.profiles || []).map((p) => h('option', { value: p }, p)));
		const make = h('button', { type: 'button', class: 'primary' }, T.make);
		make.addEventListener('click', async () => {
			make.disabled = true;
			try {
				/** @type {{ role?: string, profile: string | null }} */
				const body = { profile: profile.value || null };
				if (role.value) body.role = role.value;
				const p = await env.call('pairing', body);
				after = '';
				show(p && p.code ? p : await env.call('pairing'));
			} catch (e) {
				env.fail(e);
			} finally {
				make.disabled = false;
			}
		});
		append(
			box,
			after ? h('p', { class: 'note' }, after) : null,
			card(
				T.newCode,
				h('p', null, T.intro),
				h('label', { for: 'adm-pair-role' }, S.admin.clients.role),
				role,
				h('label', { for: 'adm-pair-profile' }, S.admin.clients.profile),
				profile,
				h('div', { class: 'adm-row' }, make),
			),
		);
		if (!give.length) make.disabled = true;
	}

	// ---------------------------------------------------------------- a code
	/** @type {HTMLElement | null} */
	let countEl = null;
	/** @type {HTMLElement | null} */
	let askBox = null;

	/** @param {Pending} p @param {string} key */
	function drawPending(p, key) {
		shown = key;
		askShown = null;
		after = '';
		clear(box);
		countEl = h('span', { class: 'mono' });
		askBox = h('div', { class: 'adm-ask-box', 'aria-live': 'assertive' });
		const withdraw = h('button', { type: 'button', class: 'danger' }, T.withdraw);
		withdraw.addEventListener('click', async () => {
			withdraw.disabled = true;
			try {
				await env.call('pairing/withdraw', {});
				after = T.withdrawn;
				shown = '';
				show({ code: null });
			} catch (e) {
				withdraw.disabled = false;
				env.fail(e);
			}
		});
		box.append(
			h(
				'div',
				{ class: 'adm-pair-top' },
				card(
					T.code,
					h('p', { class: 'adm-code-num mono', 'aria-label': fill(T.codeIs, String(p.code).split('').join(' ')) }, groupCode(p.code)),
					h('p', { class: 'adm-expiry' }, T.expiresIn, ' ', countEl),
					kv([
						[S.admin.clients.role, p.role ? roleChoice(p.role) : S.admin.none],
						[S.admin.clients.profile, p.profile || S.admin.clients.defaultProfile],
					]),
					h('p', { class: 'muted' }, p.from_page === false ? T.fromComputer : T.fromPage),
					h('div', { class: 'adm-row' }, withdraw),
				),
				askBox,
			),
			h('div', { class: 'adm-qrs' }, qrFigure(T.forApp, T.forAppHint, p.app), qrFigure(T.forWeb, T.forWebHint, p.web)),
		);
	}

	function drawCount() {
		if (!countEl || !pending || !pending.code) return;
		const left = (pending.expires || 0) - nowS();
		countEl.textContent = left > 0 ? countdown(left) : T.expired;
		countEl.classList.toggle('bad', left <= 0);
	}

	/** @param {Pending} p */
	function drawAsk(p) {
		if (!askBox) return;
		const a = p.ask;
		const id = a ? `${a.id}` : '';
		if (id === askShown) return;
		askShown = id;
		clear(askBox);
		if (!a) {
			askBox.append(card(T.waiting, h('p', { class: 'muted' }, T.waitingText)));
			return;
		}
		const offer = a.offer ? checkbox(fill(T.offer, named(a.offer))) : null;
		const answer = async (/** @type {boolean} */ allow) => {
			for (const b of askBox ? askBox.querySelectorAll('button') : []) b.disabled = true;
			try {
				await env.call('pairing/answer', { ask: a.id, allow, take: !!offer && offer.input.checked && allow });
				env.ctx.toast(fill(allow ? T.letIn : T.turnedAway, a.name));
				after = fill(allow ? T.letIn : T.turnedAway, a.name);
				askShown = null;
				show(await env.call('pairing'));
			} catch (e) {
				env.fail(e);
				askShown = null;
				if (pending) drawAsk(pending);
			}
		};
		askBox.append(
			h(
				'section',
				{ class: 'card adm-card adm-ask', role: 'alert' },
				h('h3', null, a.address ? fill(T.askFrom, a.name, a.address) : fill(T.ask, a.name)),
				a.at ? h('p', { class: 'muted' }, fill(T.askedAt, ago(a.at, nowS()))) : null,
				a.replaces ? h('p', { class: 'note' }, fill(T.replaces, named(a.replaces))) : null,
				offer ? offer.el : null,
				h('div', { class: 'adm-row' }, button(T.allow, () => answer(true), 'primary'), button(T.turnAway, () => answer(false), 'danger')),
			),
		);
	}

	return () => {
		destroyed = true;
		clearTimeout(poll);
		clearInterval(tick);
		document.removeEventListener('visibilitychange', onVisible);
	};
}

/** An entry or a profile as the service names it: a string, or an object with a name or an id. @param {any} x @returns {string} */
function named(x) {
	if (x == null) return '';
	if (typeof x !== 'object') return String(x);
	return String(x.name ?? x.id ?? JSON.stringify(x));
}

/**
 * A QR code with what it is for and its link as text.
 * @param {string} title @param {string} hint @param {{ link: string, qr: import('./types.js').Qr } | undefined} part
 */
function qrFigure(title, hint, part) {
	const T = S.admin.pairing;
	const m = part ? qrMatrix(part.qr) : null;
	/** @type {HTMLElement} */
	let picture;
	if (m) {
		const canvas = h('canvas', { class: 'adm-qr', role: 'img', 'aria-label': fill(T.qrOf, title) });
		drawQr(canvas, m);
		picture = canvas;
	} else picture = h('p', { class: 'note warning' }, T.noQr);
	return h(
		'figure',
		{ class: 'card adm-card adm-qr-card' },
		h('figcaption', null, h('strong', null, title), h('span', { class: 'muted' }, hint)),
		picture,
		part ? h('code', { class: 'adm-link' }, part.link) : null,
	);
}

/** Draws a QR code's modules, dark on light with a quiet zone, sharp at this screen's density. @param {HTMLCanvasElement} canvas @param {boolean[][]} m */
function drawQr(canvas, m) {
	const scale = qrScale(m.length, QR_QUIET, QR_PIXELS, window.devicePixelRatio || 1);
	const side = (m.length + 2 * QR_QUIET) * scale;
	canvas.width = side;
	canvas.height = side;
	const g = canvas.getContext('2d');
	if (!g) return;
	g.fillStyle = QR_LIGHT;
	g.fillRect(0, 0, side, side);
	g.fillStyle = QR_DARK;
	m.forEach((row, y) =>
		row.forEach((dark, x) => {
			if (dark) g.fillRect((x + QR_QUIET) * scale, (y + QR_QUIET) * scale, scale, scale);
		}),
	);
}
