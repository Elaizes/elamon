// What the canvases share (graphs, sparks, timelines): sharp at the screen's pixel ratio, drawn
// again when their size or the theme changes, in the theme's colours read from CSS variables.
import { h } from '../dom.js';
import { mix } from './chart.js';

/** @type {Set<() => void>} */
const themeListeners = new Set();
let watching = false;

/** Hears the theme change: the page's choice (html's data-theme) or the system's. @param {() => void} cb @returns {() => void} */
export function onTheme(cb) {
	themeListeners.add(cb);
	if (!watching && typeof MutationObserver !== 'undefined') {
		watching = true;
		const all = () => {
			for (const f of themeListeners) f();
		};
		new MutationObserver(all).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'data-black'] });
		window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', all);
	}
	return () => themeListeners.delete(cb);
}

/** A probe whose computed colour turns any CSS colour (a variable, a mix) into one a canvas takes. */
let probe = /** @type {HTMLElement | null} */ (null);

/** `css` (a colour, `var(--cpu)`, a `color-mix`) as `host` would show it. @param {Element} host @param {string} css */
export function cssColor(host, css) {
	if (!probe) {
		probe = h('span', { class: 'color-probe', 'aria-hidden': 'true' });
	}
	const parent = host.isConnected ? host : document.body;
	parent.append(probe);
	probe.style.setProperty('color', css);
	const c = getComputedStyle(probe).color;
	probe.remove();
	return c;
}

/**
 * A canvas that keeps itself sharp: `draw(ctx, width, height)` in CSS pixels, called again on a
 * resize or a theme change, and when `redraw()` asks (once a frame at most).
 * @param {string} cls @param {(ctx: CanvasRenderingContext2D, width: number, height: number) => void} draw
 * @returns {{ el: HTMLCanvasElement, redraw: () => void, destroy: () => void }}
 */
export function sharpCanvas(cls, draw) {
	const el = h('canvas', { class: cls });
	let frame = 0;
	const paint = () => {
		frame = 0;
		const width = el.clientWidth;
		const height = el.clientHeight;
		if (!width || !height) return;
		const dpr = window.devicePixelRatio || 1;
		const w = Math.round(width * dpr);
		const hh = Math.round(height * dpr);
		if (el.width !== w || el.height !== hh) {
			el.width = w;
			el.height = hh;
		}
		const ctx = el.getContext('2d');
		if (!ctx) return;
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, width, height);
		draw(ctx, width, height);
	};
	const redraw = () => {
		if (!frame) frame = requestAnimationFrame(paint);
	};
	const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(redraw) : null;
	ro?.observe(el);
	const off = onTheme(redraw);
	return {
		el,
		redraw,
		destroy() {
			ro?.disconnect();
			off();
			if (frame) cancelAnimationFrame(frame);
		},
	};
}

/**
 * The service's level as a colour a canvas takes: `good` (CSS) while normal, turning amber by the
 * blend, then red; null without a level.
 * @param {Element} host @param {string | null | undefined} level @param {number | null | undefined} blend @param {string} good
 */
export function levelColor(host, level, blend, good) {
	const b = typeof blend === 'number' && blend > 0 ? blend : 0;
	if (level === 'normal') return b > 0 ? mixCss(host, good, 'var(--warn)', b) : cssColor(host, good);
	if (level === 'warning') return b > 0 ? mixCss(host, 'var(--warn)', 'var(--hot)', b) : cssColor(host, 'var(--warn)');
	if (level === 'critical') return cssColor(host, 'var(--hot)');
	return null;
}

/** @param {Element} host @param {string} a @param {string} b @param {number} t */
function mixCss(host, a, b, t) {
	return mix(cssColor(host, a), cssColor(host, b), t);
}
