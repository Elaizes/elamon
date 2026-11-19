// The keys of the widget picked on the canvas, in a panel beside it (a sheet from the bottom on a
// narrow screen): its type's keys, those every widget takes, and those no description names, as
// JSON. Its width, its place, and taking it out.
import { h } from '../dom.js';
import { S, fill } from '../strings.js';
import { keyFields, otherField, markField } from './form.js';
import { keysOf, unknownKeys, isGroup, nameOf, typeLabel, spanRange, templateOf } from './describe.js';
import { allPaths, widgetAt, pathKey } from './draft.js';

/**
 * @typedef {{
 *   desc: import('./describe.js').Editor, dash: import('./draft.js').DraftDashboard, path: import('./draft.js').WidgetPath,
 *   mistakes: Map<string, string[]>, changed: (shape: boolean) => void, close: () => void,
 *   span: (n: number) => void, nudge: (delta: -1 | 1) => void, moveInto: (parent: import('./draft.js').WidgetPath) => void,
 *   remove: () => void, addInto: (() => void) | null, as: string | null, width: number,
 * }} SideHooks
 */

/**
 * The form of one widget. `mark` puts the service's mistakes on its fields again without drawing
 * it anew (so a field being typed in keeps its caret).
 * @param {SideHooks} o @returns {{ el: HTMLElement, mark: (mistakes: Map<string, string[]>) => void }}
 */
export function widgetForm(o) {
	const { desc, dash, path } = o;
	const w = /** @type {Record<string, any>} */ (widgetAt(dash, path));
	const name = nameOf(desc, w);
	// A widget from a template has no type of its own: its keys are those of its template's type, and
	// what the template sets is said under each key the widget doesn't set itself.
	const { own, common } = keysOf(desc, w, o.as);
	const tpl = templateOf(desc, w);
	let type = w.type;
	/** @type {import('./form.js').FormCtx} */
	const fc = {
		desc,
		template: tpl && typeof w.use === 'string' ? { name: w.use, keys: tpl } : undefined,
		changed: () => {
			// A new type brings other keys: the form is drawn again for it.
			const shape = w.type !== type;
			type = w.type;
			o.changed(shape);
		},
		addInto: o.addInto || undefined,
	};
	// `span` has its own buttons above; every other key is a field.
	// `type` and `span` have their own controls above; every other key is a field (a description
	// that still lists `type` among the common keys doesn't make it twice).
	const ownFields = keyFields(fc, own.filter((k) => k.key !== 'span' && k.key !== 'type'), w);
	const commonFields = keyFields(fc, common.filter((k) => k.key !== 'span' && k.key !== 'type'), w);
	const others = unknownKeys(desc, w, o.as).map((k) => otherField(fc, k, w));
	const whole = h('div', { class: 'ed-whole' });

	// What it is: one of the types the service describes (one it doesn't is kept, and said).
	const types = h('select', { id: 'ed-type' });
	if (w.type === undefined) types.append(h('option', { value: '' }, o.as ? fill(S.editor.templateAs, String(w.use ?? '?'), typeLabel(desc, o.as)) : fill(S.editor.fromTemplate, String(w.use ?? '?'))));
	for (const t of desc.types) types.append(h('option', { value: t.type }, t.label && t.label !== t.type ? `${t.label} (${t.type})` : t.type));
	if (w.type !== undefined && !desc.types.some((t) => t.type === w.type)) types.append(h('option', { value: String(w.type) }, String(w.type)));
	types.value = String(w.type ?? '');
	types.addEventListener('change', () => {
		if (types.value === '') delete w.type;
		else w.type = types.value;
		fc.changed();
	});

	const { min, max } = spanRange(desc);
	const span = o.width;
	const widths = h('div', { class: 'ed-seg', role: 'group', 'aria-label': S.editor.width });
	for (let n = min; n <= max; n++) {
		widths.append(h('button', { type: 'button', 'aria-pressed': n === span ? 'true' : 'false', 'aria-label': fill(S.editor.widthN, n, max), onclick: () => o.span(n) }, String(n)));
	}

	// Where it can go: the dashboard itself, or a group (never itself or one inside it).
	const into = h('select', { id: 'ed-into' });
	into.append(h('option', { value: '' }, S.editor.topLevel));
	for (const p of allPaths(dash)) {
		const g = widgetAt(dash, p);
		if (!g || !isGroup(desc, g) || pathKey(p) === pathKey(path) || pathKey(p).startsWith(`${pathKey(path)}/`)) continue;
		into.append(h('option', { value: pathKey(p) }, `${'· '.repeat(p.length - 1)}${nameOf(desc, g)}`));
	}
	into.value = pathKey(path.slice(0, -1));
	into.addEventListener('change', () => o.moveInto(into.value ? into.value.split('/').map(Number) : []));

	const el = h(
		'div',
		{ class: 'ed-form' },
		h(
			'div',
			{ class: 'ed-side-head' },
			h('h2', { id: 'ed-side-title' }, name, ' ', h('small', { class: 'muted' }, w.type !== undefined ? typeLabel(desc, w.type) : o.as ? typeLabel(desc, o.as) : String(w.use ?? ''))),
			h('button', { type: 'button', class: 'quiet', onclick: o.close }, S.core.close),
		),
		whole,
		h('div', { class: 'ed-field' }, h('label', { for: 'ed-type' }, S.editor.type), types, h('p', { class: 'ed-hint muted' }, S.editor.typeHint)),
		h('div', { class: 'ed-field' }, h('span', { class: 'ed-label' }, S.editor.width), widths),
		h(
			'div',
			{ class: 'ed-field' },
			h('label', { for: 'ed-into' }, S.editor.place),
			h(
				'div',
				{ class: 'ed-place' },
				into,
				h('button', { type: 'button', 'aria-label': S.editor.moveUp, title: S.editor.moveUp, onclick: () => o.nudge(-1) }, S.editor.up),
				h('button', { type: 'button', 'aria-label': S.editor.moveDown, title: S.editor.moveDown, onclick: () => o.nudge(1) }, S.editor.down),
			),
		),
		ownFields.length ? h('section', { class: 'ed-section' }, h('h3', null, fill(S.editor.typeKeys, typeLabel(desc, w.type ?? o.as))), ...ownFields) : null,
		h('section', { class: 'ed-section' }, h('h3', null, S.editor.commonKeys), ...commonFields),
		others.length ? h('section', { class: 'ed-section' }, h('h3', null, S.editor.otherKeys), ...others) : null,
		h('div', { class: 'ed-row-buttons' }, h('button', { type: 'button', class: 'danger', onclick: o.remove }, S.editor.removeThis)),
	);
	if (w.type === undefined) whole.append(h('p', { class: 'note' }, fill(S.editor.templateNote, String(w.use ?? '?'))));
	else if (!desc.types.some((t) => t.type === w.type)) whole.append(h('p', { class: 'note warning' }, fill(S.editor.unknownType, String(w.type))));

	/** @param {Map<string, string[]>} m */
	const mark = (m) => {
		for (const f of el.querySelectorAll('.ed-section > .ed-field')) {
			const k = /** @type {HTMLElement} */ (f).dataset.key || '';
			markField(/** @type {HTMLElement} */ (f), m.get(k) || []);
		}
		markField(whole, m.get('') || []);
	};
	mark(o.mistakes);
	return { el, mark };
}
