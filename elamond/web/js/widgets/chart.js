// Lines over time on a canvas, as the app's LineChart draws them: the newest point at the right
// edge, the window back from it, a long pause breaking the line, a fade under each, the newest
// point a dot; a scale that follows the data to round numbers unless fixed; a dashed grid and its
// top value on a large chart.
import { chartX, chartScale, firstVisibleIndex, CHART_GRID_ROWS } from '../chartmath.js';

const LINE_WIDTH = 2;
const DOT = 3;
const HALO = 6;
const FILL_ALPHA = 0.22;

/**
 * @typedef {{ t: number[], v: (number | null)[], color: string, gapMs: number | number[] }} Line
 * @typedef {{ lines: Line[], windowMs: number, tLast?: number, maxValue?: number | null, minScale: number, binary: boolean,
 *   grid?: boolean, gridColor?: string, label?: (scale: number) => string, labelColor?: string, font?: string, fill?: boolean }} ChartOpts
 */

/** The newest time of any line. @param {Line[]} lines */
export function lastTime(lines) {
	let t = -Infinity;
	for (const l of lines) if (l.t.length) t = Math.max(t, l.t[l.t.length - 1]);
	return t;
}

/**
 * Draws `opts.lines` on `ctx`, `width` by `height` CSS pixels; the scale and right edge it drew with.
 * @param {CanvasRenderingContext2D} ctx @param {number} width @param {number} height @param {ChartOpts} opts
 * @returns {{ scale: number, tLast: number }}
 */
export function drawChart(ctx, width, height, opts) {
	const tLast = opts.tLast ?? lastTime(opts.lines);
	const tStart = tLast - opts.windowMs;
	let top = 0;
	if (Number.isFinite(tLast)) {
		for (const l of opts.lines) {
			for (let i = firstVisibleIndex(l.t, tStart); i < l.t.length; i++) {
				const v = l.v[i];
				if (v != null && v > top) top = v;
			}
		}
	}
	const scale = chartScale(top, opts.maxValue, opts.minScale, opts.binary);
	if (opts.grid && opts.gridColor) {
		ctx.save();
		ctx.strokeStyle = opts.gridColor;
		ctx.lineWidth = 1;
		ctx.setLineDash([4, 4]);
		for (let i = 1; i < CHART_GRID_ROWS; i++) {
			const y = Math.round((height * i) / CHART_GRID_ROWS) + 0.5;
			ctx.beginPath();
			ctx.moveTo(0, y);
			ctx.lineTo(width, y);
			ctx.stroke();
		}
		ctx.restore();
	}
	if (Number.isFinite(tLast)) {
		for (const l of opts.lines) drawLine(ctx, l, scale, tLast, opts.windowMs, width, height, opts.fill !== false);
	}
	if (opts.label && opts.labelColor) {
		ctx.save();
		ctx.font = opts.font || '12px system-ui, sans-serif';
		ctx.fillStyle = opts.labelColor;
		ctx.textBaseline = 'top';
		ctx.fillText(opts.label(scale), 4, 2);
		ctx.restore();
	}
	return { scale, tLast };
}

/** @param {number} v @param {number} scale @param {number} height */
export function yOf(v, scale, height) {
	return height - Math.min(Math.max(v / scale, 0), 1) * (height - DOT) - DOT / 2;
}

/**
 * One line and the fade under it, smoothed through the midpoints between its points.
 * @param {CanvasRenderingContext2D} ctx @param {Line} l @param {number} scale @param {number} tLast @param {number} windowMs @param {number} width @param {number} height @param {boolean} fill
 */
function drawLine(ctx, l, scale, tLast, windowMs, width, height, fill) {
	if (!l.t.length) return;
	/** @type {[number, number][][]} */
	const runs = [];
	/** @type {[number, number][]} */
	let run = [];
	let pt = -Infinity;
	for (let i = firstVisibleIndex(l.t, tLast - windowMs); i < l.t.length; i++) {
		const v = l.v[i];
		const t = l.t[i];
		if (v == null) {
			if (run.length) runs.push(run);
			run = [];
			continue;
		}
		if (run.length && t - pt > (Array.isArray(l.gapMs) ? l.gapMs[i] : l.gapMs)) {
			runs.push(run);
			run = [];
		}
		run.push([chartX(t, tLast, windowMs, width), yOf(v, scale, height)]);
		pt = t;
	}
	if (run.length) runs.push(run);
	ctx.save();
	ctx.lineWidth = LINE_WIDTH;
	ctx.lineJoin = 'round';
	ctx.lineCap = 'round';
	ctx.strokeStyle = l.color;
	ctx.fillStyle = l.color;
	for (const r of runs) {
		const path = new Path2D();
		path.moveTo(r[0][0], r[0][1]);
		for (let i = 1; i < r.length; i++) {
			const [px, py] = r[i - 1];
			const [x, y] = r[i];
			path.quadraticCurveTo(px, py, (px + x) / 2, (py + y) / 2);
		}
		const [lx, ly] = r[r.length - 1];
		path.lineTo(lx, ly);
		if (fill && r.length > 1) {
			const area = new Path2D(path);
			area.lineTo(lx, height);
			area.lineTo(r[0][0], height);
			area.closePath();
			const g = ctx.createLinearGradient(0, 0, 0, height);
			g.addColorStop(0, withAlpha(l.color, FILL_ALPHA));
			g.addColorStop(1, withAlpha(l.color, 0));
			ctx.fillStyle = g;
			ctx.fill(area);
		}
		if (r.length === 1) {
			ctx.beginPath();
			ctx.arc(r[0][0], r[0][1], LINE_WIDTH, 0, Math.PI * 2);
			ctx.fillStyle = l.color;
			ctx.fill();
		} else ctx.stroke(path);
	}
	const last = l.v[l.v.length - 1];
	if (last != null) {
		const x = chartX(l.t[l.t.length - 1], tLast, windowMs, width);
		const y = yOf(last, scale, height);
		ctx.fillStyle = withAlpha(l.color, 0.25);
		ctx.beginPath();
		ctx.arc(x, y, HALO, 0, Math.PI * 2);
		ctx.fill();
		ctx.fillStyle = l.color;
		ctx.beginPath();
		ctx.arc(x, y, DOT, 0, Math.PI * 2);
		ctx.fill();
	}
	ctx.restore();
}

/** A computed colour's red, green and blue, where it is `rgb(…)`. @param {string} color @returns {[number, number, number] | null} */
export function rgbOf(color) {
	const m = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/.exec(color);
	return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** A computed colour with an alpha. @param {string} color @param {number} alpha */
export function withAlpha(color, alpha) {
	const c = rgbOf(color);
	return c ? `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${alpha})` : `color-mix(in srgb, ${color} ${Math.round(alpha * 100)}%, transparent)`;
}

/** `a` turned toward `b` by `t` (0–1), both computed colours. @param {string} a @param {string} b @param {number} t */
export function mix(a, b, t) {
	const x = rgbOf(a);
	const y = rgbOf(b);
	if (!x || !y || !(t > 0)) return a;
	const k = Math.min(t, 1);
	return `rgb(${x.map((v, i) => Math.round(v + (y[i] - v) * k)).join(', ')})`;
}
