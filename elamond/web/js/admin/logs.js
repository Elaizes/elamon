// What happened and what a source gives, as tables: the activity log (`admin/activity`), the kept
// events (`admin/events`), the sources and their values (`admin/values`, `?source=`).
import { h, append } from '../dom.js';
import { S, fill } from '../strings.js';
import { loadInto, head, button, card, chip, table, objectsTable, plain } from './ui.js';
import { dateTimeMs } from './time.js';
import { LIST_LIMIT } from './consts.js';

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
	if (!id) {
		loadInto(main, env, () => env.call('values'), (value, reload) => {
			const sources = /** @type {any[]} */ (value?.sources || []);
			main.append(
				head(S.admin.nav.sources, button(S.admin.refresh, reload)),
				h('p', { class: 'muted' }, T.intro),
				sources.length
					? table(
							[T.id, T.kind],
							sources.map((s) => [h('a', { class: 'mono', href: `#/admin/sources/${encodeURIComponent(String(s.id))}` }, plain(s.id)), plain(s.kind)]),
						)
					: h('p', { class: 'note' }, T.none),
			);
		});
		return () => {};
	}
	loadInto(main, env, () => env.call(`values?source=${encodeURIComponent(id)}`), (value, reload) => {
		const v = value || {};
		append(
			main,
			head(fill(T.title, plain(v.id ?? id)), h('a', { class: 'button quiet', href: '#/admin/sources' }, T.all), button(S.admin.refresh, reload)),
			h('p', { class: 'muted' }, fill(T.kindIs, plain(v.kind))),
			v.error ? h('p', { class: 'note critical' }, plain(v.error)) : null,
			card(T.values, list(v.values, ['address', 'text'], T.noValues)),
			v.commands != null ? card(T.commands, list(v.commands, ['command', 'name'], T.noCommands)) : null,
			v.settings != null ? card(T.settings, list(v.settings, ['variable', 'name', 'value'], T.noSettings)) : null,
		);
	});
	return () => {};
}

/**
 * A list the service gives whose items' shape isn't fixed: objects as a table, anything else a line each.
 * @param {unknown} items @param {string[]} first @param {string} none
 */
function list(items, first, none) {
	if (!Array.isArray(items) || !items.length) return h('p', { class: 'muted' }, none);
	if (items.every((i) => i && typeof i === 'object' && !Array.isArray(i))) return objectsTable(items, first);
	return h('ul', { class: 'adm-plain-list mono' }, ...items.map((i) => h('li', null, plain(i))));
}
