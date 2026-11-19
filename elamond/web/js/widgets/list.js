// `list`: rows of a label and a value; a row with nothing to show is left out.
import { h, setText } from '../dom.js';
import { card, captionLine, setLevel, captionOf } from './common.js';
import * as F from '../format.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const { el, body } = card(w, env);
	const items = Array.isArray(w.items) ? w.items : [];
	const rows = items.map((item) => {
		const value = h('dd', { class: 'list-value' });
		if (item.format === 'temp') value.classList.add('temp');
		const row = h('div', { class: 'list-row' }, h('dt', { class: 'list-label' }, item.label || ''), value);
		return { item, row, value };
	});
	const dl = h('dl', { class: 'list' }, ...rows.map((r) => r.row));
	const cap = captionLine();
	body.append(dl, cap.el);
	/** @param {import('./common.js').WidgetState | undefined} s */
	const show = (s) => {
		const states = s && Array.isArray(s.items) ? s.items : [];
		rows.forEach(({ item, row, value }, i) => {
			const st = states[i];
			const text = st ? (typeof st.text === 'string' ? st.text : F.format(st.v, item.format, item.unit)) : null;
			// Before the first reading every row says it's coming; after, one with nothing is left out.
			row.hidden = !!s && text == null;
			setText(value, text ?? F.DASH);
			setLevel(value, st?.level, null);
		});
		cap.set(captionOf(w, s));
	};
	show(undefined);
	return { el, update: (s) => show(s), clear: () => show(undefined) };
}
