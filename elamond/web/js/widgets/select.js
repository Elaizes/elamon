// `select`: one of an action's choices, side by side, the one now marked: as the power profiles.
import { h } from '../dom.js';
import { icon } from '../icons.js';
import { card, captionLine, stateEnabled } from './common.js';
import { S } from '../strings.js';

/** The power profiles' pictures, as the app draws them. @param {string} p */
function profileIcon(p) {
	if (['power-saver', 'low-power', 'quiet', 'cool'].includes(p)) return 'leaf';
	if (p === 'performance') return 'rocket';
	return 'gauge';
}

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const action = env.action(w.action);
	const param = action ? (action.params || []).find((p) => p.id === w.param) : null;
	const choices = param && Array.isArray(param.choices) ? param.choices : [];
	const label = w.label || (action && action.name) || '';
	const { el, body } = card(w, env, { title: label });
	const cap = captionLine();
	const group = h('div', { class: 'choices', role: 'radiogroup', 'aria-label': label });
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	let running = false;
	const buttons = choices.map((c, i) => {
		const name = c.label || c.value;
		const b = h('button', { type: 'button', class: 'choice', role: 'radio', 'aria-checked': 'false' },
			action && action.id === 'profile.set' ? icon(profileIcon(c.value), 'w-icon') : null, h('span', { class: 'choice-label' }, name));
		b.addEventListener('click', async () => {
			if (!action || !w.param || running || (last && last.v === c.value)) return;
			running = true;
			show();
			await env.run(action, { ...(w.params || {}), [w.param]: c.value }, w, label + S.panels.separator + name);
			running = false;
			show();
		});
		b.addEventListener('keydown', (e) => {
			const d = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
			if (!d) return;
			e.preventDefault();
			buttons[(i + d + buttons.length) % buttons.length].focus();
		});
		group.append(b);
		return { b, value: c.value };
	});
	body.append(cap.el, group);
	const show = () => {
		const enabled = !!action && !!w.param && env.controlOn && stateEnabled(last) && !running;
		const now = last ? last.v : null;
		const any = buttons.some(({ value }) => value === now);
		buttons.forEach(({ b, value }, i) => {
			const on = value === now;
			b.setAttribute('aria-checked', String(on));
			b.tabIndex = on || (!any && i === 0) ? 0 : -1;
			b.disabled = !enabled;
		});
		group.setAttribute('aria-busy', String(running));
		cap.set(typeof w.caption === 'string' && w.caption ? w.caption : last && last.caption ? last.caption : null);
	};
	show();
	return {
		el,
		update(s) {
			last = s;
			show();
		},
		clear() {
			last = undefined;
			show();
		},
	};
}
