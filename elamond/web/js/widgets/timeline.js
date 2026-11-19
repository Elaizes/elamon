// `timeline`: when a source was on line, on battery, or nothing was known, as a bar over its range:
// live, the stream's segments; a long range, the computer's, carried on to now by the stream's.
// Under it the time, and each state's duration, share of the known time, or both (`legend`); a
// pointer on it reads one stretch.
import { h, setText, setStyle } from '../dom.js';
import { api } from '../api.js';
import { card, captionLine, chips, captionOf } from './common.js';
import { sharpCanvas, cssColor } from './canvas.js';
import { rangeLabel, spanLabel, drawAxis } from './graph.js';
import { rangeChips } from '../history.js';
import { stretches, stretchAt, timeIn, legendEntries, legendMode, splice, TIMELINE_STATES } from '../timeline.js';
import { S, fill } from '../strings.js';
import * as F from '../format.js';

/** A timeline's range while the service doesn't say: a day. */
const DEFAULT_S = 86_400;
/** The least width a stretch is drawn, to be seen at all. */
const STRETCH_MIN_PX = 2;

/** @param {string} state */
function stateColor(state) {
	return state === 'line' ? 'var(--good)' : state === 'battery' ? 'var(--warn)' : 'var(--surface-2)';
}

/** @param {string} state */
function stateLabel(state) {
	return state === 'line' ? S.panels.timelineLine : state === 'battery' ? S.panels.timelineBattery : S.panels.timelineNone;
}

