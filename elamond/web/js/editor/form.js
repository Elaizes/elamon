// Forms made from the service's description: one field per key, its control chosen by the key's
// `kind`. A value of a kind the form doesn't know, or one that doesn't fit its key, is offered as
// JSON text and kept as it is until changed: nothing a form is given is dropped.
import { h, clear } from '../dom.js';
import { S, fill } from '../strings.js';
import { picker } from './picker.js';
import { refOptions, valueOptions, anyOf, whichOf, fits } from './describe.js';
import { isId } from './draft.js';

/**
 * What every field is handed: the description, the mistakes the service found by key, the keys a
 * lower file sets with where (they can be changed, not taken out), and what to do after a change.
 * A widget made from a template has the template's keys under `template`, to say what it sets.
 * @typedef {{ desc: import('./describe.js').Editor, changed: () => void, mistakes?: Map<string, string[]>,
 *   below?: Map<string, string>, addInto?: () => void, template?: { name: string, keys: Record<string, any> } }} FormCtx
 */

let seq = 0;
const nextId = () => `ed-f-${++seq}`;

/** Every kind's control, by `kind`. Any other is JSON text. */
const KNOWN = new Set(['string', 'text', 'id', 'address', 'ref', 'bool', 'int', 'number', 'scalar', 'enum', 'format', 'list', 'map', 'record', 'widgets', 'any']);

/** @param {unknown} v */
const isEmpty = (v) => v === undefined || (Array.isArray(v) && !v.length) || (!!v && typeof v === 'object' && !Array.isArray(v) && !Object.keys(v).length);

/**
 * A field for each key, of the object they are keys of (changed in place).
 * @param {FormCtx} fc @param {import('./describe.js').EditorKey[]} keys @param {Record<string, any>} obj
 */
export function keyFields(fc, keys, obj) {
	return keys.filter((k) => k && typeof k.key === 'string').map((k) => keyField(fc, k, obj));
}

/**
 * One key's field: its label (marked when required), the control, its hint, what the service said
 * of it, and a way to take it out.
 * @param {FormCtx} fc @param {import('./describe.js').EditorKey} key @param {Record<string, any>} obj
 * @returns {HTMLElement}
 */
export function keyField(fc, key, obj) {
	const name = /** @type {string} */ (key.key);
	const el = h('div', { class: 'ed-field', 'data-key': name });
	const draw = () => {
		clear(el);
		const id = nextId();
		const has = Object.prototype.hasOwnProperty.call(obj, name);
		const value = obj[name];
		const fromBelow = !!fc.below && fc.below.has(name);
		/** @param {unknown} v */
		const set = (v) => {
			if (isEmpty(v) && !key.required && !fromBelow) delete obj[name];
			else if (v === undefined) delete obj[name];
			else obj[name] = v;
			fc.changed();
		};
		const label = key.label || name;
		const head = h('div', { class: 'ed-field-head' });
		const ctl = control(fc, key, value, set, id, label);
		if (key.kind === 'bool' && fits(key, value ?? false)) {
			head.append(h('label', { class: 'ed-check', for: id }, ctl, h('span', null, label), key.required ? h('span', { class: 'ed-req', title: S.editor.required }, '*') : null));
		} else {
			head.append(h('label', { for: id }, label, key.required ? h('span', { class: 'ed-req', title: S.editor.required }, '*') : null));
		}
		// A group's widgets go one by one, on the canvas, never all at once.
		if (has && !key.required && key.kind !== 'widgets') {
			const takeOut = h(
				'button',
				{
					type: 'button',
					class: 'quiet ed-unset',
					title: fromBelow ? S.editor.cantUnset : S.editor.unset,
					'aria-label': fill(S.editor.unsetKey, label),
					disabled: fromBelow,
					onclick: () => {
						delete obj[name];
						fc.changed();
						draw();
					},
				},
				S.editor.unsetShort,
			);
			head.append(takeOut);
		}
		el.append(head);
		if (!(key.kind === 'bool' && fits(key, value ?? false))) el.append(ctl);
		const hint = [key.hint, key.default !== undefined && key.kind !== 'bool' ? fill(S.editor.byDefault, show(key.default)) : null, key.kind === 'bool' && key.default !== undefined ? fill(S.editor.byDefault, key.default ? S.editor.on : S.editor.off) : null]
			.filter(Boolean)
			.join(' · ');
		if (hint) el.append(h('p', { class: 'ed-hint muted' }, hint));
		if (!has && fc.template && Object.hasOwn(fc.template.keys, name)) {
			const v = fc.template.keys[name];
			el.append(h('p', { class: 'ed-hint ed-template' }, v !== null && typeof v === 'object' ? fill(S.editor.templateSets, fc.template.name) : fill(S.editor.templateKey, fc.template.name, show(v))));
		}
		if (fromBelow && fc.below) el.append(h('p', { class: 'ed-hint ed-below' }, fill(S.editor.fromBelow, fc.below.get(name) || S.editor.theDefaults)));
		markField(el, fc.mistakes ? fc.mistakes.get(name) || [] : []);
	};
	draw();
	return el;
}

