// `toggle`: a switch, the action's `param` set to its `on` or `off` value; on while the state's
// value is its `on` (read through its `map` by the service).
import { h, setText } from '../dom.js';
import { setAccent, stateEnabled, captionOf } from './common.js';
import { togglePhrase } from '../actions.js';
import { sameValue } from '../tables.js';
import { icon } from '../icons.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const action = env.action(w.action);
	const label = w.label || (action && action.name) || '';
	const on = w.on !== undefined ? w.on : true;
	const off = w.off !== undefined ? w.off : false;
	const caption = h('span', { class: 'button-caption muted' });
	caption.hidden = true;
	const knob = h('span', { class: 'switch-track', 'aria-hidden': 'true' }, h('span', { class: 'switch-knob' }));
	const button = h('button', { type: 'button', role: 'switch', class: 'switch-row', 'aria-checked': 'false' },
		icon(w.icon || (action && action.icon) || null, 'w-icon'), h('span', { class: 'button-text' }, h('span', { class: 'button-label' }, label), caption), knob);
	const el = env.inGroup ? h('div', { class: 'w w-toggle in-group' }, button) : h('section', { class: 'card w w-toggle' }, button);
	setAccent(el, w.color);
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	let running = false;
	const checked = () => !!last && last.v != null && sameValue(last.v, on);
	const show = () => {
		button.setAttribute('aria-checked', String(checked()));
		button.disabled = !action || !w.param || !env.controlOn || !stateEnabled(last) || running;
		button.setAttribute('aria-busy', String(running));
		const cap = captionOf(w, last);
		caption.hidden = !cap;
		setText(caption, cap || '');
	};
	button.addEventListener('click', async () => {
		if (!action || !w.param || running) return;
		const value = checked() ? off : on;
		const param = (action.params || []).find((p) => p.id === w.param);
		running = true;
		show();
		await env.run(action, { ...(w.params || {}), [w.param]: value }, w, togglePhrase(label, param, value));
		running = false;
		show();
	});
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
