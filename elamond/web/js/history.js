// A graph's (or a spark's) points as the page keeps them (the app's Hist.kt), and the long ranges
// `/api/v1/history` answers (HistoryRanges.kt). Pure: no DOM.

const SECOND_MS = 1000;
const MINUTE_MS = 60 * SECOND_MS;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** The long ranges by the names `ranges` gives them; `ms` null for all of it. */
export const HISTORY_RANGES = [
	{ key: '1h', ms: HOUR_MS },
	{ key: '24h', ms: DAY_MS },
	{ key: '7d', ms: 7 * DAY_MS },
	{ key: '30d', ms: 30 * DAY_MS },
	{ key: '1y', ms: 365 * DAY_MS },
	{ key: 'all', ms: null },
];

/**
 * @typedef {{ key: string, ms: number | null }} HistoryRange
 * One of the chips over a graph or a timeline: its own range, live (`range` null), or a long one.
 * @typedef {{ range: HistoryRange | null, ms: number }} RangeChip
 */

/**
 * The chips of a graph or timeline whose own range is `liveMs` and which offers `ranges`: its own
 * and the long ones the page knows, shortest first, leaving out one as long as its own. None
 * where it offers none (a small graph, a service before 0.7), or only its own would be left.
 * @param {number} liveMs @param {string[] | null | undefined} ranges @returns {RangeChip[]}
 */
export function rangeChips(liveMs, ranges) {
	const keys = new Set(ranges || []);
	const long = HISTORY_RANGES.filter((r) => keys.has(r.key) && r.ms !== liveMs);
	if (long.length === 0) return [];
	/** @type {RangeChip[]} */
	const chips = long.map((r) => ({ range: r, ms: r.ms ?? Number.MAX_SAFE_INTEGER }));
	chips.push({ range: null, ms: liveMs });
	return chips.sort((a, b) => a.ms - b.ms);
}

/**
 * A span as a chip says it: seconds under a minute, minutes under an hour, hours under two days,
 * then days; rounded. @param {number} ms @returns {['s' | 'min' | 'h' | 'd', number]}
 */
export function spanParts(ms) {
	if (ms < MINUTE_MS) return ['s', Math.round(ms / SECOND_MS)];
	if (ms < HOUR_MS) return ['min', Math.round(ms / MINUTE_MS)];
	if (ms < 2 * DAY_MS) return ['h', Math.round(ms / HOUR_MS)];
	return ['d', Math.round(ms / DAY_MS)];
}

/**
 * The points a view kept of widget `id` (in a page's `memory`), so a dashboard drawn again (the
 * settings changed) goes on with them; new ones where there were none, or of another shape.
 * @param {Map<string, any>} memory @param {string} id @param {number} series @param {number} windowMs @param {number} stepHintMs
 */
export function keptHist(memory, id, series, windowMs, stepHintMs) {
	const key = `${id}#points`;
	const had = memory.get(key);
	if (had instanceof Hist && had.series === Math.max(1, series)) {
		had.setWindow(windowMs);
		had.stepHintMs = stepHintMs;
		return had;
	}
	const h = new Hist(series, windowMs, stepHintMs);
	memory.set(key, h);
	return h;
}

/** Only a memory guard: two hours at a point a second. The window decides what stays. */
export const MAX_POINTS = 7200;
/** A step this many times the usual one is a gap: no line across it. */
export const GAP_FACTOR = 3;
const MIN_WINDOW_MS = 10_000;

/**
 * A graph's series by the service's clock, oldest first: `t` in Unix ms, `v[k]` one array per
 * series, null where a value was missing. The service's history replaces it; each data line's
 * values are appended at its time, and what falls out of the window (the graph's range) is
 * dropped, keeping one point before it so a line still enters from the edge.
 */