/**
 * Puts what the service said of a key under its field, in place of what it said before.
 * @param {HTMLElement} el @param {string[]} mistakes
 */
export function markField(el, mistakes) {
	for (const old of [...el.children].filter((c) => c.classList.contains('ed-mistake'))) old.remove();
	el.classList.toggle('bad', mistakes.length > 0);
	for (const m of mistakes) el.append(h('p', { class: 'ed-mistake', role: 'alert' }, m));
}

/**
 * A field for a key no description names: its name, its value as JSON, and taking it out.
 * @param {FormCtx} fc @param {string} name @param {Record<string, any>} obj
 */
export function otherField(fc, name, obj) {
	const id = nextId();
	const fromBelow = !!fc.below && fc.below.has(name);
	const el = h(
		'div',
		{ class: 'ed-field ed-other', 'data-key': name },
		h(
			'div',
			{ class: 'ed-field-head' },
			h('label', { for: id, class: 'mono' }, name),
			h(
				'button',
				{
					type: 'button',
					class: 'quiet ed-unset',
					title: fromBelow ? S.editor.cantUnset : S.editor.unset,
					'aria-label': fill(S.editor.unsetKey, name),
					disabled: fromBelow,
					onclick: () => {
						delete obj[name];
						fc.changed();
						el.remove();
					},
				},
				S.editor.unsetShort,
			),
		),
		jsonControl(obj[name], (v) => {
			if (v === undefined) delete obj[name];
			else obj[name] = v;
			fc.changed();
		}, id),
		h('p', { class: 'ed-hint muted' }, S.editor.otherHint),
	);
	markField(el, fc.mistakes ? fc.mistakes.get(name) || [] : []);
	return el;
}

/** A value as a form shows it in a hint. @param {unknown} v */
function show(v) {
	return typeof v === 'string' ? v : JSON.stringify(v);
}

/** What a new item of a list (or row of a map) starts as. @param {import('./describe.js').EditorKey | undefined} k @returns {any} */
function emptyOf(k) {
	if (!k) return '';
	switch (k.kind) {
		case 'number':
		case 'int':
			return typeof k.min === 'number' ? k.min : 0;
		case 'bool':
			return false;
		case 'enum':
		case 'format':
			return k.choices && k.choices.length ? k.choices[0] : '';
		case 'list':
		case 'widgets':
			return [];
		case 'map':
		case 'record':
			return {};
		case 'any':
			return emptyOf(anyOf(k)[0]);
		default:
			return '';
	}
}

/**
 * The control for a value of a key's kind. `set` takes the new value (`undefined` takes it out).
 * @param {FormCtx} fc @param {import('./describe.js').EditorKey} k @param {any} value
 * @param {(v: any) => void} set @param {string} id @param {string} label
 * @returns {HTMLElement}
 */
