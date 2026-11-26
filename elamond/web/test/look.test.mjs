// The page's look, read from its files: every colour in style.css's three theme blocks (light,
// dark, black), each whole; none anywhere else in the CSS or the scripts; contrast enough in every
// theme; neighbouring metrics told apart; spaces, corners and type from the scales.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const WEB = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (f) => readFileSync(join(WEB, f), 'utf8');
const CSS = ['style.css', 'panels.css', 'admin.css', 'editor.css'];
const stripComments = (s) => s.replace(/\/\*[\s\S]*?\*\//g, '');

/** The theme blocks of style.css: { light: { bg: '#…', … }, dark: …, black: … }, and the CSS without them. */
function themes() {
	const css = stripComments(read('style.css'));
	const out = {};
	let rest = css;
	for (const [name, re] of [
		['light', /:root,\s*:root\[data-theme="light"\]\s*\{([^}]*)\}/],
		['dark', /:root\[data-theme="dark"\]\s*\{([^}]*)\}/],
		['black', /:root\[data-theme="black"\]\s*\{([^}]*)\}/],
	]) {
		const m = re.exec(css);
		assert.ok(m, `style.css has no ${name} block`);
		out[name] = Object.fromEntries([...m[1].matchAll(/--([\w-]+)\s*:\s*([^;]+);/g)].map((x) => [x[1], x[2].trim()]));
		rest = rest.replace(m[0], '');
	}
	return { themes: out, rest };
}

const BASE = ['bg', 'surface', 'surface-2', 'surface-3', 'line', 'text', 'muted', 'primary', 'on-primary', 'secondary', 'tertiary'];
const STATES = ['good', 'warn', 'hot'];
const ON_STATES = ['on-good', 'on-warn', 'on-hot'];
const METRICS = ['cpu', 'memory', 'swap', 'gpu', 'rx', 'tx', 'read', 'write', 'battery'];
const OTHER = ['shadow', 'shadow-s', 'scrim', 'qr-ink', 'qr-paper'];
const KEYS = [...BASE, ...STATES, ...ON_STATES, ...METRICS, ...OTHER];

