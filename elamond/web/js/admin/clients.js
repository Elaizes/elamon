// The paired clients (`GET admin/clients`): who, what, from where and when; their role (up to the
// asker's own), their profile, revoking. "With the entries added with its key" where it has some.
import { h } from '../dom.js';
import { S, fill } from '../strings.js';
import { loadInto, head, button, chip, kv, confirmed, checkbox } from './ui.js';
import { ago, dateTime, nowS } from './time.js';

/** @typedef {import('./types.js').Client} Client */
/** @typedef {import('./types.js').AdminClients} AdminClients */
/** @typedef {import('./types.js').Env} Env */

/** @type {import('./index.js').Mount} */
export function mountClients(main, env) {
	loadInto(main, env, () => env.call('clients'), (data, reload) => draw(main, env, /** @type {AdminClients} */ (data), reload));
	return () => {};
}

/** @param {HTMLElement} main @param {Env} env @param {AdminClients} data @param {() => Promise<void>} reload */
function draw(main, env, data, reload) {
	const T = S.admin.clients;
	const clients = [...(data.clients || [])].sort((a, b) => Number(b.you) - Number(a.you) || (b.seen || 0) - (a.seen || 0));
	const names = new Map(clients.map((c) => [c.id, c.name]));
	main.append(
		head(S.admin.nav.clients, button(T.pairNew, () => env.ctx.go('#/admin/pairing')), button(S.admin.refresh, reload)),
		h('p', { class: 'muted' }, fill(clients.length === 1 ? T.countOne : T.count, clients.length)),
		clients.length ? h('div', { class: 'adm-clients' }, ...clients.map((c) => clientCard(env, data, c, names, reload))) : h('p', { class: 'note' }, T.none),
	);
}

/**
 * @param {Env} env @param {AdminClients} data @param {Client} c @param {Map<string, string>} names
 * @param {() => Promise<void>} reload
 */
function clientCard(env, data, c, names, reload) {
	const T = S.admin.clients;
	const roleNames = /** @type {Record<string, string>} */ (S.admin.roleNames);
	const nameOf = (/** @type {string} */ id) => names.get(id) || id;
	const added = Array.isArray(c.added) ? c.added : [];

	const roles = [...(data.roles || [])];
	if (!roles.includes(c.role)) roles.push(c.role);
	const role = h(
		'select',
		{ 'aria-label': fill(T.roleOf, c.name) },
		...roles.map((r) => h('option', { value: r, selected: r === c.role, disabled: r !== c.role && !(data.can_give || []).includes(r) }, roleNames[r] || r)),
	);
	const profiles = [...(data.profiles || [])];
	if (c.profile && !profiles.includes(c.profile)) profiles.push(c.profile);
	const profile = h(
		'select',
		{ 'aria-label': fill(T.profileOf, c.name) },
		h('option', { value: '', selected: !c.profile }, T.defaultProfile),
		...profiles.map((p) => h('option', { value: p, selected: p === c.profile }, p === c.own_profile ? fill(T.ownProfile, p) : p)),
	);
	const withAdded = added.length ? checkbox(fill(added.length === 1 ? T.withAddedOne : T.withAdded, added.length)) : null;
	const apply = h('button', { type: 'button', class: 'primary', disabled: true }, T.apply);
	const changed = () => role.value !== c.role || (profile.value || null) !== (c.profile || null);
	const onChange = () => {
		apply.disabled = !changed();
	};
	role.addEventListener('change', onChange);
	profile.addEventListener('change', onChange);

	apply.addEventListener('click', async () => {
		if (!changed()) return;
		const all = !!withAdded && withAdded.input.checked;
		const newRole = role.value;
		const newProfile = profile.value || null;
		if (c.you && newRole !== c.role) {
			const ok = await confirmed(env, { title: T.ownRoleTitle, body: [fill(T.ownRoleText, newRole)], confirm: T.ownRoleConfirm, danger: true, loud: true });
			if (!ok) return;
		}
		apply.disabled = true;
		try {
			/** @type {Set<string>} */
			const done = new Set();
			if (newRole !== c.role) {
				const a = await env.call(`clients/${encodeURIComponent(c.id)}/role`, { role: newRole, with_added: all });
				for (const id of a?.changed || []) done.add(id);
			}
			if (newProfile !== (c.profile || null)) {
				const a = await env.call(`clients/${encodeURIComponent(c.id)}/profile`, { profile: newProfile, with_added: all });
				for (const id of a?.changed || []) done.add(id);
			}
			env.ctx.toast(fill(done.size === 1 ? T.changedOne : T.changed, done.size));
			if (c.you) {
				// Its own role changed: the page's `ui`, and with it the admin panel, may be gone.
				await env.ctx.reloadUi();
				if (!env.ctx.has('admin')) return env.ctx.go('#/');
			}
			await reload();
		} catch (e) {
			env.fail(e);
			if (!env.quiet(e)) await reload();
		}
	});

	const revoke = h('button', { type: 'button', class: 'danger' }, T.revoke);
	revoke.addEventListener('click', async () => {
		const box = added.length ? checkbox(fill(added.length === 1 ? T.withAddedOne : T.withAdded, added.length)) : null;
		const ok = await confirmed(env, {
			title: c.you ? T.revokeSelfTitle : fill(T.revokeTitle, c.name),
			body: [c.you ? h('p', { class: 'note critical' }, T.revokeSelfText) : fill(T.revokeText, c.name), box ? box.el : null],
			confirm: c.you ? T.revokeSelfConfirm : T.revoke,
			danger: true,
			loud: c.you,
		});
		if (!ok) return;
		revoke.disabled = true;
		try {
			const a = await env.call(`clients/${encodeURIComponent(c.id)}/revoke`, { with_added: !!box && box.input.checked });
			const n = (a?.changed || []).length;
			env.ctx.toast(fill(n === 1 ? T.revokedOne : T.revoked, n));
			// Revoking itself, the cookie is gone: the page is back at pairing.
			if (c.you) return env.ctx.go('#/');
			await reload();
		} catch (e) {
			revoke.disabled = false;
			env.fail(e);
		}
	});

	const now = nowS();
	return h(
		'article',
		{ class: `card adm-client${c.you ? ' you' : ''}` },
		h(
			'header',
			{ class: 'adm-client-head' },
			h('h3', null, c.name),
			h('span', { class: 'adm-chips' }, c.you ? chip(T.thisBrowser, 'you') : null, chip(c.kind === 'web' ? T.kindWeb : T.kindAndroid), chip(c.role, `role-${c.role}`)),
		),
		kv([
			[T.app, c.app || S.admin.none],
			[T.address, c.address ? h('span', { class: 'mono' }, c.address) : S.admin.none],
			[T.seen, c.seen ? h('span', { title: dateTime(c.seen) }, ago(c.seen, now)) : T.never],
			[T.created, c.created ? dateTime(c.created) : S.admin.none],
			[T.profile, c.profile ? h('span', { class: 'mono' }, c.profile) : T.defaultProfile],
			[T.from, c.from ? fill(T.fromText, nameOf(c.from)) : null],
			[T.added, added.length ? added.map(nameOf).join(', ') : null],
			[T.id, h('span', { class: 'mono' }, c.id)],
		]),
		h(
			'div',
			{ class: 'adm-client-edit' },
			h('label', null, T.role, role),
			h('label', null, T.profile, profile),
			withAdded ? withAdded.el : null,
			h('div', { class: 'adm-row' }, apply, revoke),
		),
	);
}