export function control(fc, k, value, set, id, label) {
	if (!KNOWN.has(k.kind) || (value !== undefined && !fits(k, value) && k.kind !== 'any')) return jsonControl(value, set, id, !KNOWN.has(k.kind) ? S.editor.unknownKind : S.editor.doesntFit);
	const placeholder = k.default !== undefined && typeof k.default !== 'object' ? String(k.default) : '';
	switch (k.kind) {
		case 'string':
			return textInput(value, set, id, placeholder, label);
		case 'text':
			return textWithValues(fc, value, set, id, placeholder, label);
		case 'id':
			return idInput(value, set, id, placeholder);
		case 'address':
			return picker({ id, value, label, placeholder: placeholder || S.editor.pickValue, options: () => valueOptions(fc.desc), onInput: (t) => set(t || undefined) }).el;
		case 'ref': {
			const opts = refOptions(fc.desc, k.to);
			return picker({ id, value, label, placeholder: placeholder || S.editor.refs[k.to || ''] || '', options: () => opts, onInput: (t) => set(t || undefined) }).el;
		}
		case 'bool': {
			const box = h('input', { type: 'checkbox', id });
			box.checked = value === undefined ? !!k.default : !!value;
			box.addEventListener('change', () => set(box.checked));
			return box;
		}
		case 'int':
		case 'number':
			return numberInput(k, value, set, id, placeholder);
		case 'scalar':
			return scalarInput(value, set, id, placeholder);
		case 'enum':
			return enumSelect(k, value, set, id);
		case 'format':
			return formatControl(k, value, set, id);
		case 'list':
			return listControl(fc, k, value, set, label);
		case 'map':
			return mapControl(fc, k, value, set, label);
		case 'record':
			return recordControl(fc, k, value, set, label);
		case 'widgets':
			return widgetsNote(fc, k, value, id);
		case 'any':
			return anyControl(fc, k, value, set, id, label);
		default:
			return jsonControl(value, set, id, S.editor.unknownKind);
	}
}

/** @param {any} value @param {(v: any) => void} set @param {string} id @param {string} placeholder @param {string} label */
function textInput(value, set, id, placeholder, label) {
	const input = h('input', { type: 'text', id, value: value ?? '', placeholder, 'aria-label': label });
	input.addEventListener('input', () => set(input.value === '' ? undefined : input.value));
	return input;
}

/** A `text`: words that may hold values, `{cpu.usage}`, with a way to put one in where the caret is. */
function textWithValues(fc, value, set, id, placeholder, label) {
	const input = h('input', { type: 'text', id, value: value ?? '', placeholder });
	input.addEventListener('input', () => set(input.value === '' ? undefined : input.value));
	const insert = picker({
		placeholder: S.editor.insertValue,
		label: fill(S.editor.insertValueInto, label),
		options: () => valueOptions(fc.desc),
		clearOnPick: true,
		onPick: (o) => {
			const at = input.selectionStart ?? input.value.length;
			const end = input.selectionEnd ?? at;
			const token = `{${o.value}}`;
			input.value = input.value.slice(0, at) + token + input.value.slice(end);
			input.setSelectionRange(at + token.length, at + token.length);
			input.dispatchEvent(new Event('input'));
			input.focus();
		},
	});
	insert.el.classList.add('ed-insert');
	return h('div', { class: 'ed-text' }, input, insert.el);
}

function idInput(value, set, id, placeholder) {
	const input = h('input', { type: 'text', id, value: value ?? '', placeholder, class: 'mono', spellcheck: 'false', autocapitalize: 'off' });
	const note = h('p', { class: 'ed-local', hidden: true }, S.editor.badId);
	input.addEventListener('input', () => {
		const ok = input.value === '' || isId(input.value);
		note.hidden = ok;
		input.setAttribute('aria-invalid', ok ? 'false' : 'true');
		if (ok) set(input.value === '' ? undefined : input.value);
	});
	return h('div', null, input, note);
}

