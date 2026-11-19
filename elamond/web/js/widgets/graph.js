// `graph`: lines over time with the newest values. Small, a tile without a grid (the overview's
// network); medium or large, a card with a grid, the long ranges `/api/v1/history` gives beside its
// own, a time axis on those, and a finger or pointer reading the values at a moment.
import { h, setText, setStyle } from '../dom.js';
import { api } from '../api.js';
import { card, captionLine, accent, chips, captionOf } from './common.js';
import { sharpCanvas, cssColor } from './canvas.js';
import { drawChart } from './chart.js';
import { Hist, keptHist, rangeChips, spanParts } from '../history.js';
import { timeTicks, axisLabels, nearestIndex, timeAtX, chartX, tooltipLeft } from '../chartmath.js';
import { errorText } from '../errors.js';
import { S, fill } from '../strings.js';
import * as F from '../format.js';

const AXIS_SPACING = 72;
const AXIS_GAP = 8;

/** What a range chip says: "10 min", "24 h", "7 days", "Year", "All". @param {import('../history.js').RangeChip} chip */
export function rangeLabel(chip) {
	if (chip.range && chip.range.key === '1y') return S.panels.rangeYear;
	if (chip.range && chip.range.key === 'all') return S.panels.rangeAll;
	return spanLabel(chip.ms);
}

/** A span as a chip says it. @param {number} ms */
export function spanLabel(ms) {
	const [unit, n] = spanParts(ms);
	if (unit === 's') return fill(S.panels.rangeSeconds, n);
	if (unit === 'min') return fill(S.panels.rangeMinutes, n);
	if (unit === 'h') return fill(S.panels.rangeHours, n);
	return fill(n === 1 ? S.panels.rangeDay : S.panels.rangeDays, n);
}

/** How long ago, for a live chart's pointer. @param {number} seconds */
function ago(seconds) {
	if (seconds < 2) return S.panels.chartNow;
	return fill(S.panels.chartAgo, seconds < 60 ? fill(S.panels.rangeSeconds, seconds) : F.duration(seconds));
}

/** A time axis's label. @param {'time' | 'day' | 'month' | 'year'} unit @param {number} ms */
export function tickText(unit, ms) {
	const d = new Date(ms);
	if (unit === 'time') return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	if (unit === 'day') return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
	if (unit === 'month') return d.toLocaleDateString(undefined, { month: 'short' });
	return d.toLocaleDateString(undefined, { year: 'numeric' });
}

/**
 * The moments from `start` to `end` under a chart as wide, at round times of the browser's clock.
 * @param {CanvasRenderingContext2D} ctx @param {number} width @param {number} start @param {number} end @param {string} color
 */
