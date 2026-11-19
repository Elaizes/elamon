// Control (`admin/control`): on or off and why; holding it off from here, as `elamond control off`
// does. Turning it on again is done at the computer only (`elamond control on`).
import { h } from '../dom.js';
import { S } from '../strings.js';
import { loadInto, head, button, card, onOff, confirmed } from './ui.js';
import { controlText } from './overview.js';

/** @type {import('./index.js').Mount} */
export function mountControl(main, env) {
	const T = S.admin.control;
	loadInto(main, env, () => env.call('state'), (value, reload) => {
		const st = /** @type {import('./types.js').AdminState} */ (value);
		const c = st.control;
		const off = h('button', { type: 'button', class: 'danger' }, T.turnOff);
		off.addEventListener('click', async () => {
			const ok = await confirmed(env, { title: T.turnOffTitle, body: [`${T.turnOffText} ${T.whatHeld}`, T.onlyAtComputer], confirm: T.turnOff, danger: true, loud: true });
			if (!ok) return;
			off.disabled = true;
			try {
				await env.call('control', { on: false });
				env.ctx.toast(T.turnedOff, 'warning');
				await env.ctx.reloadUi().catch(() => null);
				await reload();
			} catch (e) {
				off.disabled = false;
				env.fail(e);
			}
		});
		/** How to turn it on, by why it is off. */
		const howOn =
			c.off_reason === 'settings'
				? h('p', null, T.onInSettings, ' ', h('a', { href: '#/admin/settings' }, S.admin.nav.settings))
				: c.off_reason === 'settings_unreadable'
					? h('p', null, T.onFixSettings, ' ', h('a', { href: '#/admin/overview' }, S.admin.nav.overview))
					: h('p', null, T.onAtComputer, ' ', h('code', null, T.onCommand));
		const held = !c.enabled && c.off_reason === 'held_off';
		// Writing from here, by why: `[web] write`, control held off (nothing), `control = false`
		// (files not, the editor's dashboards still); the settings not parsing stops nothing, as
		// writing is how they are put right.
		const [writes, writesText] = !st.web?.write
			? [false, T.writesOffSettings]
			: held
				? [false, T.writesHeld]
				: !c.enabled && c.off_reason === 'settings'
					? [false, T.writesNoControl]
					: [true, T.writesOn];
		main.append(
			head(S.admin.nav.control, button(S.admin.refresh, reload)),
			card(
				null,
				h('p', { class: 'adm-big' }, onOff(c.enabled), ' ', controlText(c)),
				h('p', { class: 'muted' }, T.what, held ? ` ${T.whatHeld}` : ''),
				c.enabled ? h('div', null, h('p', null, T.turnOffText, ' ', T.whatHeld), h('div', { class: 'adm-row' }, off)) : h('div', { class: 'note warning' }, howOn),
			),
			card(T.writes, h('p', null, onOff(writes), ' ', writesText)),
		);
	});
	return () => {};
}