// WCAG 2 contrast, and the distance of two colours in OKLab.
const hex = (c) => {
	assert.match(c, /^#[0-9a-f]{6}$/i, `${c} is not a #rrggbb colour`);
	return [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16) / 255);
};
const lin = (v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const lum = (c) => {
	const [r, g, b] = hex(c).map(lin);
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
export const ratio = (a, b) => {
	const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
	return (x + 0.05) / (y + 0.05);
};
const oklab = (c) => {
	const [r, g, b] = hex(c).map(lin);
	const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
	const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
	const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
	return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
};
const distance = (a, b) => Math.hypot(...oklab(a).map((v, i) => v - oklab(b)[i]));

test('each theme block is whole: the same keys, every one of the page’s colours', () => {
	const { themes: t } = themes();
	for (const [name, block] of Object.entries(t)) {
		assert.deepEqual(Object.keys(block).sort(), [...KEYS].sort(), `the ${name} block's keys`);
	}
});

test('no colour outside the theme blocks: in the CSS', () => {
	const { rest } = themes();
	const all = [rest, ...CSS.slice(1).map((f) => stripComments(read(f)))].join('\n');
	const found = [...all.matchAll(/#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(\s*[\d.]/gi)].map((m) => m[0]);
	assert.deepEqual(found, [], 'colours written out of the theme blocks');
	const named = [...all.matchAll(/(?:color|background|border[\w-]*|outline|fill|stroke|box-shadow|accent-color)\s*:[^;{}]*\b(white|black|red|green|blue|gray|grey|orange|yellow|purple|silver)\b/gi)].map((m) => m[0]);
	assert.deepEqual(named, [], 'named colours');
});

function scripts(dir = join(WEB, 'js')) {
	return readdirSync(dir).flatMap((f) => {
		const p = join(dir, f);
		return statSync(p).isDirectory() ? scripts(p) : f.endsWith('.js') ? [p] : [];
	});
}

test('no colour outside the theme blocks: in the scripts (canvases, QR codes, SVG read them from CSS)', () => {
	const found = [];
	for (const p of scripts()) {
		const code = readFileSync(p, 'utf8').replace(/\/\/[^\n]*|\/\*[\s\S]*?\*\//g, '');
		for (const m of code.matchAll(/['"`][^'"`\n]*?(#[0-9a-f]{3}(?:[0-9a-f]{3})?(?:[0-9a-f]{2})?\b|\b(?:rgba?|hsla?)\(\s*\d)[^'"`\n]*['"`]/gi)) found.push(`${p.slice(WEB.length)}: ${m[1]}`);
	}
	assert.deepEqual(found, []);
});

test('outside the page: the manifest, the icon and theme-color in the dark theme’s colours', () => {
	const { themes: t } = themes();
	const dark = new Set(Object.values(t.dark).map((v) => v.toLowerCase()));
	const manifest = JSON.parse(read('manifest.webmanifest'));
	assert.equal(manifest.background_color, t.dark.bg);
	assert.equal(manifest.theme_color, t.dark.bg);
	for (const c of read('icon.svg').match(/#[0-9a-f]{6}/gi) || []) assert.ok(dark.has(c.toLowerCase()), `icon.svg: ${c} is no colour of the dark theme`);
	assert.match(read('index.html'), new RegExp(`<meta name="theme-color" content="${t.dark.bg}">`));
});

test('contrast in every theme: words 4.5 : 1, graphics 3 : 1', () => {
	const { themes: t } = themes();
	const fails = [];
	const need = (name, th, fg, bg, min) => {
		const r = ratio(th[fg], th[bg]);
		if (r < min) fails.push(`${name}: ${fg} on ${bg} ${r.toFixed(2)} < ${min}`);
	};
	for (const [name, th] of Object.entries(t)) {
		// Words: the text and the quiet text on every surface; the accent; words on the accent and on a state.
		for (const fg of ['text', 'muted', 'secondary']) for (const bg of ['bg', 'surface', 'surface-2']) need(name, th, fg, bg, 4.5);
		for (const bg of ['bg', 'surface']) need(name, th, 'primary', bg, 4.5);
		need(name, th, 'on-primary', 'primary', 4.5);
		for (const s of STATES) need(name, th, `on-${s}`, s, 4.5);
		// The states are words too (a mistake under a field, a note in a log): 4.5 on the page and its cards.
		for (const s of STATES) for (const bg of ['bg', 'surface', 'surface-2']) need(name, th, s, bg, 4.5);
		// Graphics: each metric and the tertiary accent on a card and on a tile.
		for (const m of [...METRICS, 'tertiary']) for (const bg of ['surface', 'surface-2']) need(name, th, m, bg, 3);
	}
	assert.deepEqual(fails, []);
});

test('neighbouring metrics and the states are told apart', () => {
	const { themes: t } = themes();
	const fails = [];
	const PAIRS = [['cpu', 'memory'], ['memory', 'swap'], ['cpu', 'swap'], ['rx', 'tx'], ['read', 'write'], ['good', 'warn'], ['warn', 'hot'], ['good', 'hot']];
	for (const [name, th] of Object.entries(t)) {
		for (const [a, b] of PAIRS) {
			const d = distance(th[a], th[b]);
			if (d < 0.09) fails.push(`${name}: ${a} and ${b} ${d.toFixed(3)} apart (OKLab), under 0.09`);
		}
		// Neutral greys: the base has no hue.
		for (const k of ['bg', 'surface', 'surface-2', 'surface-3', 'line', 'text', 'muted']) {
			const [, a, b] = oklab(th[k]);
			if (Math.hypot(a, b) > 0.01) fails.push(`${name}: ${k} ${th[k]} has a hue`);
		}
	}
	assert.deepEqual(fails, []);
});

test('spaces and corners from the scales; type from its steps', () => {
	const fails = [];
	for (const f of CSS) {
		const css = stripComments(read(f)).replace(/:root\s*\{[^}]*\}/, '');
		for (const m of css.matchAll(/(?:^|[;{\s])((?:padding|margin|gap|row-gap|column-gap)(?:-[a-z-]+)?|border(?:-[a-z]+)*-radius)\s*:\s*([^;}]+)/g)) {
			for (const px of m[2].matchAll(/(?<![\w.-])(\d+(?:\.\d+)?)px\b/g)) {
				const v = Number(px[1]);
				// A line's width (1 px) and the room a control takes (32 px and over) aren't spaces.
				if (v > 1 && v < 32) fails.push(`${f}: ${m[1]}: ${m[2].trim()}`);
			}
		}
		for (const m of css.matchAll(/font-size\s*:\s*([^;}]+)/g)) {
			if (!/^var\(--(?:t[1-6]|code-size)\)$|^inherit$/.test(m[1].trim())) fails.push(`${f}: font-size: ${m[1].trim()}`);
		}
	}
	assert.deepEqual(fails, []);
});