export function drawAxis(ctx, width, start, end, color) {
	const ticks = timeTicks(start, end, Math.floor(width / AXIS_SPACING));
	ctx.font = '11px system-ui, sans-serif';
	ctx.fillStyle = color;
	ctx.textBaseline = 'top';
	const texts = ticks.times.map((t) => tickText(ticks.unit, t));
	const xs = ticks.times.map((t) => ((t - start) / (end - start)) * width);
	const widths = texts.map((t) => ctx.measureText(t).width);
	for (const [i, left] of axisLabels(xs, widths, width, AXIS_GAP)) ctx.fillText(texts[i], left, 2);
}

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const series = Array.isArray(w.series) && w.series.length ? w.series : [{}];
	const first = series[0];
	const small = w.height === 'small';
	const liveMs = (typeof w.range === 'number' && w.range > 0 ? w.range : env.historyMs / 1000) * 1000;
	const live = keptHist(env.memory, w.id, series.length, liveMs, env.intervalMs);
	const binary = first.format === 'bytes' || first.format === 'rate';
	const floor = typeof w.floor === 'number' ? w.floor : 1;
	const colorOf = (/** @type {any} */ ser) => accent(ser.color || w.color);
	const rangeKey = `${w.id}#range`;
	const chipsOf = small ? [] : rangeChips(liveMs, w.ranges);
	/** @type {{ key: string, chip: import('../history.js').RangeChip, hist: Hist | null, step: number, loading: boolean, failed: boolean } | null} */
	let long = null;
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	let fetches = 0;
	/** Shown again after a drop or a hidden page: a long range is asked for anew with the next line. */
	let stale = false;

	const now = h('span', { class: 'graph-now' });
	const { el, body } = card(w, env, { tile: small, trailing: !small && series.length === 1 ? now : undefined });
	const empty = h('p', { class: 'graph-empty muted' });
	empty.hidden = true;
	const note = h('p', { class: 'graph-note muted' });
	note.hidden = true;
	const cap = captionLine();

	// The figures of a small graph, each series' over its line.
	const figures = series.map((ser) => {
		const value = h('span', { class: 'fig-num' });
		const unit = h('span', { class: 'fig-unit' });
		const label = h('span', { class: 'graph-series-label' }, ser.label || '');
		setStyle(label, { color: colorOf(ser) });
		return { el: h('div', { class: 'graph-figure' }, series.length > 1 || ser.label ? label : null, h('div', { class: 'fig' }, value, unit)), value, unit };
	});
	const legend = series.map((ser) => {
		const text = h('span', { class: 'legend-text' });
		const dot = h('span', { class: 'legend-dot', 'aria-hidden': 'true' });
		setStyle(dot, { background: colorOf(ser) });
		return { el: h('span', { class: 'legend' }, dot, text), text };
	});

	const hist = () => (long && long.hist ? long.hist : live);
	const chart = sharpCanvas(`chart${small ? ' small' : w.height === 'large' ? ' large' : ''}`, (ctx, width, height) => {
		const hh = hist();
		const lines = series.map((ser, k) => ({ t: hh.t, v: hh.v[k], color: cssColor(el, colorOf(ser)), gapMs: long && long.hist ? long.step * 3 : hh.gapMs }));
		const max = last && typeof last.max === 'number' ? last.max : typeof w.max === 'number' ? w.max : null;
		const muted = cssColor(el, 'var(--muted)');
		const drawn = drawChart(ctx, width, height, {
			lines,
			windowMs: hh.windowMs,
			tLast: long && long.hist ? Math.max(hh.lastTime, Date.now()) : undefined,
			maxValue: max,
			minScale: floor,
			binary,
			grid: !small,
			gridColor: cssColor(el, 'var(--line)'),
			label: small ? undefined : (v) => F.number(v, first.format, first.unit),
			labelColor: muted,
		});
		scale = drawn.scale;
		tLast = drawn.tLast;
		if (pointerX != null) drawPointer(ctx, width, height);
		if (axis) axis.redraw();
	});
	chart.el.setAttribute('role', 'img');
	let scale = 1;
	let tLast = 0;
	/** @type {number | null} */
	let pointerX = null;
	const tip = h('div', { class: 'chart-tip', role: 'presentation' });
	tip.hidden = true;
	const wrap = h('div', { class: 'chart-wrap' }, chart.el, tip);
	/** @type {ReturnType<typeof sharpCanvas> | null} */
	let axis = null;
	if (!small) {
		axis = sharpCanvas('chart-axis', (ctx, width) => {
			const hh = hist();
			if (!long || !long.hist || !hh.size) return;
			drawAxis(ctx, width, tLast - hh.windowMs, tLast, cssColor(el, 'var(--muted)'));
		});
		axis.el.setAttribute('aria-hidden', 'true');
	}

	if (!small) {
		wrap.addEventListener('pointermove', (e) => {
			pointerX = e.offsetX;
			chart.redraw();
		});
		wrap.addEventListener('pointerleave', () => {
			pointerX = null;
			tip.hidden = true;
			chart.redraw();
		});
	}

	/** @param {CanvasRenderingContext2D} ctx @param {number} width @param {number} height */
	function drawPointer(ctx, width, height) {
		const hh = hist();
		const x0 = /** @type {number} */ (pointerX);
		const i = nearestIndex(hh.t, timeAtX(x0, tLast, hh.windowMs, width));
		if (i < 0) return;
		const t = hh.t[i];
		const x = Math.min(Math.max(chartX(t, tLast, hh.windowMs, width), 0), width);
		ctx.save();
		ctx.strokeStyle = cssColor(el, 'var(--muted)');
		ctx.lineWidth = 1;
		ctx.beginPath();
		ctx.moveTo(x + 0.5, 0);
		ctx.lineTo(x + 0.5, height);
		ctx.stroke();
		ctx.restore();
		const parts = [long && long.hist ? F.time(t, true) : ago(Math.max(0, Math.round((tLast - t) / 1000)))];
		series.forEach((ser, k) => {
			const v = hh.v[k][i];
			if (v != null) parts.push([series.length > 1 ? ser.label : null, F.number(v, ser.format, ser.unit)].filter(Boolean).join(' '));
		});
		setText(tip, parts.join('  ·  '));
		tip.hidden = false;
		setStyle(tip, { left: `${tooltipLeft(x, tip.offsetWidth, width)}px` });
	}

	/** @type {ReturnType<typeof chips> | null} */
	let rangePicker = null;
	if (small) {
		body.append(empty, h('div', { class: 'graph-figures' }, ...figures.map((f) => f.el)), wrap, cap.el);
	} else {
		if (chipsOf.length) {
			const picker = chips(S.panels.rangeChips, chipsOf.map((c) => ({ value: c.range ? c.range.key : '', text: rangeLabel(c) })), '', (key) => pick(key));
			if (!env.live) picker.el.setAttribute('aria-disabled', 'true');
			body.append(picker.el);
			const kept = env.memory.get(rangeKey);
			if (kept && chipsOf.some((c) => c.range && c.range.key === kept) && env.live) {
				picker.select(kept);
				queueMicrotask(() => pick(kept));
			}
			rangePicker = picker;
		}
		body.append(empty, note, wrap);
		if (axis) body.append(axis.el);
		if (series.length > 1) body.append(h('div', { class: 'legends' }, ...legend.map((l) => l.el)));
		body.append(cap.el);
	}

	/** Shows the live range (key '') or a long one, asking the computer for it. @param {string} key */
	function pick(key) {
		if (!env.live) {
			rangePicker?.select(long ? long.key : '');
			return;
		}
		env.memory.set(rangeKey, key || null);
		if (!key) {
			long = null;
			showNote();
			chart.redraw();
			return;
		}
		const chip = chipsOf.find((c) => c.range && c.range.key === key);
		if (!chip) return;
		long = { key, chip, hist: long && long.key === key ? long.hist : null, step: long && long.key === key ? long.step : 0, loading: true, failed: false };
		showNote();
		fetchLong();
	}

	async function fetchLong() {
		if (!long) return;
		const asked = long;
		const n = ++fetches;
		try {
			const r = await api(`/api/v1/history?widget=${encodeURIComponent(w.id)}&range=${encodeURIComponent(asked.key)}`);
			if (n !== fetches || long !== asked) return;
			const span = asked.chip.range && asked.chip.range.ms != null ? asked.chip.range.ms : Math.max(r.to - r.from, 60_000);
			const hh = new Hist(series.length, span, r.step_ms || env.intervalMs);
			hh.replace({ t: r.t || [], v: r.v || [] });
			asked.hist = hh;
			asked.step = r.step_ms || 0;
			asked.loading = false;
		} catch (e) {
			if (n !== fetches || long !== asked) return;
			asked.loading = false;
			asked.failed = !asked.hist;
			if (asked.failed) note.title = errorText(e);
		}
		showNote();
		chart.redraw();
	}

	function showNote() {
		const text = long && long.failed ? fill(S.panels.rangeFailed, rangeLabel(long.chip)) : long && long.loading && !long.hist ? S.panels.loading : null;
		note.hidden = !text;
		setText(note, text || '');
		if (long && long.failed) {
			long = null;
			rangePicker?.select('');
		}
	}

	function describe() {
		const hh = hist();
		const span = long ? rangeLabel(long.chip) : spanLabel(liveMs);
		const parts = series.map((ser, k) => {
			const vs = hh.v[k].filter((v) => v != null);
			const name = ser.label || w.label || '';
			if (!vs.length) return fill(S.panels.chartNoData, name);
			const newest = /** @type {number} */ (vs[vs.length - 1]);
			return fill(S.panels.chartSeries, name, F.number(newest, ser.format, ser.unit), F.number(Math.max(.../** @type {number[]} */ (vs)), ser.format, ser.unit));
		});
		chart.el.setAttribute('aria-label', fill(S.panels.chartSummary, span, parts.join('; ')));
	}

	/** @param {import('./common.js').WidgetState | undefined} s @param {import('./common.js').Data | null} data */
	function show(s, data) {
		last = s;
		const values = s && Array.isArray(s.v) ? s.v.map((x) => (typeof x === 'number' ? x : null)) : [];
		if (data) {
			const entry = data.history ? data.history[w.id] : undefined;
			if (entry) live.take(data.t, values, entry);
			else if (s) live.append(data.t, values);
			if (long && long.hist && s) long.hist.appendEvery(data.t, values, long.step);
			// A new stream or dashboard: the long range is asked again, as it may have moved on.
			if ((entry || stale) && long && env.live) fetchLong();
			stale = false;
		}
		const text = s && typeof s.text === 'string' ? s.text : null;
		empty.hidden = !text;
		setText(empty, text || '');
		figures.forEach((f, k) => {
			const v = values[k];
			if (v == null) {
				setText(f.value, F.DASH);
				setText(f.unit, '');
			} else {
				const [a, b] = F.parts(v, series[k].format, series[k].unit);
				setText(f.value, a);
				setText(f.unit, b ? (b === '%' || b === '°C' || b === '°F' ? b : ` ${b}`) : '');
			}
		});
		legend.forEach((l, k) => {
			const v = values[k];
			setText(l.text, [series[k].label, v == null ? null : F.number(v, series[k].format, series[k].unit)].filter(Boolean).join(' '));
		});
		if (series.length === 1) {
			setText(now, values[0] == null ? '' : F.number(values[0], first.format, first.unit));
			setStyle(now, { color: colorOf(first) });
		}
		cap.set(captionOf(w, s));
		describe();
		chart.redraw();
	}
	show(undefined, null);

	return {
		el,
		update: (s, data) => show(s, data),
		clear() {
			stale = true;
			live.clear();
			if (long) long.hist?.clear();
			show(undefined, null);
		},
		destroy() {
			fetches++;
			chart.destroy();
			axis?.destroy();
		},
	};
}
