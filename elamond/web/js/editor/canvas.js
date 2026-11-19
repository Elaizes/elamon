// The canvas: the dashboard as it will look, a cell for each widget of the draft drawn by panels.js
// from the preview, and over each a frame to pick it, drag it (pointer events: mouse, pen, touch),
// size it and take it out. Groups are opened out, their widgets cells of their own. Keyboard: each
// cell is focusable; Alt+↑/↓ moves it, Alt+←/→ sizes it, Enter opens its keys, Delete takes it out.
import { h, svg, clear, setStyle } from '../dom.js';
import { S, fill } from '../strings.js';
import { dashboardView } from '../panels.js';
import { flow, pathKey, samePath, within, WIDGETS } from './draft.js';
import { spanOf, spanRange, nameOf, typeLabel, isGroup, matchDrawn } from './describe.js';

/**
 * What the canvas asks of the editor.
 * @typedef {{
 *   select: (path: import('./draft.js').WidgetPath | null, open?: boolean, into?: boolean) => void,
 *   move: (from: import('./draft.js').WidgetPath, parent: import('./draft.js').WidgetPath, index: number) => void,
 *   nudge: (path: import('./draft.js').WidgetPath, delta: -1 | 1) => void,
 *   span: (path: import('./draft.js').WidgetPath, span: number) => void,
 *   remove: (path: import('./draft.js').WidgetPath) => void,
 * }} CanvasHooks
 *
 * The preview as last drawn: the `ui` with the draft (the page's own filled in), the dashboard in
 * it and its data.
 * @typedef {{ ui: import('../types.js').Ui, dashboard: import('../types.js').Dashboard | null, data: import('../types.js').Data | null }} Drawn
 */

/** @param {string} key @returns {import('./draft.js').WidgetPath} */
const parsePath = (key) => (key ? key.split('/').map(Number) : []);

/** The columns the page's grid has now (style.css: four, two under 600 px, one under 320). */
function columns() {
	if (window.matchMedia('(max-width: 320px)').matches) return 1;
	if (window.matchMedia('(max-width: 600px)').matches) return 2;
	return 4;
}

/** @param {'grip' | 'close' | 'resize'} name */
function icon(name) {
	const a = { viewBox: '0 0 24 24', width: 20, height: 20, 'aria-hidden': 'true', focusable: 'false', class: 'ed-svg' };
	if (name === 'grip') {
		return svg('svg', a, ...[6, 12, 18].flatMap((y) => [9, 15].map((x) => svg('circle', { cx: x, cy: y, r: 1.8 }))));
	}
	if (name === 'resize') return svg('svg', a, svg('path', { d: 'M9 6v12M15 6v12', 'stroke-width': 2, 'stroke-linecap': 'round' }));
	return svg('svg', a, svg('path', { d: 'M6 6l12 12M18 6L6 18', 'stroke-width': 2, 'stroke-linecap': 'round' }));
}

export class Canvas {
	/**
	 * @param {{ desc: () => import('./describe.js').Editor, dash: () => import('./draft.js').DraftDashboard, hooks: CanvasHooks }} opts
	 */
	constructor(opts) {
		this.opts = opts;
		this.el = h('div', { class: 'ed-canvas', 'aria-label': S.editor.canvas });
		/**
		 * Each cell's drawing, by the widget's path: the element it is in, the view, and the preview's
		 * widgets it was drawn from (a group's cell has none of its own).
		 * @type {Map<string, { draw: HTMLElement | null, view: import('../types.js').DashboardView | null, sig: string, many: boolean }>}
		 */
		this.cells = new Map();
		/** @type {Map<string, any[] | null>} */
		this.assigned = new Map();
		/** The draft's shape the cells were made for. */
		this.shape = '';
		/** @type {import('./draft.js').WidgetPath | null} */
		this.selected = null;
		/** @type {Drawn | null} */
		this.drawn = null;
		/** @type {Map<string, Map<string, string[]>>} */
		this.mistakes = new Map();
		this.justDragged = false;
		/** While a widget is dragged or sized, a preview waits to be drawn (the cells stay as they are). */
		this.busy = false;
		this.pending = false;
	}

