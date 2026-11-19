// `gauge`: a ring, filled from `min` to `max` (the state's, where it changes), in the level's colour.
import { svg, h, setText } from '../dom.js';
import { card, captionLine, figureText, fraction, setLevel } from './common.js';

const R = 42;
const CIRCUMFERENCE = 2 * Math.PI * R;
/** The ring's opening at the bottom, as a share of it. */
const GAP = 0.25;

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const { el, body } = card(w, env, { tile: true });
	const arc = CIRCUMFERENCE * (1 - GAP);
	const track = svg('circle', { class: 'ring-track', cx: 50, cy: 50, r: R, 'stroke-dasharray': `${arc} ${CIRCUMFERENCE}`, transform: `rotate(${90 + GAP * 180} 50 50)` });
	const value = svg('circle', { class: 'ring-value', cx: 50, cy: 50, r: R, 'stroke-dasharray': `0 ${CIRCUMFERENCE}`, transform: `rotate(${90 + GAP * 180} 50 50)` });
	const text = h('span', { class: 'ring-text' });
	const ring = h('div', { class: 'ring', role: 'meter', 'aria-valuemin': '0', 'aria-valuemax': '100' },
		svg('svg', { viewBox: '0 0 100 100', 'aria-hidden': 'true', focusable: 'false' }, track, value), text);
	if (w.label) ring.setAttribute('aria-label', w.label);
	const cap = captionLine();
	body.append(ring, cap.el);
	/** @param {import('./common.js').WidgetState | undefined} s */
	const show = (s) => {
		const f = fraction(w, s);
		value.setAttribute('stroke-dasharray', `${arc * f} ${CIRCUMFERENCE}`);
		const t = figureText(w, s);
		setText(text, t);
		ring.setAttribute('aria-valuenow', String(Math.round(f * 100)));
		ring.setAttribute('aria-valuetext', t);
		setLevel(ring, s?.level, s?.blend);
		cap.set(typeof w.caption === 'string' && w.caption ? w.caption : s && s.caption ? s.caption : null);
	};
	show(undefined);
	return { el, update: (s) => show(s), clear: () => show(undefined) };
}