/** @param {import('./describe.js').EditorKey} k */
function numberInput(k, value, set, id, placeholder) {
	const int = k.kind === 'int';
	const input = h('input', { type: 'number', id, value: value ?? '', placeholder, step: int ? '1' : 'any', min: k.min, max: k.max, inputmode: int ? 'numeric' : 'decimal' });
	const range = typeof k.min === 'number' && typeof k.max === 'number' ? fill(S.editor.between, k.min, k.max) : typeof k.min === 'number' ? fill(S.editor.atLeast, k.min) : typeof k.max === 'number' ? fill(S.editor.atMost, k.max) : '';
	const note = h('p', { class: 'ed-local', hidden: true });
	input.addEventListener('input', () => {
		if (input.value === '') {
			note.hidden = true;
			input.setAttribute('aria-invalid', 'false');
			set(undefined);
			return;
		}
		const n = Number(input.value);
		let why = '';
		if (!Number.isFinite(n) || input.validity.badInput) why = S.editor.notNumber;
		else if (int && !Number.isInteger(n)) why = S.editor.notWhole;
		else if ((typeof k.min === 'number' && n < k.min) || (typeof k.max === 'number' && n > k.max)) why = range;
		note.textContent = why;
		note.hidden = !why;
		input.setAttribute('aria-invalid', why ? 'true' : 'false');
		if (!why) set(n);
	});
	return h('div', null, input, note);
}

/** A string, number or bool: a number or `true`/`false` is taken as one; quotes keep it words. */
function scalarInput(value, set, id, placeholder) {
	const text = typeof value === 'string' && (/^(true|false)$/.test(value) || (value.trim() !== '' && Number.isFinite(Number(value)))) ? JSON.stringify(value) : value === undefined ? '' : String(value);
	const input = h('input', { type: 'text', id, value: text, placeholder, class: 'mono' });
	input.addEventListener('input', () => set(parseScalar(input.value)));
	return h('div', null, input, h('p', { class: 'ed-hint muted' }, S.editor.scalarHint));
}

/** @param {string} t @returns {string | number | boolean | undefined} */
export function parseScalar(t) {
	if (t === '') return undefined;
	if (t === 'true') return true;
	if (t === 'false') return false;
	if (/^".*"$/.test(t)) return t.slice(1, -1);
	const n = Number(t);
	return t.trim() !== '' && Number.isFinite(n) ? n : t;
}

/** @param {import('./describe.js').EditorKey} k */
function enumSelect(k, value, set, id) {
	const sel = h('select', { id });
	sel.append(h('option', { value: '' }, k.default !== undefined ? fill(S.editor.defaultChoice, show(k.default)) : S.editor.noChoice));
	for (const c of k.choices || []) sel.append(h('option', { value: c }, c));
	sel.value = value ?? '';
	sel.addEventListener('change', () => set(sel.value === '' ? undefined : sel.value));
	return sel;
}

/** One of `choices`, or a pattern of one's own (`{:.1}`). @param {import('./describe.js').EditorKey} k */
function formatControl(k, value, set, id) {
	const choices = k.choices || [];
	const PATTERN = '\u0000pattern';
	const sel = h('select', { id });
	sel.append(h('option', { value: '' }, k.default !== undefined ? fill(S.editor.defaultChoice, show(k.default)) : S.editor.noChoice));
	for (const c of choices) sel.append(h('option', { value: c }, c));
	sel.append(h('option', { value: PATTERN }, S.editor.pattern));
	const own = typeof value === 'string' && !choices.includes(value);
	const pattern = h('input', { type: 'text', class: 'mono', value: own ? value : '', placeholder: S.editor.patternExample, 'aria-label': S.editor.pattern, hidden: !own });
	sel.value = own ? PATTERN : (value ?? '');
	sel.addEventListener('change', () => {
		pattern.hidden = sel.value !== PATTERN;
		if (sel.value === PATTERN) {
			pattern.focus();
			set(pattern.value || undefined);
		} else set(sel.value === '' ? undefined : sel.value);
	});
	pattern.addEventListener('input', () => set(pattern.value || undefined));
	return h('div', { class: 'ed-format' }, sel, pattern);
}