	/**
	 * The preview came. Cells drawing the same widgets as before only take the new data; a cell
	 * whose widgets changed (a label that holds values, a key changed) is drawn again alone.
	 * @param {Drawn} drawn
	 */
	setDrawn(drawn) {
		this.drawn = drawn;
		this.el.classList.remove('stale');
		if (this.busy) {
			this.pending = true;
			return;
		}
		const assigned = this.assign();
		if (shapeOf(this.opts.dash()) !== this.shape || assigned.size !== this.cells.size) return this.render();
		for (const [key, list] of assigned) {
			const cell = this.cells.get(key);
			if (!cell || cell.many !== (!!list && list.length > 1)) return this.render();
		}
		this.assigned = assigned;
		for (const [key, list] of assigned) {
			const cell = /** @type {NonNullable<ReturnType<typeof this.cells.get>>} */ (this.cells.get(key));
			if (!cell.draw) continue;
			if (JSON.stringify(list) !== cell.sig) this.paint(cell, list, key);
			else if (cell.view && drawn.data) cell.view.update(drawn.data);
		}
	}

	/** Which of the preview's widgets each widget of the draft became, by path (null before the first preview). */
	assign() {
		/** @type {Map<string, any[] | null>} */
		const out = new Map();
		const desc = this.opts.desc();
		/** @param {Record<string, any>[]} list @param {import('./draft.js').WidgetPath} parent @param {any[] | null} drawnList @param {string} prefix */
		const walk = (list, parent, drawnList, prefix) => {
			const matched = drawnList ? matchDrawn(drawnList, prefix, list) : list.map(() => null);
			list.forEach((w, i) => {
				const path = [...parent, i];
				out.set(pathKey(path), matched[i]);
				if (w && typeof w === 'object' && isGroup(desc, w)) {
					const g = matched[i] && matched[i][0];
					walk(Array.isArray(w[WIDGETS]) ? w[WIDGETS] : [], path, g ? g.widgets || [] : matched[i] ? [] : null, g ? `${g.id}/` : '');
				}
			});
		};
		const d = this.drawn && this.drawn.dashboard;
		const dash = this.opts.dash();
		walk(Array.isArray(dash[WIDGETS]) ? dash[WIDGETS] : [], [], d ? d.widgets || [] : null, d ? `${d.id}/` : '');
		return out;
	}

	/** The draft changed shape: the drawing waits for the next preview. */
	markStale() {
		this.el.classList.add('stale');
	}

	destroyViews() {
		for (const c of this.cells.values()) if (c.view) c.view.destroy();
		this.cells.clear();
	}

	/** Draws the draft's widgets anew. */
	render() {
		const focused = document.activeElement instanceof HTMLElement && this.el.contains(document.activeElement) ? document.activeElement.dataset.path : undefined;
		this.destroyViews();
		clear(this.el);
		this.pending = false;
		const dash = this.opts.dash();
		this.shape = shapeOf(dash);
		this.assigned = this.assign();
		const list = Array.isArray(dash[WIDGETS]) ? dash[WIDGETS] : [];
		this.el.append(this.grid(list, []));
		if (!list.length) this.el.append(h('p', { class: 'muted ed-empty' }, S.editor.noWidgets));
		if (focused !== undefined) this.focus(parsePath(focused));
	}

	/**
	 * A grid of cells: a list of the draft's widgets, the dashboard's or a group's.
	 * @param {Record<string, any>[]} list @param {import('./draft.js').WidgetPath} parent
	 */
	grid(list, parent) {
		const desc = this.opts.desc();
		const g = h('div', { class: 'ed-grid', role: 'list', 'data-parent': pathKey(parent) });
		const drawn = list.map((w, i) => this.assigned.get(pathKey([...parent, i])) ?? null);
		// A widget made for each item of something takes the whole width: its items flow inside.
		const spans = list.map((w, i) => (drawn[i] && /** @type {any[]} */ (drawn[i]).length > 1 ? 4 : spanOf(desc, w && typeof w === 'object' ? w : {})));
		const wide = flow(spans, 4);
		const narrow = flow(spans, 2);
		list.forEach((w, i) => g.append(this.cell(w && typeof w === 'object' ? w : {}, [...parent, i], drawn[i], wide[i], narrow[i])));
		g.append(
			h(
				'div',
				{ class: `ed-drop${list.length ? '' : ' ed-drop-empty'}`, 'data-parent': pathKey(parent), 'data-index': list.length },
				list.length ? null : S.editor.dropHere,
			),
		);
		return g;
	}

