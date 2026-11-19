// `button`: runs an action with the values it fixes (asking for those it leaves open), or opens a
// dashboard. Alone, a row in a card of its own; in a group, a tile (span 1, as the power buttons) or
// a row. While it runs, a spinner in its icon's place; how a command ended, under its name.
import { h, setText } from '../dom.js';
import { icon } from '../icons.js';
import { setAccent, stateEnabled, captionOf } from './common.js';
import { showOutput, outcomePill, isCommandResult, isDanger } from '../actions.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const action = env.action(w.action);
	const label = w.label || (action && action.name) || '';
	const tile = !!env.inGroup && w.span <= 1;
	const opensOnly = !w.action && !!w.open;
	const mark = h('span', { class: 'button-mark' });
	const caption = h('span', { class: 'button-caption muted' });
	caption.hidden = true;
	const outcome = h('span', { class: 'button-outcome' });
	const button = h('button', { type: 'button', class: `action-button${tile ? ' tile-button' : ' row-button'}${isDanger(action && action.id) ? ' danger-soft' : ''}` },
		mark, h('span', { class: 'button-text' }, h('span', { class: 'button-label' }, label), caption));
	const el = env.inGroup ? h('div', { class: 'w w-button in-group' }, button, outcome) : h('section', { class: 'card w w-button' }, button, outcome);
	setAccent(el, w.color || (opensOnly ? null : 'tertiary'));
	let running = false;
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	const iconName = w.icon || (action && action.icon) || (opensOnly ? null : 'play');
	const enabled = () => (opensOnly ? env.canOpen(w.open) : !!action && env.controlOn && stateEnabled(last));
	const show = () => {
		mark.replaceChildren(running ? h('span', { class: 'spinner', 'aria-hidden': 'true' }) : icon(tile ? iconName : opensOnly ? iconName : 'play', 'w-icon'));
		button.disabled = !enabled() || running;
		button.setAttribute('aria-busy', String(running));
		const cap = captionOf(w, last);
		caption.hidden = !cap;
		setText(caption, cap || '');
	};
	button.addEventListener('click', async () => {
		if (opensOnly) {
			env.open(/** @type {string} */ (w.open));
			return;
		}
		if (!action || running) return;
		running = true;
		show();
		const r = await env.run(action, w.params && typeof w.params === 'object' ? w.params : {}, w, label);
		running = false;
		show();
		if (r && isCommandResult(r)) {
			const pill = outcomePill(r);
			const b = h('button', { type: 'button', class: 'pill-button', 'aria-label': `${label}: ${pill.textContent}` }, pill);
			b.addEventListener('click', () => showOutput(label, r));
			outcome.replaceChildren(b);
		}
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