/** A list: a row for each item, of the `of` kind, to add, take out and put in order. */
function listControl(fc, k, value, set, label) {
	const of = k.of && !Array.isArray(k.of) ? k.of : { kind: 'json' };
	/** @type {any[]} */
	const cur = Array.isArray(value) ? value : [];
	const rows = h('ol', { class: 'ed-rows' });
	const el = h('div', { class: 'ed-list' }, rows);
	// An item that is a table or list of its own gets the whole width, its row's buttons above it.
	const block = ['record', 'map', 'list', 'any'].includes(of.kind);
	const draw = () => {
		clear(rows);
		cur.forEach((item, i) => {
			const id = nextId();
			rows.append(
				h(
					'li',
					{ class: block ? 'ed-row ed-row-block' : 'ed-row' },
					h('div', { class: 'ed-row-value' }, control(fc, of, item, (v) => {
						cur[i] = v === undefined ? emptyOf(of) : v;
						set(cur);
					}, id, fill(S.editor.item, label, i + 1))),
					h(
						'div',
						{ class: 'ed-row-tools' },
						rowButton(S.editor.up, fill(S.editor.itemUp, i + 1), i === 0, () => {
							[cur[i - 1], cur[i]] = [cur[i], cur[i - 1]];
							set(cur);
							draw();
						}),
						rowButton(S.editor.down, fill(S.editor.itemDown, i + 1), i === cur.length - 1, () => {
							[cur[i + 1], cur[i]] = [cur[i], cur[i + 1]];
							set(cur);
							draw();
						}),
						rowButton(S.editor.remove, fill(S.editor.itemRemove, i + 1), false, () => {
							cur.splice(i, 1);
							set(cur);
							draw();
						}),
					),
				),
			);
		});
	};
	draw();
	el.append(
		h('button', {
			type: 'button',
			class: 'ed-add-row',
			onclick: () => {
				cur.push(emptyOf(of));
				set(cur);
				draw();
				const last = rows.lastElementChild && rows.lastElementChild.querySelector('input, select, textarea');
				if (last instanceof HTMLElement) last.focus();
			},
		}, fill(S.editor.addItem, label)),
	);
	return el;
}

/** A map: rows of a key and a value of the `of` kind. Rows without a key aren't written. */
function mapControl(fc, k, value, set, label) {
	const of = k.of && !Array.isArray(k.of) ? k.of : { kind: 'json' };
	/** @type {[string, any][]} */
	const entries = value && typeof value === 'object' && !Array.isArray(value) ? Object.entries(value) : [];
	const write = () => {
		/** @type {Record<string, any>} */
		const out = {};
		for (const [key, v] of entries) if (key !== '') out[key] = v;
		set(out);
	};
	const rows = h('ul', { class: 'ed-rows' });
	const el = h('div', { class: 'ed-map' }, rows);
	const draw = () => {
		clear(rows);
		entries.forEach((entry, i) => {
			const keyInput = h('input', { type: 'text', value: entry[0], placeholder: S.editor.mapKey, 'aria-label': fill(S.editor.mapKeyOf, label, i + 1), class: 'mono ed-map-key' });
			keyInput.addEventListener('input', () => {
				entry[0] = keyInput.value;
				write();
			});
			rows.append(
				h(
					'li',
					{ class: 'ed-row' },
					keyInput,
					h('div', { class: 'ed-row-value' }, control(fc, of, entry[1], (v) => {
						entry[1] = v === undefined ? emptyOf(of) : v;
						write();
					}, nextId(), fill(S.editor.mapValueOf, label, entry[0] || i + 1))),
					h('div', { class: 'ed-row-tools' }, rowButton(S.editor.remove, fill(S.editor.itemRemove, i + 1), false, () => {
						entries.splice(i, 1);
						write();
						draw();
					})),
				),
			);
		});
	};
	draw();
	el.append(
		h('button', {
			type: 'button',
			class: 'ed-add-row',
			onclick: () => {
				entries.push(['', emptyOf(of)]);
				draw();
				const last = rows.lastElementChild && rows.lastElementChild.querySelector('input');
				if (last instanceof HTMLElement) last.focus();
			},
		}, fill(S.editor.addItem, label)),
	);
	return el;
}

