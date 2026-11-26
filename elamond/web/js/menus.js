// The page's menus: a button and what it opens (⋮, the theme, the layout), one kind of each.
import { h, setText } from './dom.js';
import { S, fill } from './strings.js';
import { icon } from './icons.js';
import { THEMES, THEME_ICONS, choose as chooseTheme, shown } from './theme.js';
import { PICKS, KEYS, current, choose as chooseLayout, reset } from './arrange.js';
import { onTheme } from './widgets/canvas.js';

/**
 * A button and the menu it opens, closed by a pick, Escape, a press elsewhere or the focus leaving it; focus goes to the
 * item picked now, or the first, and ↑ ↓ Home End move it among the items. The menu opens under the
 * button and ends under its right edge (⋮, last in a bar), or with `start` begins under its left one
 * (a button further in, whose menu would leave the window on a narrow screen); in the side bar's
 * foot it opens above (style.css).
 * @param {HTMLButtonElement} button @param {string} id @param {HTMLElement[]} items what is in it (groups too)
 * @param {boolean} [start]
 * @returns {{ wrap: HTMLElement, close: () => void }}
 */
export function dropdown(button, id, items, start = false) {
	button.setAttribute('aria-haspopup', 'menu');
	button.setAttribute('aria-expanded', 'false');
	button.setAttribute('aria-controls', id);
	const list = h('div', { id, role: 'menu', class: 'menu card' }, ...items);
	list.hidden = true;
	const wrap = h('div', { class: start ? 'menu-wrap start' : 'menu-wrap' }, button, list);
	const choices = () => /** @type {HTMLElement[]} */ ([...list.querySelectorAll('[role^="menuitem"]')]);
	const close = () => {
		list.hidden = true;
		button.setAttribute('aria-expanded', 'false');
		document.removeEventListener('pointerdown', outside);
	};
	const outside = (/** @type {Event} */ e) => {
		if (!wrap.contains(/** @type {Node} */ (e.target))) close();
	};
	// Focus gone elsewhere (Tab out of it): closed, so Escape and the page's keys are the page's again.
	wrap.addEventListener('focusout', (e) => {
		if (!list.hidden && !wrap.contains(/** @type {Node | null} */ (e.relatedTarget))) close();
	});
	button.addEventListener('click', () => {
		if (!list.hidden) return close();
		list.hidden = false;
		button.setAttribute('aria-expanded', 'true');
		document.addEventListener('pointerdown', outside);
		const all = choices();
		(all.find((i) => i.getAttribute('aria-checked') === 'true') || all[0])?.focus();
	});
	list.addEventListener('keydown', (e) => {
		if (e.key === 'Escape') {
			// The menu's: the page's keys (shell.js) don't take it for leaving the page.
			e.preventDefault();
			close();
			button.focus();
			return;
		}
		const all = choices();
		const at = all.indexOf(/** @type {HTMLElement} */ (document.activeElement));
		const to = e.key === 'ArrowDown' ? (at + 1) % all.length : e.key === 'ArrowUp' ? (at - 1 + all.length) % all.length : e.key === 'Home' ? 0 : e.key === 'End' ? all.length - 1 : -1;
		if (to < 0 || !all.length) return;
		e.preventDefault();
		all[to].focus();
	});
	return { wrap, close };
}

/** The theme menu: light, dark or black for this browser; its button shows the theme shown and follows it. */
export function themeMenu() {
	const button = h('button', { type: 'button', class: 'icon-button quiet' });
	const items = THEMES.map((t) => h('button', { type: 'button', role: 'menuitemradio', class: 'menu-item', 'data-pick': t }, icon(THEME_ICONS[t]), S.core.themes[t]));
	const menu = dropdown(button, 'theme-menu', items, true);
	const show = () => {
		const now = shown();
		const text = fill(S.core.themeIs, S.core.themes[now]);
		button.setAttribute('aria-label', text);
		button.title = text;
		button.replaceChildren(icon(THEME_ICONS[now]));
		for (const b of items) b.setAttribute('aria-checked', String(b.dataset.pick === now));
	};
	for (const b of items) {
		b.addEventListener('click', () => {
			chooseTheme(/** @type {import('./theme.js').Theme} */ (b.dataset.pick));
			menu.close();
			show();
			button.focus();
		});
	}
	show();
	return { el: menu.wrap, off: onTheme(show) };
}

/**
 * The layout menu: the page's width, its columns, its density and its edge, each a group of
 * choices with a tick on the one shown, and Reset; it stays open for the next pick.
 */
export function layoutMenu() {
	const L = S.core.layout;
	const button = h('button', { type: 'button', class: 'icon-button quiet', 'aria-label': L.button, title: L.button }, icon('layout'));
	/** @type {HTMLButtonElement[]} */
	const radios = [];
	const groups = KEYS.map((key) => {
		const title = h('span', { class: 'menu-title', id: `layout-${key}` }, L[key].title);
		const items = /** @type {string[]} */ (PICKS[key]).map((v) => {
			const b = /** @type {HTMLButtonElement} */ (h('button', { type: 'button', role: 'menuitemradio', class: 'menu-item', 'data-key': key, 'data-pick': v }, /** @type {Record<string, string>} */ (L[key])[v]));
			radios.push(b);
			return b;
		});
		return h('div', { role: 'group', class: 'menu-group', 'aria-labelledby': `layout-${key}` }, title, ...items);
	});
	const again = h('button', { type: 'button', role: 'menuitem', class: 'menu-item' }, L.reset);
	const menu = dropdown(button, 'layout-menu', [...groups, h('div', { role: 'separator', class: 'menu-sep' }), again], true);
	menu.wrap.classList.add('layout-wrap');
	menu.wrap.querySelector('.menu')?.classList.add('layout-menu');
	const show = () => {
		const now = current();
		for (const b of radios) b.setAttribute('aria-checked', String(now[/** @type {keyof typeof now} */ (b.dataset.key)] === b.dataset.pick));
	};
	for (const b of radios) {
		b.addEventListener('click', () => {
			chooseLayout(/** @type {any} */ (b.dataset.key), /** @type {any} */ (b.dataset.pick));
			show();
			b.focus();
		});
	}
	again.addEventListener('click', () => {
		reset();
		show();
		setText(again, L.reset);
		again.focus();
	});
	show();
	return { el: menu.wrap };
}
