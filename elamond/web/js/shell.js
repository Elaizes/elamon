// The frame of the computer's pages (its dashboards, the admin panel, the dashboard editor). On a
// wide window, a side bar: the computer's name, its dashboards, the admin panel's sections, the
// editor, and at its foot the theme and the layout; it folds to a strip of icons, as this browser
// keeps (arrange.js). On a narrow one, as 0.11.3 had it: the name and the tools in a bar at the top,
// the dashboards in a row of tabs under it, the admin panel's sections in a row of their own. Over
// the page, a bar with the place's title, the computer's chips and the connection's state, which
// stays in place while the page scrolls (on a wide window; on a phone it would take a third of the
// screen). The parts are made once and moved between the side bar and the top bar as the window
// crosses SIDE_PX. Keys: 1–9 the dashboards, Escape back to them from the admin panel or the editor.
import { h } from './dom.js';
import { S, fill } from './strings.js';
import { icon } from './icons.js';
import * as state from './state.js';
import { api, ApiError } from './api.js';
import { dropdown, themeMenu, layoutMenu } from './menus.js';
import { folded, fold } from './arrange.js';
import { roleName } from './roles.js';
import { confirmDialog } from './actions.js';
import { errorText } from './errors.js';
import { SIDE_PX, NARROW_PX, ONE_COLUMN_PX } from './consts.js';

const P = S.panels;

/** The admin panel's sections, in its order, and the icon of each in the side bar. @type {[keyof typeof S.admin.nav, string][]} */
export const ADMIN_SECTIONS = [
	['overview', 'overview'],
	['settings', 'control'],
	['clients', 'laptop'],
	['pairing', 'lock'],
	['control', 'power'],
	['files', 'storage'],
	['activity', 'clock'],
	['events', 'warning'],
	['sources', 'network'],
];

/**
 * Where the frame is: on a dashboard (`tab`, picking one through `pick` rather than the address),
 * in the admin panel (`section`) or in the editor.
 * @typedef {{ at: 'tab' | 'admin' | 'edit', section?: string, pick?: (id: string) => void, forgotten?: () => void }} Where
 *
 * What a page gets of the frame: its title's place, the chips' and the state's places in the bar,
 * where its content goes, and the dashboard on screen to mark.
 * @typedef {{ title: (text: string) => void, chips: HTMLElement, status: HTMLElement, body: HTMLElement,
 *   select: (id: string | null) => void, destroy: () => void }} Frame
 */

/** The dashboards of `ui` that are tabs, once each, in order. @param {import('./types.js').Ui} ui */
export function tabsOf(ui) {
	return (ui.tabs || []).filter((t, i, all) => all.indexOf(t) === i && (ui.dashboards || []).some((d) => d.id === t));
}

/** Whether a key goes to what has the focus (a field, a list, a slider), not to the page. @param {EventTarget | null} t */
function typing(t) {
	return t instanceof HTMLElement && (t.isContentEditable || t.matches('input, textarea, select, [role="listbox"], [role="combobox"], [role="slider"], [role="option"], [role="textbox"], [role="spinbutton"]'));
}

/** The room the page has beside the side bar, as the editor's sizes go by it (editor.css): under 900, 600 and 320 px. @param {number} px */
export function roomOf(px) {
	return [px <= 900 ? 'sheet' : '', px <= NARROW_PX ? 'two' : '', px <= ONE_COLUMN_PX ? 'one' : ''].filter(Boolean).join(' ');
}

/**
 * Draws the frame in `root` for `where`. @param {HTMLElement} root @param {import('./types.js').Ctx} ctx @param {Where} where
 * @returns {Frame}
 */
