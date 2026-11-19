// Times as the admin panel says them: "5 min ago", "3 h 12 min", "4:59", "2026-11-19 14:05".
import { S, fill } from '../strings.js';
import { JUST_NOW_S } from '../consts.js';

const MINUTE = 60;
const HOUR = 3600;
const DAY = 86400;

/**
 * How long ago a Unix time (seconds) was, seen from `now` (seconds): "just now", "5 min ago",
 * "3 h ago", "1 day ago", "12 days ago"; "never" for none. A time ahead of `now` (the clocks
 * differ) is "just now".
 * @param {number | null | undefined} t @param {number} now
 */
export function ago(t, now) {
	const T = S.admin.time;
	if (!t || !Number.isFinite(t)) return T.never;
	const d = Math.floor(now - t);
	if (d < JUST_NOW_S) return T.justNow;
	if (d < HOUR) return fill(T.minAgo, Math.floor(d / MINUTE));
	if (d < DAY) return fill(T.hAgo, Math.floor(d / HOUR));
	const days = Math.floor(d / DAY);
	return days === 1 ? T.dayAgo : fill(T.daysAgo, days);
}

/**
 * A span of seconds: "45 s", "12 min", "3 h 5 min", "2 d 4 h".
 * @param {number} seconds
 */
export function duration(seconds) {
	const T = S.admin.time;
	const s = Math.max(0, Math.floor(seconds));
	if (s < MINUTE) return fill(T.s, s);
	if (s < HOUR) return fill(T.min, Math.floor(s / MINUTE));
	if (s < DAY) return fill(T.hMin, Math.floor(s / HOUR), Math.floor((s % HOUR) / MINUTE));
	return fill(T.dH, Math.floor(s / DAY), Math.floor((s % DAY) / HOUR));
}

/** Seconds left as `m:ss`, never below `0:00`. @param {number} seconds */
export function countdown(seconds) {
	const s = Math.max(0, Math.ceil(seconds));
	return `${Math.floor(s / MINUTE)}:${String(s % MINUTE).padStart(2, '0')}`;
}

/** @param {number} n */
const two = (n) => String(n).padStart(2, '0');

/**
 * A Unix time (seconds) in this browser's time zone: `2026-11-19 14:05`, with `:07` seconds when
 * asked; empty for none.
 * @param {number | null | undefined} t @param {boolean} [seconds]
 */
export function dateTime(t, seconds = false) {
	if (!t || !Number.isFinite(t)) return '';
	const d = new Date(t * 1000);
	const day = `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
	const time = `${two(d.getHours())}:${two(d.getMinutes())}${seconds ? `:${two(d.getSeconds())}` : ''}`;
	return `${day} ${time}`;
}

/** The same for a Unix time in milliseconds (the activity log's, the events'). @param {number | null | undefined} ms */
export function dateTimeMs(ms) {
	return ms && Number.isFinite(ms) ? dateTime(ms / 1000, true) : '';
}

/** Now, as a Unix time in seconds. */
export function nowS() {
	return Date.now() / 1000;
}
