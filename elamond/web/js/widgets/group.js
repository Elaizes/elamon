// `group`: a card around widgets in a grid of its own, as many columns as the group spans; one with
// `collapsed` folds, as it started until folded or opened here.
import { card, captionLine } from './common.js';
import { makeGrid } from './grid.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const { el, body } = card(w, env);
	const cap = captionLine();
	// Inside a group, buttons are its tiles and rows rather than cards of their own.
	const grid = makeGrid(Array.isArray(w.widgets) ? w.widgets : [], { ...env, inGroup: true });
	grid.el.classList.add('inner');
	body.append(cap.el, grid.el);
	return {
		el,
		update(s, data) {
			cap.set(typeof w.caption === 'string' && w.caption ? w.caption : s && s.caption ? s.caption : null);
			grid.update(data);
		},
		layout: (cols) => grid.layout(cols),
		clear() {
			cap.set(null);
			grid.clear();
		},
		destroy: () => grid.destroy(),
	};
}
