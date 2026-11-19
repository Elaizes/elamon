// `meter`: the figure, and a bar under it filled from `min` to `max`, in the level's colour.
import { h, setStyle } from '../dom.js';
import { card, captionLine, figure, figureText, fraction, setLevel, captionOf } from './common.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const { el, body } = card(w, env, { tile: true });
	const fig = figure(true);
	if (w.format === 'temp') fig.el.classList.add('temp');
	const fill = h('div', { class: 'bar-fill' });
	const bar = h('div', { class: 'bar', role: 'meter', 'aria-valuemin': '0', 'aria-valuemax': '100' }, fill);
	if (w.label) bar.setAttribute('aria-label', w.label);
	const cap = captionLine();
	body.append(fig.el, bar, cap.el);
	/** @param {import('./common.js').WidgetState | undefined} s */
	const show = (s) => {
		fig.set(w, s);
		setLevel(fig.el, s?.level, s?.blend);
		const f = fraction(w, s);
		setStyle(fill, { width: `${(f * 100).toFixed(2)}%` });
		setLevel(bar, s?.level, s?.blend);
		bar.setAttribute('aria-valuenow', String(Math.round(f * 100)));
		bar.setAttribute('aria-valuetext', figureText(w, s));
		cap.set(captionOf(w, s));
	};
	show(undefined);
	return { el, update: (s) => show(s), clear: () => show(undefined) };
}
