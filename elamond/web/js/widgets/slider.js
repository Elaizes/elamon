// `slider`: a number in an action's range, on its step, sent when let go.
import { h, setText } from '../dom.js';
import { card, captionLine, stateEnabled, numberOf } from './common.js';
import { snap, asNumber } from '../actions.js';
import { S } from '../strings.js';
import * as F from '../format.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const action = env.action(w.action);
	const param = action ? (action.params || []).find((p) => p.id === w.param) : undefined;
	const min = param && typeof param.min === 'number' ? param.min : 0;
	const max = Math.max(param && typeof param.max === 'number' ? param.max : 100, min + 1);
	const step = param && param.step && param.step > 0 ? param.step : null;
	const label = w.label || (action && action.name) || '';
	const value = h('span', { class: 'slider-value' });
	const { el, body } = card(w, env, { title: label, trailing: value });
	const cap = captionLine();
	const input = h('input', { type: 'range', min: String(min), max: String(max), step: step ? String(step) : 'any', 'aria-label': label });
	body.append(cap.el, input);
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	let dragging = false;
	let running = false;
	/** @param {number} v */
	const words = (v) => F.withUnit(F.round(v, step == null || step % 1 === 0 ? 0 : 1), param ? param.unit : null);
	const now = () => {
		const n = numberOf(last);
		if (n != null) return n;
		return param && typeof param.default === 'number' ? param.default : min;
	};
	const show = () => {
		if (!dragging) input.value = String(now());
		setText(value, words(dragging ? Number(input.value) : now()));
		input.setAttribute('aria-valuetext', words(Number(input.value)));
		input.disabled = !action || !param || !env.controlOn || !stateEnabled(last) || running;
		cap.set(typeof w.caption === 'string' && w.caption ? w.caption : last && last.caption ? last.caption : null);
	};
	input.addEventListener('input', () => {
		dragging = true;
		if (param) setText(value, words(snap(Number(input.value), param)));
	});
	input.addEventListener('change', async () => {
		dragging = false;
		if (!action || !param || !w.param) return show();
		const v = asNumber(snap(Number(input.value), param), param);
		input.value = String(v);
		running = true;
		show();
		await env.run(action, { ...(w.params || {}), [w.param]: v }, w, label + S.panels.separator + words(v));
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
