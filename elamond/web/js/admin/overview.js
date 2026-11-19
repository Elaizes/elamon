// The service at a glance (`GET admin/state`): what runs, where it listens, control, the page's
// switches and certificate, whether the settings are sound, `elamond setup --check`, the files.
import { h } from '../dom.js';
import { S, fill } from '../strings.js';
import { loadInto, head, button, card, kv, chip, onOff, table } from './ui.js';
import { duration, dateTime, nowS } from './time.js';
import { sizeText } from './text.js';
import { isFingerprint, colonFingerprint } from '../fingerprint.js';
import { roleName } from '../roles.js';
import { fileHref } from './names.js';

/** @typedef {import('./types.js').AdminState} AdminState */

/** @type {import('./index.js').Mount} */
export function mountOverview(main, env) {
	loadInto(main, env, () => env.call('state'), (st, reload) => draw(main, /** @type {AdminState} */ (st), reload));
	return () => {};
}

/** @param {HTMLElement} main @param {AdminState} st @param {() => void} reload */
function draw(main, st, reload) {
	const T = S.admin.overview;
	const s = st.service;
	main.append(
		head(S.admin.nav.overview, button(S.admin.refresh, reload)),
		h(
			'div',
			{ class: 'adm-cards' },
			card(
				T.service,
				kv([
					[T.name, s.name],
					[T.version, s.version],
					[T.up, s.started ? fill(T.upSince, duration(nowS() - s.started), dateTime(s.started)) : S.admin.none],
					[T.features, h('span', { class: 'adm-chips' }, ...(s.features || []).map((f) => chip(f)))],
					[T.fingerprint, h('span', { class: 'mono adm-fp' }, isFingerprint(s.fingerprint) ? colonFingerprint(s.fingerprint) : String(s.fingerprint ?? ''))],
					[T.listening, lines(s.listening)],
					[T.endpoints, lines(s.endpoints)],
					[T.you, st.you ? fill(T.youAre, roleName(st.you.role), st.you.id) : null],
				]),
			),
			controlCard(st),
			webCard(st),
			settingsCard(st),
			setupCard(st),
		),
		filesCard(st),
	);
}

/** Lines of text, one each. @param {string[] | null | undefined} list */
function lines(list) {
	if (!list || !list.length) return S.admin.none;
	return h('span', { class: 'adm-lines mono' }, ...list.map((l) => h('span', null, l)));
}

/** Control's state in words. @param {import('./types.js').ControlState} c */
export function controlText(c) {
	const T = S.admin.control;
	if (c.enabled) return T.isOn;
	const reasons = /** @type {Record<string, string>} */ (T.reasons);
	const why = (c.off_reason && reasons[c.off_reason]) || c.off_reason || T.reasonUnknown;
	return c.held_off_since ? fill(T.isOffSince, why, dateTime(c.held_off_since)) : fill(T.isOff, why);
}

/** @param {AdminState} st */
function controlCard(st) {
	const c = st.control;
	return card(
		S.admin.overview.control,
		h('p', null, onOff(c.enabled), ' ', controlText(c)),
		h('p', null, h('a', { href: '#/admin/control' }, S.admin.overview.toControl)),
	);
}

/** @param {AdminState} st */
function webCard(st) {
	const T = S.admin.overview;
	const w = st.web;
	if (!w) return card(T.web, h('p', { class: 'muted' }, S.admin.none));
	const cert = w.certificate;
	return card(
		T.web,
		kv([
			[T.page, onOff(w.page)],
			[T.admin, onOff(w.admin)],
			[T.write, onOff(w.write)],
			[T.editor, onOff(w.editor)],
			[T.role, roleName(w.role)],
			[T.editorFile, w.editor_file ? h('a', { class: 'mono', href: fileHref(w.editor_file) }, w.editor_file) : S.admin.none],
			[
				T.certificate,
				cert
					? h('span', null, chip(cert.ok ? T.certOk : T.certBad, cert.ok ? 'good' : 'bad'), ' ', h('span', { class: 'mono' }, cert.name), cert.message ? h('span', { class: 'adm-sub' }, cert.message) : null)
					: h('span', { class: 'muted' }, T.certOwn),
			],
		]),
	);
}

/** @param {AdminState} st */
function settingsCard(st) {
	const T = S.admin.overview;
	const s = st.settings;
	if (!s) return card(T.settings, h('p', { class: 'muted' }, S.admin.none));
	if (s.sound) return card(T.settings, h('p', null, chip(T.sound, 'good'), ' ', fill(T.generation, s.generation)), h('p', null, h('a', { href: '#/admin/settings' }, T.toSettings)));
	return card(
		T.settings,
		h('p', null, chip(T.unsound, 'bad'), ' ', T.unsoundWhy),
		h('ul', { class: 'adm-mistakes-plain mono' }, ...String(s.message || '').split('\n').filter(Boolean).map((l) => h('li', null, l))),
	);
}

/** @param {AdminState} st */
function setupCard(st) {
	const T = S.admin.overview;
	const lines = st.setup || [];
	if (!lines.length) return card(T.setup, h('p', { class: 'muted' }, T.setupNone));
	return card(
		T.setup,
		h(
			'ul',
			{ class: 'adm-setup' },
			...lines.map((l) => h('li', { class: l.ok ? 'ok' : 'bad' }, h('span', { class: 'adm-mark', 'aria-label': l.ok ? T.ok : T.notOk }, l.ok ? S.admin.marks.ok : S.admin.marks.notOk), h('span', null, l.text))),
		),
	);
}

/** The settings files, as the files editor lists them too. @param {AdminState} st */
export function filesCard(st) {
	const T = S.admin.files;
	const kinds = /** @type {Record<string, string>} */ (S.admin.kinds);
	return card(
		S.admin.overview.files,
		table(
			[T.name, T.kind, T.writable, T.size, T.modified],
			(st.files || []).map((f) => [
				h('a', { class: 'mono', href: fileHref(f.name) }, f.name),
				kinds[f.kind] || f.kind,
				f.writable ? chip(S.admin.yes, 'good') : h('span', { class: 'adm-why' }, chip(T.readOnly, 'off'), ' ', f.why || ''),
				sizeText(f.size),
				dateTime(f.modified),
			]),
			'adm-files',
		),
	);
}