export function mountFrame(root, ctx, where) {
	const ui = /** @type {import('./types.js').Ui} */ (ctx.ui());
	const tabs = tabsOf(ui);
	const name = (ctx.hello && ctx.hello.name) || (ui.service && ui.service.name) || S.core.title;
	const wideQuery = matchMedia(`(min-width: ${SIDE_PX}px)`);
	/** The dashboard marked: the one on screen, or none (the admin panel, the editor). */
	let selected = /** @type {string | null} */ (null);
	/** The dashboard Edit opens and Escape goes back to. */
	let last = /** @type {string | null} */ (tabs.includes(state.load('tab', null)) ? state.load('tab', null) : tabs[0] || null);

	// ---- the parts

	const foldButton = h('button', { type: 'button', class: 'icon-button quiet side-fold' }, icon('menu'));
	const sideName = h('span', { class: 'side-name' }, name);

	/** @type {HTMLAnchorElement[]} */
	const links = [];
	const tablist = h('div', { class: 'tabs', role: 'tablist', 'aria-label': P.tabs });
	tabs.forEach((id, i) => {
		const d = (ui.dashboards || []).find((x) => x.id === id);
		const title = (d && d.title) || id;
		const a = /** @type {HTMLAnchorElement} */ (h('a', { href: `#/tab/${encodeURIComponent(id)}`, role: 'tab', class: 'tab', 'aria-selected': 'false', 'aria-controls': 'page-body', tabindex: '-1', title, 'data-tab': id },
			icon(d && d.icon, 'w-icon'), h('span', { class: 'nav-label', 'data-label': title }, title)));
		a.addEventListener('click', (e) => {
			if (!where.pick || e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
			e.preventDefault();
			where.pick(id);
		});
		a.addEventListener('keydown', (e) => {
			const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
			const to = e.key === 'Home' ? 0 : e.key === 'End' ? links.length - 1 : step ? (i + step + links.length) % links.length : -1;
			if (to < 0) return;
			e.preventDefault();
			links[to].focus();
			links[to].click();
		});
		links.push(a);
		tablist.append(a);
	});

	const admin = ctx.has('admin');
	const adminNav = admin
		? h('nav', { class: 'adm-nav', 'aria-label': S.admin.nav.label },
				...ADMIN_SECTIONS.map(([id, ic]) => h('a', { href: `#/admin/${id}`, class: 'nav-item', title: S.admin.nav[id], 'aria-current': where.at === 'admin' && where.section === id ? 'page' : null },
					icon(ic, 'w-icon'), h('span', { class: 'nav-label', 'data-label': S.admin.nav[id] }, S.admin.nav[id]))))
		: null;
	const adminGroup = adminNav ? h('div', { class: 'side-group' }, h('h2', { class: 'side-title' }, S.core.admin), adminNav) : null;
	// The admin panel from the top bar, on a narrow window.
	const adminLink = admin
		? h('a', { class: 'nav-item', href: '#/admin', title: S.core.admin, 'aria-current': where.at === 'admin' ? 'page' : null }, icon('control', 'w-icon'), h('span', { class: 'nav-label', 'data-label': S.core.admin }, S.core.admin))
		: null;
	const editLink = ctx.has('editor')
		? /** @type {HTMLAnchorElement} */ (h('a', { class: 'nav-item edit', 'aria-label': P.editThis, title: P.editThis, 'aria-current': where.at === 'edit' ? 'page' : null }, icon('edit', 'w-icon'), h('span', { class: 'nav-label', 'data-label': P.edit }, P.edit)))
		: null;

	const theme = themeMenu();
	const layout = layoutMenu();
	const role = h('span', { class: 'pill role', title: fill(P.roleAbout, roleName(ui.role || '')), 'aria-label': fill(P.roleAbout, roleName(ui.role || '')) }, roleName(ui.role || ''));
	const more = moreMenu(ctx, where);

	const sideScroll = h('div', { class: 'side-scroll' });
	const sideFoot = h('div', { class: 'side-foot' });
	const side = h('nav', { class: 'side', 'aria-label': fill(P.sideLabel, name) }, h('div', { class: 'side-head' }, foldButton, sideName), sideScroll, sideFoot);
	const title = h('h1', { class: 'name' });
	const chips = h('div', { class: 'head-chips' });
	const tools = h('div', { class: 'top-tools' });
	const status = h('div', { class: 'head-status' });
	const head = h('header', { class: 'topbar page-top' }, h('div', { class: 'machine-title' }, title, chips), tools, status);
	const rows = h('div', { class: 'page-rows' });
	const body = h('div', { class: 'page-body', id: 'page-body' });
	const page = h('div', { class: 'page' }, head, rows, body);
	root.append(h('div', { class: 'shell' }, side, page));

	// ---- placing them for the window

	let placeTitle = '';
	let wide = false;
	const showTitle = () => {
		title.textContent = wide ? placeTitle || name : name;
	};
	const place = () => {
		wide = wideQuery.matches;
		if (wide) {
			sideScroll.replaceChildren(...[tablist, adminGroup, editLink].filter((x) => x !== null));
			if (adminGroup && adminNav) adminGroup.append(adminNav);
			sideFoot.replaceChildren(theme.el, layout.el);
			tools.replaceChildren(role, more);
			rows.replaceChildren();
		} else {
			sideScroll.replaceChildren();
			sideFoot.replaceChildren();
			tools.replaceChildren(...[role, theme.el, layout.el, where.at === 'tab' ? editLink : null, adminLink, more].filter((x) => x !== null));
			// The tabs: a way back from the admin panel and the editor; on the dashboards, when there is a choice.
			const tabRow = tabs.length > 1 || (tabs.length && where.at !== 'tab') ? tablist : null;
			rows.replaceChildren(...[tabRow, where.at === 'admin' ? adminNav : null].filter((x) => x !== null));
			if (where.at === 'admin') adminNav?.querySelector('[aria-current="page"]')?.scrollIntoView({ block: 'nearest', inline: 'center' });
		}
		tablist.setAttribute('aria-orientation', wide ? 'vertical' : 'horizontal');
		showTitle();
		measure();
	};

	// The top bar's height, for what sticks under it (editor.css); the room beside the side bar.
	const html = document.documentElement;
	const measure = () => {
		const stuck = getComputedStyle(head).position === 'sticky';
		html.style.setProperty('--head-h', `${stuck ? Math.ceil(head.getBoundingClientRect().height) : 0}px`);
		const sideW = wide ? side.getBoundingClientRect().width : 0;
		html.dataset.room = roomOf(window.innerWidth - sideW);
	};
	const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(measure) : null;
	ro?.observe(head);
	ro?.observe(side);
	window.addEventListener('resize', measure);
	wideQuery.addEventListener('change', place);

	const showFold = () => {
		const rail = folded();
		const text = rail ? P.sideUnfold : P.sideFold;
		foldButton.setAttribute('aria-label', text);
		foldButton.title = text;
		foldButton.setAttribute('aria-expanded', String(!rail));
	};
	foldButton.addEventListener('click', () => {
		fold(!folded());
		showFold();
	});
	showFold();

	// ---- keys

	const openOverlay = () => !!document.querySelector('dialog[open], .menu:not([hidden])');
	/** @param {KeyboardEvent} e */
	const keys = (e) => {
		// After the page's own listeners (on the document): what one of them took (a menu's Escape, the editor's) isn't the frame's.
		if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey || typing(e.target) || openOverlay()) return;
		if (/^[1-9]$/.test(e.key)) {
			const id = tabs[Number(e.key) - 1];
			if (!id) return;
			e.preventDefault();
			if (where.pick) where.pick(id);
			else ctx.go(`#/tab/${encodeURIComponent(id)}`);
		} else if (e.key === 'Escape' && where.at !== 'tab') {
			e.preventDefault();
			ctx.go(last ? `#/tab/${encodeURIComponent(last)}` : '#/');
		}
	};
	window.addEventListener('keydown', keys);

	/** @param {string | null} id */
	const select = (id) => {
		selected = id;
		if (id && tabs.includes(id)) last = id;
		links.forEach((a, i) => {
			const on = a.dataset.tab === selected;
			a.setAttribute('aria-selected', String(on));
			// One tab in the list takes the focus: the one marked, else the first.
			a.tabIndex = on || (!links.some((x) => x.dataset.tab === selected) && i === 0) ? 0 : -1;
		});
		if (editLink) {
			const to = where.at === 'tab' ? id : last;
			editLink.hidden = !to;
			if (to) editLink.href = `#/edit/${encodeURIComponent(to)}`;
		}
	};
	select(null);
	place();

	return {
		title(text) {
			placeTitle = text;
			showTitle();
		},
		chips,
		status,
		body,
		select,
		destroy() {
			ro?.disconnect();
			window.removeEventListener('resize', measure);
			window.removeEventListener('keydown', keys);
			wideQuery.removeEventListener('change', place);
			theme.off();
		},
	};
}