/** A record: the keys `records[record]` names, as a nested form; the keys it doesn't name, as JSON. */
function recordControl(fc, k, value, set, label) {
	const keys = k.record ? fc.desc.records[k.record] : null;
	if (!Array.isArray(keys)) return jsonControl(value, set, nextId(), S.editor.unknownKind);
	/** @type {Record<string, any>} */
	const cur = value && typeof value === 'object' && !Array.isArray(value) ? value : {};
	/** @type {FormCtx} */
	const inner = { desc: fc.desc, changed: () => set(cur) };
	const names = new Set(keys.map((x) => x.key));
	return h(
		'fieldset',
		{ class: 'ed-record' },
		h('legend', { class: 'ed-sr' }, label),
		...keyFields(inner, keys, cur),
		...Object.keys(cur)
			.filter((n) => !names.has(n))
			.map((n) => otherField(inner, n, cur)),
	);
}

/** A group's widgets: they are edited on the canvas. */
function widgetsNote(fc, k, value, id) {
	const n = Array.isArray(value) ? value.length : 0;
	return h(
		'div',
		{ class: 'ed-widgets-note', id },
		h('span', { class: 'muted' }, fill(n === 1 ? S.editor.widgetsOne : S.editor.widgetsMany, n)),
		fc.addInto ? h('button', { type: 'button', onclick: () => fc.addInto && fc.addInto() }, S.editor.addInto) : null,
	);
}

/** One of several shapes: which, then that shape's control. @param {import('./describe.js').EditorKey} k */
function anyControl(fc, k, value, set, id, label) {
	const shapes = anyOf(k);
	let which = whichOf(k, value);
	if (value !== undefined && which < 0) return jsonControl(value, set, id, S.editor.doesntFit);
	const sel = h('select', { 'aria-label': fill(S.editor.shapeOf, label), class: 'ed-shape' });
	shapes.forEach((s, i) => sel.append(h('option', { value: String(i) }, shapeLabel(s))));
	const slot = h('div', { class: 'ed-shape-value' });
	const draw = (/** @type {any} */ v) => {
		clear(slot);
		slot.append(control(fc, shapes[which], v, set, id, label));
	};
	if (which < 0) which = 0;
	sel.value = String(which);
	sel.addEventListener('change', () => {
		which = Number(sel.value);
		set(undefined);
		draw(undefined);
	});
	draw(value);
	return h('div', { class: 'ed-any' }, shapes.length > 1 ? sel : null, slot);
}

/** What a shape of an `any` is called: its label, else its kind's words. @param {import('./describe.js').EditorKey} s */
function shapeLabel(s) {
	if (s.label) return s.label;
	if (s.kind === 'ref' && s.to && S.editor.refKinds[s.to]) return S.editor.refKinds[s.to];
	if (s.kind === 'list' && s.of && !Array.isArray(s.of)) return fill(S.editor.kinds.listOf, shapeLabel(s.of));
	return S.editor.kinds[s.kind] || s.kind;
}

/**
 * A value as JSON text, for a kind the form doesn't know or a value that doesn't fit its key. Taken
 * only once it reads as JSON; emptied, the key is taken out.
 * @param {any} value @param {(v: any) => void} set @param {string} id @param {string} [why]
 */
export function jsonControl(value, set, id, why) {
	const area = h('textarea', { id, class: 'ed-json', spellcheck: 'false', rows: value !== undefined && typeof value === 'object' ? 4 : 1 });
	area.value = value === undefined ? '' : JSON.stringify(value, null, 2);
	const note = h('p', { class: 'ed-local', hidden: true });
	area.addEventListener('input', () => {
		if (area.value.trim() === '') {
			note.hidden = true;
			set(undefined);
			return;
		}
		try {
			const v = JSON.parse(area.value);
			note.hidden = true;
			area.setAttribute('aria-invalid', 'false');
			set(v);
		} catch (e) {
			note.textContent = fill(S.editor.notJson, e instanceof Error ? e.message : String(e));
			note.hidden = false;
			area.setAttribute('aria-invalid', 'true');
		}
	});
	return h('div', { class: 'ed-jsonbox' }, area, why ? h('p', { class: 'ed-hint muted' }, why) : null, note);
}

/** A small button of a row: up, down, out. */
function rowButton(text, label, disabled, onclick) {
	return h('button', { type: 'button', class: 'quiet ed-icon', 'aria-label': label, title: label, disabled, onclick }, text);
}