export class Hist {
	/** @param {number} series @param {number} windowMs @param {number} [stepHintMs] how often points come */
	constructor(series, windowMs, stepHintMs = 1000) {
		this.series = Math.max(1, series);
		this.windowMs = Math.max(windowMs, MIN_WINDOW_MS);
		this.stepHintMs = stepHintMs;
		/** @type {number[]} */
		this.t = [];
		/** @type {(number | null)[][]} */
		this.v = Array.from({ length: this.series }, () => []);
		/** Counts changes, for a drawing that has to tell a redraw apart. */
		this.version = 0;
	}

	get size() {
		return this.t.length;
	}

	get lastTime() {
		return this.t.length ? this.t[this.t.length - 1] : -Infinity;
	}

	/** @param {number} ms */
	setWindow(ms) {
		const w = Math.max(ms, MIN_WINDOW_MS);
		if (w === this.windowMs) return;
		this.windowMs = w;
		this.prune();
		this.version++;
	}

	clear() {
		this.t = [];
		this.v = Array.from({ length: this.series }, () => []);
		this.version++;
	}

	/** The service's record replaces the page's: it also covers the time nobody was watching. @param {{ t?: number[], v?: (number | null)[][] }} entry */
	replace(entry) {
		this.clear();
		const ts = entry.t || [];
		for (let i = 0; i < ts.length; i++) {
			if (this.t.length && ts[i] <= this.lastTime) continue;
			this.push(ts[i], (entry.v || []).map((s) => (s ? s[i] : null)));
		}
		this.prune();
		this.version++;
	}

	/**
	 * Adds `values` at `t`, newer than the last point; false for a time already there. A clock
	 * turned back by more than the whole window starts over.
	 * @param {number} t @param {(number | null | undefined)[]} values
	 */
	append(t, values) {
		if (this.t.length && t <= this.lastTime) {
			if (t >= this.lastTime - this.windowMs) return false;
			this.clear();
		}
		this.push(t, values);
		this.prune();
		this.version++;
		return true;
	}

	/** Adds `values` at `t` only once `stepMs` has passed since the newest point: a stream's points joining a long range's buckets. @param {number} t @param {(number | null | undefined)[]} values @param {number} stepMs */
	appendEvery(t, values, stepMs) {
		if (this.t.length && t - this.lastTime < stepMs) return false;
		return this.append(t, values);
	}

	/**
	 * A data line's points: its `history` when it has one (the first of a dashboard), and its own
	 * `values` at `t` where that doesn't reach so far, so the newest point is never left out.
	 * @param {number} t @param {(number | null | undefined)[]} values @param {{ t?: number[], v?: (number | null)[][] } | null | undefined} history
	 */
	take(t, values, history) {
		if (!history) {
			this.append(t, values);
			return;
		}
		this.replace(history);
		if (t > this.lastTime) this.append(t, values);
	}

	/** The usual time between points: what a gap is measured against. */
	medianStepMs() {
		if (this.t.length < 2) return 0;
		const steps = [];
		for (let i = 1; i < this.t.length; i++) steps.push(this.t[i] - this.t[i - 1]);
		steps.sort((a, b) => a - b);
		return steps[Math.floor(steps.length / 2)];
	}

	get gapMs() {
		return GAP_FACTOR * Math.max(this.medianStepMs(), this.stepHintMs);
	}

	/** @param {number} t @param {(number | null | undefined)[]} values */
	push(t, values) {
		if (this.t.length >= MAX_POINTS) {
			this.t.shift();
			for (const s of this.v) s.shift();
		}
		this.t.push(t);
		for (let k = 0; k < this.series; k++) {
			const x = values[k];
			this.v[k].push(typeof x === 'number' && Number.isFinite(x) ? x : null);
		}
	}

	/** Drops what fell out of the window, keeping one point before it. */
	prune() {
		const from = this.lastTime - this.windowMs;
		let drop = 0;
		while (this.t.length - drop > 1 && this.t[drop + 1] <= from) drop++;
		if (drop > 0) {
			this.t.splice(0, drop);
			for (const s of this.v) s.splice(0, drop);
		}
	}
}