	/**
	 * @param {Record<string, any>} w @param {import('./draft.js').WidgetPath} path @param {any[] | null} drawn
	 * @param {number} wide its width on four columns @param {number} narrow on two
	 */
	cell(w, path, drawn, wide, narrow) {
		const desc = this.opts.desc();
		const group = isGroup(desc, w);
		const name = nameOf(desc, w);
		const span = spanOf(desc, w);
		const key = pathKey(path);
		const mistakes = this.mistakes.get(key);
		const sel = !!this.selected && samePath(this.selected, path);
		const cell = h('div', {
			class: `ed-cell ed-w4-${wide} ed-w2-${narrow}${group ? ' ed-groupcell' : ''}${sel ? ' selected' : ''}${mistakes ? ' bad' : ''}`,
			tabindex: 0,
			role: 'listitem',
			'data-path': key,
			'aria-label': fill(S.editor.cellLabel, name, typeLabel(desc, w.type), span),
			'aria-current': sel ? 'true' : null,
			'aria-keyshortcuts': 'Alt+ArrowUp Alt+ArrowDown Alt+ArrowLeft Alt+ArrowRight Enter Delete',
		});
		const bar = this.bar(w, path, name, span, cell);
		const entry = { draw: /** @type {HTMLElement | null} */ (null), view: null, sig: '', many: !!drawn && drawn.length > 1 };
		this.cells.set(key, entry);
		if (group) {
			cell.append(bar, this.grid(Array.isArray(w[WIDGETS]) ? w[WIDGETS] : [], path));
		} else {
			entry.draw = h('div', { class: 'ed-draw', inert: true });
			this.paint(entry, drawn, key, name);
			const over = h('div', { class: 'ed-over' });
			over.addEventListener('pointerdown', (e) => {
				if (e.pointerType === 'mouse') this.drag(e, path, cell, over, name, 6);
			});
			const resize = h('div', { class: 'ed-resize', title: S.editor.resize, 'aria-hidden': 'true' }, icon('resize'));
			resize.addEventListener('pointerdown', (e) => this.resize(e, path, cell, span));
			cell.append(entry.draw, over, bar, resize);
		}
		if (mistakes) {
			const all = [...mistakes.entries()].flatMap(([k, list]) => list.map((m) => (k ? `${k}: ${m}` : m)));
			cell.append(h('p', { class: 'ed-cell-mistake', role: 'alert' }, all.join(' · ')));
		}
		cell.addEventListener('click', (e) => {
			if (this.justDragged) return;
			const inner = /** @type {Element} */ (e.target).closest('.ed-cell');
			if (inner !== cell) return;
			this.opts.hooks.select(path, true);
		});
		cell.addEventListener('keydown', (e) => this.key(e, path, cell, w));
		return cell;
	}

	/**
	 * Draws a cell's widget: panels.js's view of a dashboard holding just it (or its items), or why
	 * there is nothing to draw.
	 * @param {{ draw: HTMLElement | null, view: import('../types.js').DashboardView | null, sig: string }} cell
	 * @param {any[] | null} drawn @param {string} key @param {string} [name]
	 */
	paint(cell, drawn, key, name) {
		const draw = cell.draw;
		if (!draw) return;
		if (cell.view) cell.view.destroy();
		cell.view = null;
		cell.sig = JSON.stringify(drawn);
		clear(draw);
		const label = name ?? nameOf(this.opts.desc(), this.widget(key) || {});
		const d = this.drawn;
		if (!drawn || !d || !d.dashboard) draw.append(h('p', { class: 'muted ed-placeholder' }, label));
		else if (!drawn.length) draw.append(h('p', { class: 'muted ed-placeholder' }, label, h('br'), h('small', null, S.editor.notShown)));
		else {
			const dash = d.dashboard;
			const widgets = drawn.length > 1 ? drawn : [{ ...drawn[0], span: 4 }];
			/** @type {import('../types.js').Ui} */
			const ui = { ...d.ui, header: null, tabs: [dash.id], dashboards: d.ui.dashboards.map((x) => (x.id === dash.id ? { ...x, widgets } : x)) };
			try {
				cell.view = dashboardView(ui, dash.id, { preview: true });
				if (d.data) cell.view.update(d.data);
				draw.append(cell.view.el);
			} catch {
				draw.append(h('p', { class: 'muted ed-placeholder' }, S.editor.cantDraw));
			}
		}
	}

