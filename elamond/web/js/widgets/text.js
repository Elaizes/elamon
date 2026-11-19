// `text`: words; at level warning or critical, a notice in a card.
import { h, setText } from '../dom.js';
import { notice } from './common.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const el = h('div', { class: 'w w-text' });
	/** @type {string} */
	let shownAs = '';
	/** @param {import('./common.js').WidgetState | undefined} s */
	const show = (s) => {
		const text = s && typeof s.text === 'string' ? s.text : typeof w.text === 'string' ? w.text : null;
		const level = (s && s.level) || w.level || 'normal';
		const key = `${level}\u0000${text}`;
		if (key === shownAs) return;
		shownAs = key;
		if (text == null) {
			el.replaceChildren();
			return;
		}
		if (level === 'warning' || level === 'critical') {
			el.replaceChildren(notice(w.label, text, level));
			return;
		}
		const p = el.querySelector('p.text-plain') || h('p', { class: 'text-plain muted' });
		setText(p, text);
		el.replaceChildren(p);
	};
	show(undefined);
	return { el, update: (s) => show(s), clear: () => show(undefined) };
}
