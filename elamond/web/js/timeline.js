// A timeline's arithmetic (the app's ChartMath.kt, timelines): where its stretches fall on the bar,
// how long each state lasted, its legend in durations, shares or both, and a long range carried on
// by the stream. Pure.

/** The states a timeline draws, in the order its legend has them. */
export const TIMELINE_STATES = ['line', 'battery', 'none'];

/** The states that count as known: on line and on battery. */
const KNOWN_STATES = new Set(['line', 'battery']);

const PERCENT = 100;
const MS_PER_S = 1000;

/**
 * @typedef {{ from: number, to: number, state: string }} Segment
 * A segment as drawn, from `x0` to `x1`; `widened` when drawn wider than its time, to be seen at all.
 * @typedef {{ x0: number, x1: number, segment: Segment, widened: boolean }} Stretch
 * @typedef {{ state: string, seconds: number | null, share: number | 'under' | null }} LegendEntry
 */

/**
 * `segments` on a bar `width` wide from `start` to `end` (Unix ms): each clipped to it, those
 * outside it left out, each at least `minPx` wide around its middle and inside the bar.
 * @param {Segment[]} segments @param {number} start @param {number} end @param {number} width @param {number} minPx @returns {Stretch[]}
 */
export function stretches(segments, start, end, width, minPx) {
	if (!(end > start) || width <= 0) return [];
	const span = end - start;
	/** @type {Stretch[]} */
	const out = [];
	for (const s of segments) {
		const from = Math.max(s.from, start);
		const to = Math.min(s.to, end);
		if (to <= from) continue;
		const x0 = ((from - start) / span) * width;
		const x1 = ((to - start) / span) * width;
		const segment = { from, to, state: s.state };
		if (x1 - x0 >= minPx) {
			out.push({ x0, x1, segment, widened: false });
			continue;
		}
		const w = Math.min(minPx, width);
		const left = Math.min(Math.max((x0 + x1) / 2 - w / 2, 0), width - w);
		out.push({ x0: left, x1: left + w, segment, widened: true });
	}
	return out;
}

/** The stretch under `x`: a widened one first, as it is drawn on top; -1 for none. @param {Stretch[]} all @param {number} x */
export function stretchAt(all, x) {
	const wide = all.findIndex((s) => s.widened && x >= s.x0 && x <= s.x1);
	return wide >= 0 ? wide : all.findIndex((s) => x >= s.x0 && x <= s.x1);
}

/** How long each state lasts from `start` to `end`, in ms, by state. @param {Segment[]} segments @param {number} start @param {number} end @returns {Map<string, number>} */
export function timeIn(segments, start, end) {
	/** @type {Map<string, number>} */
	const out = new Map();
	for (const s of segments) {
		const d = Math.min(s.to, end) - Math.max(s.from, start);
		if (d > 0) out.set(s.state, (out.get(s.state) || 0) + d);
	}
	return out;
}

/**
 * Each known state's share of the time something was known (on line and on battery), whole
 * percent each rounded on its own, `'under'` for above nothing but under 1%; none where nothing
 * was known. @param {Map<string, number>} totals @returns {Map<string, number | 'under'>}
 */
export function shares(totals) {
	const known = [...totals].filter(([state, ms]) => KNOWN_STATES.has(state) && ms > 0);
	const sum = known.reduce((a, [, ms]) => a + ms, 0);
	/** @type {Map<string, number | 'under'>} */
	const out = new Map();
	if (sum <= 0) return out;
	for (const [state, ms] of known) {
		const p = (ms * PERCENT) / sum;
		out.set(state, p < 1 ? 'under' : Math.round(p));
	}
	return out;
}

/** What the legend tells, by the widget's `legend`: a word the page doesn't know, or none, tells durations. @param {string | null | undefined} key @returns {'duration' | 'percent' | 'both'} */
export function legendMode(key) {
	return key === 'percent' || key === 'both' ? key : 'duration';
}

/**
 * The legend of `totals` in `mode`: the states in order, those the range has. A state without a
 * share ("none", or any where nothing was known) is told by its duration, whatever the mode.
 * @param {Map<string, number>} totals @param {'duration' | 'percent' | 'both'} mode @param {string[]} [states] @returns {LegendEntry[]}
 */
export function legendEntries(totals, mode, states = TIMELINE_STATES) {
	const sh = mode === 'duration' ? new Map() : shares(totals);
	/** @type {LegendEntry[]} */
	const out = [];
	for (const state of states) {
		const ms = totals.get(state);
		if (ms == null) continue;
		const share = sh.get(state) ?? null;
		const seconds = mode !== 'percent' || share == null ? Math.trunc(ms / MS_PER_S) : null;
		out.push({ state, seconds, share });
	}
	return out;
}

/**
 * A long range's `history`, which reaches `to`, carried on to now by the stream's `live`
 * segments from then on; a stretch that goes on across `to` is one.
 * @param {Segment[]} history @param {number} to @param {Segment[]} live @returns {Segment[]}
 */
export function splice(history, to, live) {
	const out = history.map((s) => ({ ...s }));
	for (const s of live) {
		if (s.to <= to) continue;
		const next = s.from < to ? { ...s, from: to } : { ...s };
		const last = out[out.length - 1];
		if (last && last.state === next.state && last.to === next.from) last.to = next.to;
		else out.push(next);
	}
	return out;
}
