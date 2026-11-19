// `bars`: one bar per item of a collection, each in its level's colour, its figure under it.
import { h, setText, setStyle } from '../dom.js';
import { card, captionLine, setLevel, captionOf } from './common.js';
import { S, fill } from '../strings.js';
import * as F from '../format.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const { el, body } = card(w, env);
	const chart = h('div', { class: 'bars', role: 'img' });
	const cells = h('div', { class: 'bar-cells' });
	const cap = captionLine();
	body.append(chart, cells, cap.el);
	/** @type {{ bar: HTMLElement, fill: HTMLElement, cell: HTMLElement, label: HTMLElement, value: HTMLElement }[]} */
	const drawn = [];
	/** @param {import('./common.js').WidgetState | undefined} s */
	const show = (s) => {
		const values = s && Array.isArray(s.v) ? s.v.map((x) => (typeof x === 'number' ? x : null)) : [];
		const max = (typeof w.max === 'number' ? w.max : s && typeof s.max === 'number' ? s.max : 100) || 1;
		while (drawn.length < values.length) {
			const f = h('div', { class: 'bars-fill' });
			const bar = h('div', { class: 'bars-bar' }, f);
			const label = h('span', { class: 'bar-cell-label muted' });
			const value = h('span', { class: 'bar-cell-value' });
			const cell = h('div', { class: 'bar-cell' }, label, value);
			chart.append(bar);
			cells.append(cell);
			drawn.push({ bar, fill: f, cell, label, value });
		}
		while (drawn.length > values.length) {
			const d = /** @type {(typeof drawn)[number]} */ (drawn.pop());
			d.bar.remove();
			d.cell.remove();
		}
		values.forEach((v, i) => {
			const d = drawn[i];
			const level = s && Array.isArray(s.levels) ? s.levels[i] : null;
			setStyle(d.fill, { height: `${(Math.min(Math.max((v ?? 0) / max, 0), 1) * 100).toFixed(1)}%` });
			setLevel(d.fill, level, null);
			setLevel(d.value, level, null);
			setText(d.label, s && Array.isArray(s.labels) && s.labels[i] != null ? s.labels[i] : String(i));
			setText(d.value, v == null ? F.DASH : F.number(v, w.format, w.unit));
		});
		const known = /** @type {number[]} */ (values.filter((v) => v != null));
		if (known.length) {
			const avg = known.reduce((a, b) => a + b, 0) / known.length;
			chart.setAttribute('aria-label', fill(S.panels.barsSummary, w.label || '', values.length, F.number(avg, w.format, w.unit), F.number(Math.max(...known), w.format, w.unit)));
		} else chart.removeAttribute('aria-label');
		cap.set(captionOf(w, s));
	};
	show(undefined);
	return { el, update: (s) => show(s), clear: () => show(undefined) };
}
