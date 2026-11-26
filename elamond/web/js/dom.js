// The page's one way to make elements. What comes from the service goes in as text, never as markup
// (no HTML is ever parsed from a string), and attributes that run or load something are refused.

/** @typedef {Node | string | number | null | undefined | false} Child */

const REFUSED = /^(on|srcdoc$|formaction$)/i;

/**
 * An element with attributes and children. `class`, `text`, `style` (an object of properties) and
 * `on<event>` (a function) are handled; anything else is an attribute. Children that are strings or
 * numbers become text nodes.
 * @template {keyof HTMLElementTagNameMap} K
 * @param {K} tag
 * @param {Record<string, any> | null} [attrs]
 * @param {...Child} children
 * @returns {HTMLElementTagNameMap[K]}
 */
export function h(tag, attrs, ...children) {
	const el = document.createElement(tag);
	if (attrs) {
		for (const [k, v] of Object.entries(attrs)) {
			if (v == null || v === false) continue;
			if (k.startsWith('on') && typeof v === 'function') el.addEventListener(k.slice(2), v);
			else if (k === 'class') el.className = String(v);
			else if (k === 'text') el.textContent = String(v);
			else if (k === 'style') setStyle(el, v);
			else if (k === 'href' || k === 'src') setUrl(el, k, String(v));
			else if (REFUSED.test(k)) throw new Error(`h(): ${k} is not set from data`);
			else el.setAttribute(k, v === true ? '' : String(v));
		}
	}
	append(el, ...children);
	return el;
}

/** `text` with a place to break after each `ch` (a <wbr>, which copying leaves out). @param {string} text @param {string} ch */
export function breakAfter(text, ch) {
	const out = document.createDocumentFragment();
	text.split(ch).forEach((part, i, all) => {
		out.append(i < all.length - 1 ? part + ch : part);
		if (i < all.length - 1) out.append(document.createElement('wbr'));
	});
	return out;
}

/** An SVG element, for icons. */
export function svg(tag, attrs, ...children) {
	const el = document.createElementNS('http://www.w3.org/2000/svg', tag);
	for (const [k, v] of Object.entries(attrs || {})) {
		if (v == null || v === false || REFUSED.test(k) || k === 'href') continue;
		el.setAttribute(k, String(v));
	}
	for (const c of children) if (c) el.append(c);
	return el;
}

/** @param {Element} el @param {...Child} children */
export function append(el, ...children) {
	for (const c of children) {
		if (c == null || c === false) continue;
		el.append(typeof c === 'string' || typeof c === 'number' ? document.createTextNode(String(c)) : c);
	}
}

/** Empties an element. @param {Element} el */
export function clear(el) {
	while (el.firstChild) el.firstChild.remove();
}

/** Sets an element's text, keeping nothing else in it. @param {Element} el @param {unknown} text */
export function setText(el, text) {
	const t = text == null ? '' : String(text);
	if (el.textContent !== t) el.textContent = t;
}

/**
 * Inline styles through the CSSOM (the page's CSP refuses style attributes).
 * @param {HTMLElement | SVGElement} el @param {Record<string, string | number | null>} props
 */
export function setStyle(el, props) {
	for (const [k, v] of Object.entries(props)) {
		if (v == null) el.style.removeProperty(k);
		else el.style.setProperty(k, String(v));
	}
}

/**
 * A URL safe to follow or load: the page's own (a path), or an http(s) one. Any other scheme
 * (javascript:, data: …) is dropped.
 * @param {string} url @returns {string | null}
 */
export function safeUrl(url) {
	try {
		const u = new URL(url, location.href);
		return u.protocol === 'https:' || u.protocol === 'http:' ? u.href : null;
	} catch {
		return null;
	}
}

/** @param {HTMLElement} el @param {string} k @param {string} url */
function setUrl(el, k, url) {
	const safe = safeUrl(url);
	if (safe) el.setAttribute(k, safe);
}
