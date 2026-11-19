// What happened and what a source gives, as tables: the activity log (`admin/activity`), the kept
// events (`admin/events`), the sources and their values (`admin/values`, `?source=`).
import { h, append } from '../dom.js';
import { S, fill } from '../strings.js';
import { loadInto, head, button, card, chip, table, plain } from './ui.js';
import { dateTimeMs } from './time.js';
import { LIST_LIMIT } from '../consts.js';

/** The source the monitors are asked for by (`elamond values displays`), where the computer has them. */
const DISPLAYS = 'displays';

/** @type {import('./index.js').Mount} */
export function mountActivity(main, env) {
	const T = S.admin.activity;
	loadInto(main, env, () => env.call(`activity?limit=${LIST_LIMIT}`), (value, reload) => {
		const entries = /** @type {any[]} */ (value?.entries || []);
		main.append(
			head(S.admin.nav.activity, button(S.admin.refresh, reload)),
			h('p', { class: 'muted' }, fill(T.intro, entries.length)),
			entries.length
				? table(
						[T.when, T.client, T.what, T.result, T.message],
						entries.map((e) => [
							h('span', { class: 'adm-nowrap' }, dateTimeMs(e.t)),
							h('span', null, plain(e.phone), e.family ? h('span', { class: 'adm-sub mono' }, fill(T.family, plain(e.family))) : null),
							h('span', { class: 'mono' }, plain(e.what)),
							chip(plain(e.result), e.result === 'ok' ? 'good' : 'bad'),
							plain(e.message),
						]),
						'adm-log',
					)
				: h('p', { class: 'note' }, T.none),
		);
	});
	return () => {};
}

/** @type {import('./index.js').Mount} */
export function mountEvents(main, env) {
	const T = S.admin.events;
	loadInto(main, env, () => env.call(`events?limit=${LIST_LIMIT}`), (value, reload) => {
		const events = /** @type {any[]} */ (value?.events || []);
		main.append(
			head(S.admin.nav.events, button(S.admin.refresh, reload)),
			h('p', { class: 'muted' }, fill(T.intro, events.length)),
			events.length
				? table(
						[T.when, T.level, T.rule, T.phase, T.text],
						events.map((e) => [
							h('span', { class: 'adm-nowrap' }, dateTimeMs(e.t)),
							chip(plain(e.level), `lv-${String(e.level)}`),
							h('span', { class: 'mono' }, plain(e.rule), e.code && e.code !== e.rule ? h('span', { class: 'adm-sub' }, plain(e.code)) : null),
							plain(e.phase),
							h('span', null, e.title ? h('strong', null, plain(e.title), ' ') : null, plain(e.text)),
						]),
						'adm-log',
					)
				: h('p', { class: 'note' }, T.none),
		);
	});
	return () => {};
}

/** @type {import('./index.js').Mount} */
export function mountSources(main, env, rest) {
	const T = S.admin.sources;
	const id = rest.length ? rest.join('/') : null;
	const link = (/** @type {unknown} */ sid) => h('a', { class: 'mono', href: `#/admin/sources/${encodeURIComponent(String(sid))}` }, plain(sid));
	if (!id) {
		loadInto(main, env, () => env.call('values'), (value, reload) => {
			const sources = /** @type {{ id: string, kind: string }[]} */ (Array.isArray(value?.sources) ? value.sources : []);
			const values = /** @type {{ address: string, now?: string | null }[]} */ (Array.isArray(value?.values) ? value.values : []);
			// The monitors are asked for as a source too, where the computer has them.
			const rows = env.ctx.has('displays') && !sources.some((s) => s.id === DISPLAYS) ? [...sources, { id: DISPLAYS, kind: DISPLAYS }] : sources;
			main.append(
				head(S.admin.nav.sources, button(S.admin.refresh, reload)),
				h('p', { class: 'muted' }, T.intro),
				rows.length ? table([T.id, T.kind], rows.map((s) => [link(s.id), plain(s.kind)])) : h('p', { class: 'note' }, T.none),
				card(T.values, values.length ? table([T.address, T.now], values.map((v) => [h('span', { class: 'mono' }, plain(v.address)), plain(v.now)]), 'adm-log') : h('p', { class: 'muted' }, T.noValues)),
			);
		});
		return () => {};
	}
	loadInto(main, env, () => env.call(`values?source=${encodeURIComponent(id)}`), (value, reload) => {
		const v = value || {};
		const lines = Array.isArray(v.lines) ? v.lines.map(plain) : [];
		append(
			main,
			head(fill(T.title, plain(v.id ?? id)), h('a', { class: 'button quiet', href: '#/admin/sources' }, T.all), button(S.admin.refresh, reload)),
			h('p', { class: 'muted' }, fill(T.kindIs, plain(v.kind))),
			v.error ? h('p', { class: 'note critical', role: 'alert' }, fill(T.failed, plain(v.error))) : null,
			// As `elamond values <id>` prints it: its own columns, so as it is, in monospace.
			card(fill(T.printed, plain(v.id ?? id)), lines.length ? h('pre', { class: 'adm-pre mono' }, lines.join('\n')) : h('p', { class: 'muted' }, T.noLines)),
		);
	});
	return () => {};
}
