// `status`: a state as a coloured word, a dot before it in its level's colour.
import { h, setText } from '../dom.js';
import { card, captionLine, figureText, setLevel, captionOf } from './common.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const { el, body } = card(w, env, { tile: true });
	const word = h('span', { class: 'pill-text' });
	const pill = h('span', { class: 'pill status' }, h('span', { class: 'dot', 'aria-hidden': 'true' }), word);
	const cap = captionLine();
	body.append(pill, cap.el);
	/** @param {import('./common.js').WidgetState | undefined} s */
	const show = (s) => {
		setText(word, figureText(w, s));
		setLevel(pill, s?.level, s?.blend);
		cap.set(captionOf(w, s));
	};
	show(undefined);
	return { el, update: (s) => show(s), clear: () => show(undefined) };
}