	/** The draft's widget at a path's key. @param {string} key */
	widget(key) {
		/** @type {any} */
		let w = { [WIDGETS]: this.opts.dash()[WIDGETS] };
		for (const i of parsePath(key)) w = w && Array.isArray(w[WIDGETS]) ? w[WIDGETS][i] : null;
		return w && typeof w === 'object' ? w : null;
	}

	/** The frame's bar: a handle to drag by, the widget's name, its width, taking it out. */
	bar(w, path, name, span, cell) {
		const desc = this.opts.desc();
		const { min, max } = spanRange(desc);
		const grip = h('button', { type: 'button', class: 'ed-grip quiet', tabindex: -1, 'aria-label': fill(S.editor.dragToMove, name), title: S.editor.drag }, icon('grip'));
		grip.addEventListener('pointerdown', (e) => this.drag(e, path, cell, grip, name, 0));
		const widths = h('span', { class: 'ed-widths', role: 'group', 'aria-label': S.editor.width });
		for (let n = min; n <= max; n++) {
			widths.append(
				h('button', {
					type: 'button',
					tabindex: -1,
					class: 'quiet',
					'aria-pressed': n === span ? 'true' : 'false',
					'aria-label': fill(S.editor.widthN, n, max),
					title: fill(S.editor.widthN, n, max),
					onclick: (/** @type {Event} */ e) => {
						e.stopPropagation();
						this.opts.hooks.span(path, n);
					},
				}, String(n)),
			);
		}
		const out = h('button', {
			type: 'button',
			tabindex: -1,
			class: 'quiet ed-remove',
			'aria-label': fill(S.editor.removeWidget, name),
			title: fill(S.editor.removeWidget, name),
			onclick: (/** @type {Event} */ e) => {
				e.stopPropagation();
				this.opts.hooks.remove(path);
			},
		}, icon('close'));
		const bar = h('div', { class: 'ed-bar' }, grip, h('span', { class: 'ed-name' }, h('span', null, name), h('small', { class: 'muted' }, typeLabel(desc, w.type))), widths, out);
		// With a mouse the bar is a handle too (its buttons aside), once it has moved a little.
		bar.addEventListener('pointerdown', (e) => {
			const t = /** @type {Element} */ (e.target);
			if (e.pointerType === 'mouse' && !t.closest('button')) this.drag(e, path, cell, bar, name, 6);
		});
		return bar;
	}

	/** A widget's name changed in its form: its frame says the new one. @param {import('./draft.js').WidgetPath} path */
	relabel(path) {
		const cell = this.el.querySelector(`.ed-cell[data-path="${pathKey(path)}"]`);
		const w = this.widget(pathKey(path));
		if (!(cell instanceof HTMLElement) || !w) return;
		const desc = this.opts.desc();
		const name = nameOf(desc, w);
		const span = cell.querySelector(':scope > .ed-bar .ed-name > span');
		if (span) span.textContent = name;
		cell.setAttribute('aria-label', fill(S.editor.cellLabel, name, typeLabel(desc, w.type), spanOf(desc, w)));
	}

	/** Picks a widget: its frame is marked. @param {import('./draft.js').WidgetPath | null} path */
	select(path) {
		this.selected = path;
		for (const c of this.el.querySelectorAll('.ed-cell')) {
			const on = !!path && /** @type {HTMLElement} */ (c).dataset.path === pathKey(path);
			c.classList.toggle('selected', on);
			if (on) c.setAttribute('aria-current', 'true');
			else c.removeAttribute('aria-current');
		}
	}

