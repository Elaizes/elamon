// A text field with a list to pick from as it is typed in (values with their readings now, actions,
// dashboards, collections…). What is typed stays, picked or not: a `for_each` dashboard's
// `{item.unit}` is no option, and a value the computer doesn't have now is kept.
import { h, clear } from '../dom.js';
import { search } from './describe.js';

let seq = 0;

/**
 * @param {{ value?: string, options: () => import('./describe.js').Option[], placeholder?: string, id?: string,
 *   label?: string, onInput?: (text: string) => void, onPick?: (option: import('./describe.js').Option) => void, clearOnPick?: boolean,
 *   invalid?: boolean }} opts
 * @returns {{ el: HTMLElement, input: HTMLInputElement }}
 */
export function picker(opts) {
	const listId = `ed-list-${++seq}`;
	const input = h('input', {
		type: 'text',
		id: opts.id,
		value: opts.value ?? '',
		placeholder: opts.placeholder,
		role: 'combobox',
		autocomplete: 'off',
		spellcheck: 'false',
		'aria-autocomplete': 'list',
		'aria-expanded': 'false',
		'aria-controls': listId,
		'aria-label': opts.id ? null : opts.label,
		class: 'mono',
	});
	if (opts.invalid) input.setAttribute('aria-invalid', 'true');
	const list = h('ul', { id: listId, class: 'ed-options', role: 'listbox', hidden: true });
	const el = h('div', { class: 'ed-combo' }, input, list);
	/** @type {import('./describe.js').Option[]} */
	let shown = [];
	let active = -1;

	const close = () => {
		list.hidden = true;
		input.setAttribute('aria-expanded', 'false');
		input.removeAttribute('aria-activedescendant');
		active = -1;
	};
	const mark = () => {
		[...list.children].forEach((li, i) => li.setAttribute('aria-selected', i === active ? 'true' : 'false'));
		const li = list.children[active];
		if (li) {
			input.setAttribute('aria-activedescendant', li.id);
			li.scrollIntoView({ block: 'nearest' });
		} else input.removeAttribute('aria-activedescendant');
	};
	const open = () => {
		shown = search(opts.options(), input.value);
		clear(list);
		shown.forEach((o, i) =>
			list.append(
				h(
					'li',
					{
						id: `${listId}-${i}`,
						role: 'option',
						'aria-selected': 'false',
						onpointerdown: (/** @type {PointerEvent} */ e) => {
							e.preventDefault();
							pick(o);
						},
					},
					h('span', { class: 'ed-opt-value mono' }, o.value),
					o.label && o.label !== o.value ? h('span', { class: 'ed-opt-label' }, o.label) : null,
					o.detail ? h('span', { class: 'ed-opt-detail' }, o.detail) : null,
				),
			),
		);
		list.hidden = !shown.length;
		input.setAttribute('aria-expanded', shown.length ? 'true' : 'false');
		active = -1;
		mark();
	};
	/** @param {import('./describe.js').Option} o */
	const pick = (o) => {
		close();
		if (opts.clearOnPick) input.value = '';
		else {
			input.value = o.value;
			opts.onInput?.(o.value);
		}
		opts.onPick?.(o);
	};

	input.addEventListener('focus', open);
	input.addEventListener('input', () => {
		opts.onInput?.(input.value);
		open();
	});
	input.addEventListener('blur', () => setTimeout(close, 120));
	input.addEventListener('keydown', (e) => {
		if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault();
			if (list.hidden) open();
			if (!shown.length) return;
			active = (active + (e.key === 'ArrowDown' ? 1 : -1) + shown.length) % shown.length;
			mark();
		} else if (e.key === 'Enter' && !list.hidden && active >= 0) {
			e.preventDefault();
			pick(shown[active]);
		} else if (e.key === 'Escape' && !list.hidden) {
			e.preventDefault();
			e.stopPropagation();
			close();
		}
	});
	return { el, input };
}
