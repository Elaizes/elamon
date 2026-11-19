// Where things fall on a chart and how its scale and time axis read (the app's ChartMath.kt). Pure.

/** Room above the highest point, so a line doesn't run along the top. */
export const CHART_HEADROOM = 1.15;
/** Rows the dashed grid divides a chart into. */
export const CHART_GRID_ROWS = 4;

/** Where a moment falls on a chart `width` wide whose right edge is `tLast` and which spans `windowMs`. @param {number} t @param {number} tLast @param {number} windowMs @param {number} width */
export function chartX(t, tLast, windowMs, width) {
	return width - ((tLast - t) / windowMs) * width;
}

/** The moment at `x` on such a chart. @param {number} x @param {number} tLast @param {number} windowMs @param {number} width */
export function timeAtX(x, tLast, windowMs, width) {
	return tLast - Math.trunc(((width - x) / width) * windowMs);
}

/** The index of the time closest to `t` in the growing `times`, or -1 when there are none. @param {number[]} times @param {number} t */
export function nearestIndex(times, t) {
	if (times.length === 0) return -1;
	let lo = 0;
	let hi = times.length - 1;
	while (lo < hi) {
		const mid = (lo + hi) >>> 1;
		if (times[mid] < t) lo = mid + 1;
		else hi = mid;
	}
	return lo > 0 && Math.abs(t - times[lo - 1]) <= Math.abs(times[lo] - t) ? lo - 1 : lo;
}

/** The first point to draw from `tStart` on: the last one at or before it, so the line enters from the edge. @param {number[]} times @param {number} tStart */
export function firstVisibleIndex(times, tStart) {
	if (times.length === 0) return 0;
	let lo = 0;
	let hi = times.length - 1;
	while (lo < hi) {
		const mid = (lo + hi + 1) >>> 1;
		if (times[mid] <= tStart) lo = mid;
		else hi = mid - 1;
	}
	return lo;
}

const NICE_STEPS = [1, 2, 2.5, 5, 10];

/** 1, 2, 2.5, 5, 10… so the scale label reads as a round number. @param {number} v */
export function niceCeiling(v) {
	if (Number.isNaN(v) || v <= 0) return 1;
	if (!Number.isFinite(v)) return Number.MAX_VALUE;
	let p = 1;
	while (p * 10 <= v) p *= 10;
	while (p > v) p /= 10;
	if (p === 0) return v;
	for (const m of NICE_STEPS) if (p * m >= v) return p * m;
	return p * 10;
}

/** The next power of two at or above `v`: 512 KiB, 1 MiB, 2 MiB… @param {number} v */
export function niceBinary(v) {
	let p = 1;
	while (p < v && Number.isFinite(p)) p *= 2;
	return Number.isFinite(p) ? p : Number.MAX_VALUE;
}

/**
 * The value at a chart's top: `maxValue` when the scale is fixed; otherwise the data's `top` with
 * headroom, never under `minScale`, rounded up (`binary` for bytes).
 * @param {number} top @param {number | null | undefined} maxValue @param {number} minScale @param {boolean} binary
 */
export function chartScale(top, maxValue, minScale, binary) {
	if (maxValue != null && Number.isFinite(maxValue) && maxValue > 0) return maxValue;
	const wanted = Math.max(top * CHART_HEADROOM, minScale);
	const scale = binary ? niceBinary(wanted) : niceCeiling(wanted);
	return Number.isNaN(scale) || scale <= 0 ? 1 : scale;
}

// ---- time axes ---------------------------------------------------------------------------------

/** @typedef {'time' | 'day' | 'month' | 'year'} TickUnit */

/** Steps between labels: n of a unit, each label at a round local moment of it. */
const TICK_STEPS = /** @type {const} */ ([
	['minute', 1, 'time'], ['minute', 2, 'time'], ['minute', 5, 'time'], ['minute', 10, 'time'], ['minute', 15, 'time'], ['minute', 30, 'time'],
	['hour', 1, 'time'], ['hour', 2, 'time'], ['hour', 3, 'time'], ['hour', 6, 'time'], ['hour', 12, 'time'],
	['day', 1, 'day'], ['day', 2, 'day'], ['week', 1, 'day'], ['week', 2, 'day'],
	['month', 1, 'month'], ['month', 2, 'month'], ['month', 3, 'month'], ['month', 6, 'month'],
	['year', 1, 'year'], ['year', 2, 'year'], ['year', 5, 'year'], ['year', 10, 'year'],
]);