/** "On battery · 12 min", "On battery · 4%", "On battery · 4% · 12 min". @param {import('../timeline.js').LegendEntry} e */
export function legendText(e) {
	const told = [e.share != null ? F.share(e.share) : null, e.seconds != null ? F.duration(e.seconds) : null].filter(Boolean).join(' · ');
	return fill(S.panels.timelineLegend, stateLabel(e.state), told);
}

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const liveMs = (typeof w.range === 'number' && w.range > 0 ? w.range : DEFAULT_S) * 1000;
	const chipsOf = rangeChips(liveMs, w.ranges);
	const rangeKey = `${w.id}#range`;
	const mode = legendMode(w.legend);
	/** @type {import('../timeline.js').Segment[]} */
	let live = [];
	/** @type {{ key: string, chip: import('../history.js').RangeChip, history: { from: number, to: number, segments: import('../timeline.js').Segment[] } | null, loading: boolean } | null} */
	let long = null;
	let fetches = 0;
	/** Shown again after a drop or a hidden page: a long range is asked for anew with the next line. */
	let stale = false;
	/** @type {{ segments: import('../timeline.js').Segment[], start: number, end: number }} */
	let view = { segments: [], start: 0, end: 0 };
	/** @type {number | null} */
	let pointerX = null;

	const { el, body } = card(w, env);
	const note = h('p', { class: 'graph-note muted' });
	note.hidden = true;
	const touched = h('p', { class: 'timeline-touched' });
	touched.hidden = true;
	const legends = h('div', { class: 'legends' });
	const cap = captionLine();
	const bar = sharpCanvas('timeline-bar', (ctx, width, height) => {
		const all = stretches(view.segments, view.start, view.end, width, STRETCH_MIN_PX);
		ctx.fillStyle = cssColor(el, stateColor('none'));
		ctx.fillRect(0, 0, width, height);
		for (const wide of [false, true]) {
			for (const st of all) {
				if (st.widened !== wide) continue;
				ctx.fillStyle = cssColor(el, stateColor(st.segment.state));
				ctx.fillRect(st.x0, 0, st.x1 - st.x0, height);
			}
		}
		if (pointerX != null) {
			ctx.fillStyle = cssColor(el, 'var(--text)');
			ctx.fillRect(Math.round(pointerX), 0, 1, height);
			const i = stretchAt(all, pointerX);
			const seg = i >= 0 ? all[i].segment : null;
			touched.hidden = !seg;
			legends.hidden = !!seg;
			if (seg) setText(touched, stretchText(seg));
		} else {
			touched.hidden = true;
			legends.hidden = false;
		}
		axis.redraw();
	});
	bar.el.setAttribute('role', 'img');
	const axis = sharpCanvas('chart-axis', (ctx, width) => {
		if (view.segments.length && view.end > view.start) drawAxis(ctx, width, view.start, view.end, cssColor(el, 'var(--muted)'));
	});
	axis.el.setAttribute('aria-hidden', 'true');
	bar.el.addEventListener('pointermove', (e) => {
		pointerX = e.offsetX;
		bar.redraw();
	});
	bar.el.addEventListener('pointerleave', () => {
		pointerX = null;
		bar.redraw();
	});

	/** @type {ReturnType<typeof chips> | null} */
	let picker = null;
	if (chipsOf.length) {
		picker = chips(S.panels.rangeChips, chipsOf.map((c) => ({ value: c.range ? c.range.key : '', text: rangeLabel(c) })), '', (key) => pick(key));
		if (!env.live) picker.el.setAttribute('aria-disabled', 'true');
		body.append(picker.el);
		const kept = env.memory.get(rangeKey);
		if (kept && env.live && chipsOf.some((c) => c.range && c.range.key === kept)) {
			picker.select(kept);
			queueMicrotask(() => pick(kept));
		}
	}
	body.append(note, bar.el, axis.el, touched, legends, cap.el);

	/** One stretch as a pointer reads it: "On battery · 16/11/2026, 14:02 – 14:14 · 12 min". @param {import('../timeline.js').Segment} s */
	function stretchText(s) {
		const sameDay = new Date(s.from).toDateString() === new Date(s.to).toDateString();
		const to = sameDay ? new Date(s.to).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }) : F.time(s.to, true);
		return fill(S.panels.timelineTouched, stateLabel(s.state), F.time(s.from, true), to, F.duration(Math.trunc((s.to - s.from) / 1000)));
	}

	/** @param {string} key */
	function pick(key) {
		if (!env.live) {
			picker?.select(long ? long.key : '');
			return;
		}
		env.memory.set(rangeKey, key || null);
		if (!key) {
			long = null;
			note.hidden = true;
			redraw();
			return;
		}
		const chip = chipsOf.find((c) => c.range && c.range.key === key);
		if (!chip) return;
		long = { key, chip, history: long && long.key === key ? long.history : null, loading: true };
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
			asked.history = { from: r.from, to: r.to, segments: Array.isArray(r.segments) ? r.segments : [] };
			asked.loading = false;
		} catch {
			if (n !== fetches || long !== asked) return;
			asked.loading = false;
			if (!asked.history) {
				setText(note, fill(S.panels.rangeFailed, rangeLabel(asked.chip)));
				note.hidden = false;
				long = null;
				picker?.select('');
				redraw();
				return;
			}
		}
		showNote();
		redraw();
	}

	function showNote() {
		const loading = !!long && long.loading && !long.history;
		note.hidden = !loading;
		setText(note, loading ? S.panels.loading : '');
	}

	function redraw() {
		const history = long ? long.history : null;
		const segments = !long ? live : history ? splice(history.segments, history.to, live) : [];
		const end = Math.max(segments.length ? segments[segments.length - 1].to : 0, history ? history.to : 0);
		const start = history && long && long.key === 'all' ? history.from : end - (long && long.chip.range && long.chip.range.ms != null ? long.chip.range.ms : liveMs);
		view = { segments, start, end };
		const totals = segments.length ? timeIn(segments, start, end) : new Map();
		const entries = legendEntries(totals, mode, TIMELINE_STATES);
		legends.replaceChildren(
			...entries.map((e) => {
				const dot = h('span', { class: 'legend-dot', 'aria-hidden': 'true' });
				setStyle(dot, { background: stateColor(e.state) });
				return h('span', { class: 'legend' }, dot, h('span', { class: 'legend-text' }, legendText(e)));
			}),
		);
		const span = long ? rangeLabel(long.chip) : spanLabel(liveMs);
		bar.el.setAttribute('aria-label', fill(S.panels.chartSummary, span, entries.map(legendText).join(', ')));
		bar.redraw();
	}

	/** @param {import('./common.js').WidgetState | undefined} s @param {import('./common.js').Data | null} data */
	function show(s, data) {
		live = s && Array.isArray(s.segments) ? s.segments : [];
		if (data && (data.history || stale) && long && env.live) fetchLong();
		if (data) stale = false;
		cap.set(captionOf(w, s));
		redraw();
	}
	show(undefined, null);

	return {
		el,
		update: (s, data) => show(s, data),
		clear() {
			stale = true;
			show(undefined, null);
		},
		destroy() {
			fetches++;
			bar.destroy();
			axis.destroy();
		},
	};
}
