// Widgets in a grid, in order, row by row as layout.js places them: a dashboard's, or a group's
// inside its card. Each row is a grid of its own columns (the grid's, or one for each widget where
// they leave it incomplete), so a row they don't fill is shared evenly. A table that hides while
// empty leaves the grid while it is.
import { h, setStyle } from '../dom.js';
import { place, span } from '../layout.js';

/**
 * @param {import('./common.js').Widget[]} widgets @param {import('./common.js').Env} env
 * @returns {{ el: HTMLElement, update: (data: import('./common.js').Data) => void, layout: (columns: number) => void, clear: () => void, destroy: () => void }}
 */
export function makeGrid(widgets, env) {
	const el = h('div', { class: 'dash-grid' });
	const items = widgets.map((w) => ({ w, inst: env.make(w, env) }));
	for (const { inst } of items) el.append(inst.el);
	let columns = 0;
	/** Which widgets were shown, and in which rows, when last laid out. */
	let shownKey = '';
	let rowsKey = '';

	const shownNow = () => items.map(({ inst }) => (inst.shown ? inst.shown() : true));

	/** @param {number} cols */
	function layout(cols) {
		columns = cols;
		const shown = shownNow();
		shownKey = shown.join();
		const visible = items.filter((_, i) => shown[i]);
		const lines = place(visible.map(({ w }) => w.span), cols);
		setStyle(el, { '--cols': cols });
		const key = `${cols}|${shownKey}`;
		if (key !== rowsKey) {
			rowsKey = key;
			// Rows anew only when they change: moving a widget would lose the focus in it.
			const rows = lines.map((line) => {
				const row = h('div', { class: 'dash-row' });
				setStyle(row, { '--n': line.columns });
				for (const [k, s] of line.cells) {
					const inst = visible[k].inst;
					setStyle(inst.el, { 'grid-column': `span ${s}` });
					row.append(inst.el);
				}
				return row;
			});
			const hidden = items.filter((_, i) => !shown[i]).map(({ inst }) => inst.el);
			for (const e of hidden) e.hidden = true;
			for (const { inst } of visible) inst.el.hidden = false;
			el.replaceChildren(...rows, ...hidden);
		}
		// A group lays its own widgets out in as many columns as its span has, as the app does.
		for (const { w, inst } of visible) inst.layout?.(span(w.span, cols));
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