const DAY_MS = 86_400_000;
/** 1970-01-05, the epoch's first Monday, as a day of the epoch. */
const FIRST_MONDAY = 4;

/** Days since the epoch of the local date of `d`. @param {Date} d */
function epochDay(d) {
	return Math.round(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / DAY_MS);
}

/** @param {number} a @param {number} n */
function floorMod(a, n) {
	return ((a % n) + n) % n;
}

/** The round local moment of a step at or before `ms`. @param {string} unit @param {number} n @param {number} ms */
function firstTick(unit, n, ms) {
	const d = new Date(ms);
	const day = new Date(d.getFullYear(), d.getMonth(), d.getDate());
	switch (unit) {
		case 'minute':
			return new Date(d.getFullYear(), d.getMonth(), d.getDate(), d.getHours(), d.getMinutes() - (d.getMinutes() % n));
		case 'hour':
			return new Date(d.getFullYear(), d.getMonth(), d.getDate(), d.getHours() - (d.getHours() % n));
		case 'day':
			return new Date(day.getFullYear(), day.getMonth(), day.getDate() - floorMod(epochDay(day), n));
		case 'week': {
			const monday = new Date(day.getFullYear(), day.getMonth(), day.getDate() - floorMod(day.getDay() - 1, 7));
			const weeks = Math.floor((epochDay(monday) - FIRST_MONDAY) / 7);
			return new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() - 7 * floorMod(weeks, n));
		}
		case 'month':
			return new Date(day.getFullYear(), day.getMonth() - (day.getMonth() % n), 1);
		default:
			return new Date(day.getFullYear() - floorMod(day.getFullYear(), n), 0, 1);
	}
}

/** `d` a step later, by the local calendar. @param {Date} d @param {string} unit @param {number} n */
function plus(d, unit, n) {
	const y = d.getFullYear();
	const m = d.getMonth();
	const day = d.getDate();
	switch (unit) {
		case 'minute':
			return new Date(y, m, day, d.getHours(), d.getMinutes() + n);
		case 'hour':
			return new Date(d.getTime() + n * 3_600_000);
		case 'day':
			return new Date(y, m, day + n);
		case 'week':
			return new Date(y, m, day + 7 * n);
		case 'month':
			return new Date(y, m + n, 1);
		default:
			return new Date(y + n, 0, 1);
	}
}

/**
 * The labels of a time axis from `start` to `end` (Unix ms), at most `most` of them: at round
 * moments of the browser's clock (every 6 hours from midnight, each Monday, each first of a
 * month), the shortest step that keeps to `most`. None when `most` is under 1 or the axis spans nothing.
 * @param {number} start @param {number} end @param {number} most @returns {{ unit: TickUnit, times: number[] }}
 */
export function timeTicks(start, end, most) {
	/** @type {{ unit: TickUnit, times: number[] }} */
	const none = { unit: 'time', times: [] };
	if (most < 1 || !(end > start) || !Number.isFinite(start) || !Number.isFinite(end)) return none;
	for (const [unit, n, reads] of TICK_STEPS) {
		let at = firstTick(unit, n, start);
		const times = [];
		let fits = true;
		for (let guard = 0; guard < 100_000; guard++) {
			const ms = at.getTime();
			if (Number.isNaN(ms)) return none;
			if (ms > end) break;
			if (ms >= start) {
				if (times.length === most) {
					fits = false;
					break;
				}
				times.push(ms);
			}
			at = plus(at, unit, n);
		}
		if (fits) return { unit: reads, times };
	}
	return none;
}

/**
 * Where the labels of an axis `width` wide go: each centred on its x, kept inside the axis; one
 * that would come closer than `gap` to the one before is left out. Each shown label's index and left edge.
 * @param {number[]} xs @param {number[]} widths @param {number} width @param {number} gap @returns {[number, number][]}
 */
export function axisLabels(xs, widths, width, gap) {
	/** @type {[number, number][]} */
	const out = [];
	let right = -Infinity;
	for (let i = 0; i < xs.length; i++) {
		const left = Math.min(Math.max(xs[i] - widths[i] / 2, 0), Math.max(0, width - widths[i]));
		if (left < right + gap) continue;
		out.push([i, left]);
		right = left + widths[i];
	}
	return out;
}

/** The left edge of a tooltip `boxWidth` wide, centred on `x` but inside a chart `width` wide. @param {number} x @param {number} boxWidth @param {number} width */
export function tooltipLeft(x, boxWidth, width) {
	return Math.min(Math.max(x - boxWidth / 2, 0), Math.max(0, width - boxWidth));
}