	/** @param {import('./draft.js').WidgetPath} path */
	focus(path) {
		const c = this.el.querySelector(`.ed-cell[data-path="${pathKey(path)}"]`);
		if (c instanceof HTMLElement) {
			c.focus({ preventScroll: true });
			c.scrollIntoView({ block: 'nearest' });
		}
	}

	/** @param {KeyboardEvent} e @param {import('./draft.js').WidgetPath} path @param {HTMLElement} cell @param {Record<string, any>} w */
	key(e, path, cell, w) {
		if (e.target !== cell) return;
		const hooks = this.opts.hooks;
		if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
			e.preventDefault();
			hooks.nudge(path, e.key === 'ArrowUp' ? -1 : 1);
		} else if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
			e.preventDefault();
			hooks.span(path, spanOf(this.opts.desc(), w) + (e.key === 'ArrowLeft' ? -1 : 1));
		} else if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			hooks.select(path, true, true);
		} else if (e.key === 'Delete' || e.key === 'Backspace') {
			e.preventDefault();
			hooks.remove(path);
		}
	}

	/**
	 * Drags a widget to another place: in its list, into a group or out of one. Starts at once from
	 * the handle; from the frame, once the mouse has gone `threshold` pixels (a click picks it).
	 * @param {PointerEvent} e @param {import('./draft.js').WidgetPath} from @param {HTMLElement} cell
	 * @param {HTMLElement} handle @param {string} name @param {number} threshold
	 */
	drag(e, from, cell, handle, name, threshold) {
		if (e.button !== 0) return;
		e.preventDefault();
		const x0 = e.clientX;
		const y0 = e.clientY;
		let active = false;
		/** @type {HTMLElement | null} */
		let ghost = null;
		/** @type {{ parent: import('./draft.js').WidgetPath, index: number, el: Element, where: string } | null} */
		let target = null;
		try {
			handle.setPointerCapture(e.pointerId);
		} catch {}
		const unmark = () => {
			for (const m of this.el.querySelectorAll('.drop-before, .drop-after, .drop-above, .drop-below, .drop-into')) m.classList.remove('drop-before', 'drop-after', 'drop-above', 'drop-below', 'drop-into');
		};
		const move = (/** @type {PointerEvent} */ ev) => {
			if (!active) {
				if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < threshold) return;
				active = true;
				this.busy = true;
				ghost = h('div', { class: 'ed-ghost', 'aria-hidden': 'true' }, name);
				document.body.append(ghost);
				cell.classList.add('dragged');
				this.el.classList.add('dragging');
			}
			setStyle(/** @type {HTMLElement} */ (ghost), { transform: `translate(${ev.clientX + 12}px, ${ev.clientY + 12}px)` });
			unmark();
			target = this.dropAt(ev.clientX, ev.clientY, from);
			if (target) target.el.classList.add(`drop-${target.where}`);
			if (ev.clientY < 56) window.scrollBy(0, -14);
			else if (ev.clientY > window.innerHeight - 56) window.scrollBy(0, 14);
		};
		const end = (/** @type {boolean} */ commit) => {
			handle.removeEventListener('pointermove', move);
			handle.removeEventListener('pointerup', up);
			handle.removeEventListener('pointercancel', cancel);
			window.removeEventListener('keydown', esc, true);
			unmark();
			if (ghost) ghost.remove();
			cell.classList.remove('dragged');
			this.el.classList.remove('dragging');
			if (active) {
				this.justDragged = true;
				setTimeout(() => (this.justDragged = false), 0);
			}
			this.busy = false;
			if (commit && active && target) this.opts.hooks.move(from, target.parent, target.index);
			else if (!active && threshold === 0) this.opts.hooks.select(from, false);
			else if (this.pending) this.render();
		};
		const up = () => end(true);
		const cancel = () => end(false);
		const esc = (/** @type {KeyboardEvent} */ ev) => {
			if (ev.key === 'Escape') {
				ev.preventDefault();
				ev.stopPropagation();
				end(false);
			}
		};
		handle.addEventListener('pointermove', move);
		handle.addEventListener('pointerup', up);
		handle.addEventListener('pointercancel', cancel);
		window.addEventListener('keydown', esc, true);
	}

	/**
	 * Where a widget dragged to a point goes: before or after the cell under it, or at the end of
	 * a list (into a group). Never into itself.
	 * @param {number} x @param {number} y @param {import('./draft.js').WidgetPath} from
	 */
	dropAt(x, y, from) {
		const under = document.elementFromPoint(x, y);
		if (!under || !this.el.contains(under)) return null;
		const zone = under.closest('.ed-drop');
		if (zone instanceof HTMLElement) {
			const parent = parsePath(zone.dataset.parent || '');
			if (within(from, parent)) return null;
			return { parent, index: Number(zone.dataset.index), el: zone, where: 'into' };
		}
		const cell = under.closest('.ed-cell');
		if (!(cell instanceof HTMLElement) || !cell.parentElement) return null;
		const p = parsePath(cell.dataset.path || '');
		if (within(from, p)) return null;
		const r = cell.getBoundingClientRect();
		const g = cell.parentElement.getBoundingClientRect();
		// Beside a narrow cell, the pointer's side says before or after; above or below a wide one.
		const wide = r.width > g.width * 0.75;
		const after = wide ? y > r.top + r.height / 2 : x > r.left + r.width / 2;
		const where = wide ? (after ? 'below' : 'above') : after ? 'after' : 'before';
		return { parent: p.slice(0, -1), index: p[p.length - 1] + (after ? 1 : 0), el: cell, where };
	}

	/**
	 * Sizes a widget by its handle: the width follows the pointer, in columns of the grid it is in.
	 * @param {PointerEvent} e @param {import('./draft.js').WidgetPath} path @param {HTMLElement} cell @param {number} span
	 */
	resize(e, path, cell, span) {
		if (e.button !== 0 || columns() < 4 || !cell.parentElement) return;
		e.preventDefault();
		e.stopPropagation();
		const handle = /** @type {HTMLElement} */ (e.currentTarget);
		const grid = cell.parentElement;
		const { min, max } = spanRange(this.opts.desc());
		const was = [...cell.classList].find((c) => c.startsWith('ed-w4-')) || '';
		let now = span;
		try {
			handle.setPointerCapture(e.pointerId);
		} catch {}
		cell.classList.add('resizing');
		this.busy = true;
		const move = (/** @type {PointerEvent} */ ev) => {
			const gr = grid.getBoundingClientRect();
			const gap = parseFloat(getComputedStyle(grid).columnGap) || 0;
			const col = (gr.width - gap * 3) / 4;
			const r = cell.getBoundingClientRect();
			now = Math.max(min, Math.min(max, Math.round((ev.clientX - r.left + gap) / (col + gap))));
			cell.classList.remove(...[...cell.classList].filter((c) => c.startsWith('ed-w4-')));
			cell.classList.add(`ed-w4-${now}`);
		};
		const up = () => {
			handle.removeEventListener('pointermove', move);
			handle.removeEventListener('pointerup', up);
			handle.removeEventListener('pointercancel', up);
			cell.classList.remove('resizing');
			this.busy = false;
			this.justDragged = true;
			setTimeout(() => (this.justDragged = false), 0);
			if (now !== span) this.opts.hooks.span(path, now);
			else if (this.pending) this.render();
			else if (was) {
				cell.classList.remove(...[...cell.classList].filter((c) => c.startsWith('ed-w4-')));
				cell.classList.add(was);
			}
		};
		handle.addEventListener('pointermove', move);
		handle.addEventListener('pointerup', up);
		handle.addEventListener('pointercancel', up);
	}
}

/** The shape of a draft (which widget where, of which type and width), to tell a move from new data. @param {Record<string, any>} dash */
function shapeOf(dash) {
	/** @param {any[]} list @returns {any[]} */
	const walk = (list) => list.map((w) => [w && w.type, w && w.id, w && w.span, w && Array.isArray(w[WIDGETS]) ? walk(w[WIDGETS]) : null]);
	return JSON.stringify(walk(Array.isArray(dash[WIDGETS]) ? dash[WIDGETS] : []));
}
