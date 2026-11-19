// Widgets in a grid, in order, each as wide as its span (layout.js): a dashboard's, or a group's
// inside its card. A table that hides while empty leaves the grid while it is.
import { h, setStyle } from '../dom.js';
import { placedSpans, span } from '../layout.js';

/**
 * @param {import('./common.js').Widget[]} widgets @param {import('./common.js').Env} env
 * @returns {{ el: HTMLElement, update: (data: import('./common.js').Data) => void, layout: (columns: number) => void, clear: () => void, destroy: () => void }}
 */
export function makeGrid(widgets, env) {
	const el = h('div', { class: 'dash-grid' });
	const items = widgets.map((w) => ({ w, inst: env.make(w, env) }));
	for (const { inst } of items) el.append(inst.el);
	let columns = 0;
	/** Which widgets were shown when last laid out. */
	let shownKey = '';

	const shownNow = () => items.map(({ inst }) => (inst.shown ? inst.shown() : true));

	/** @param {number} cols */
	function layout(cols) {
		columns = cols;
		const shown = shownNow();
		shownKey = shown.join();
		const visible = items.filter((_, i) => shown[i]);
		const spans = placedSpans(visible.map(({ w }) => w.span), cols);
		setStyle(el, { '--cols': cols });
		let k = 0;
		items.forEach(({ inst }, i) => {
			inst.el.hidden = !shown[i];
			if (shown[i]) setStyle(inst.el, { 'grid-column': `span ${spans[k++]}` });
		});
		// A group lays its own widgets out in as many columns as its span has, as the app does.
		items.forEach(({ w, inst }, i) => {
			if (shown[i]) inst.layout?.(span(w.span, cols));
		});
	}

	return {
		el,
		layout,
		update(data) {
			for (const { w, inst } of items) inst.update(data.widgets ? data.widgets[w.id] : undefined, data);
			if (columns && shownNow().join() !== shownKey) layout(columns);
		},
		clear() {
			for (const { inst } of items) inst.clear?.();
			if (columns && shownNow().join() !== shownKey) layout(columns);
		},
		destroy() {
			for (const { inst } of items) inst.destroy?.();
		},
	};
}