/** The menu with what is done rarely: a new dashboard, forgetting this browser. @param {import('./types.js').Ctx} ctx @param {Where} where */
function moreMenu(ctx, where) {
	const button = h('button', { type: 'button', class: 'icon-button quiet', 'aria-label': P.menu, title: P.menu }, icon('more'));
	const forget = h('button', { type: 'button', role: 'menuitem', class: 'menu-item danger-text' }, P.forget);
	// A new dashboard, for the role the editor is open to.
	const add = ctx.has('editor') ? h('button', { type: 'button', role: 'menuitem', class: 'menu-item', onclick: () => (menu.close(), ctx.go('#/edit')) }, P.newDashboard) : null;
	const menu = dropdown(button, 'machine-menu', add ? [add, forget] : [forget]);
	forget.addEventListener('click', async () => {
		menu.close();
		const yes = await confirmDialog(P.forgetQuestion, P.forgetBody, P.forget, true, null);
		if (!yes) return;
		try {
			await api('/api/v1/unpair', { method: 'POST' });
		} catch (e) {
			if (!(e instanceof ApiError && e.status === 401)) {
				ctx.toast(fill(P.forgetFailed, errorText(e)), 'critical');
				return;
			}
		}
		where.forgotten?.();
		try {
			await ctx.reloadUi();
		} catch {}
		ctx.go('#/');
	});
	return menu.wrap;
}

