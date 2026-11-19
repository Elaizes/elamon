// Numbers as people read them, by the service's formats (docs/config.md, "Formats"), as the app
// writes them (Format.kt): elamond/fixtures/formats.json holds the cases both sides are tested
// against. Halves round away from zero; a unit follows its number after a space, but %, °C and °F
// follow it directly. Unit symbols and the few words a format writes read as the service writes
// them, so they live here rather than in strings.js. Pure: no DOM.

const BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
const BIT_UNITS = ['bit', 'kbit', 'Mbit', 'Gbit', 'Tbit'];
const PER_SECOND = '/s';
const BITS_PER_SECOND = 'bit/s';
export const PERCENT = '%';
const CELSIUS = '°C';
const FAHRENHEIT = '°F';
const MHZ = 'MHz';
const GHZ = 'GHz';
const WATT = 'W';
const WATT_HOUR = 'Wh';
const RPM = 'RPM';
/** A share above nothing that rounds to none: "<1%". */
const UNDER = '<1';
/** Units written right after their number. */
const ATTACHED = new Set([PERCENT, CELSIUS, FAHRENHEIT]);
/** What `bool` writes, and `duration`'s units: as the service writes them into strings. */
const YES = 'yes';
const NO = 'no';
const DAYS = 'd';
const HOURS = 'h';
const MINUTES = 'min';

const KILO = 1000;
const KIBI = 1024;
const SECONDS_PER_DAY = 86400;
const SECONDS_PER_HOUR = 3600;
const SECONDS_PER_MINUTE = 60;
const MS_PER_SECOND = 1000;

/** `{:.2}`: that many decimals. */
const PATTERN = /^\{:\.(\d)\}$/;

/** While a value is missing. */
export const DASH = '—';

/**
 * `v` with `decimals` decimals, halves away from zero, as the shortest text of the number reads
 * (12.25 → "12.3", which binary arithmetic would round down). No "-0": a figure rounded to nothing
 * has no sign.
 * @param {number} v @param {number} decimals @returns {string}
 */
export function round(v, decimals) {
	if (!Number.isFinite(v)) return Number.isNaN(v) ? 'NaN' : v > 0 ? 'Infinity' : '-Infinity';
	const negative = v < 0;
	const [mantissa, e] = String(Math.abs(v)).split('e');
	const [whole, frac = ''] = mantissa.split('.');
	let digits = whole + frac;
	let point = whole.length + Number(e || 0);
	if (point <= 0) {
		digits = '0'.repeat(1 - point) + digits;
		point = 1;
	}
	if (point > digits.length) digits += '0'.repeat(point - digits.length);
	const keep = point + decimals;
	const head = digits.slice(0, keep).padEnd(keep, '0').split('').map(Number);
	if (keep < digits.length && Number(digits[keep]) >= 5) {
		let i = head.length - 1;
		while (i >= 0) {
			if (head[i] < 9) {
				head[i]++;
				break;
			}
			head[i] = 0;
			i--;
		}
		if (i < 0) {
			head.unshift(1);
			point++;
		}
	}
	const int = head.slice(0, point).join('').replace(/^0+(?=\d)/, '');
	const dec = head.slice(point).join('');
	const text = decimals > 0 ? `${int}.${dec}` : int;
	return negative && /[1-9]/.test(text) ? `-${text}` : text;
}

/** `number` and its `unit`, with a space unless the unit follows it directly. @param {string} number @param {string | null | undefined} unit */
export function withUnit(number, unit) {
	if (!unit) return number;
	return ATTACHED.has(unit) ? number + unit : `${number} ${unit}`;
}

/** @param {boolean} b */
export function bool(b) {
	return b ? YES : NO;
}

/**
 * `v` by `format` and `unit`; `field` is the table field it comes from, for `time` (`t` is in
 * milliseconds). A string is shown as it is, a bool as yes or no; null when the value is missing
 * or isn't one value (a list).
 * @param {unknown} v @param {string | null | undefined} format @param {string | null | undefined} unit @param {string} [field]
 * @returns {string | null}
 */
export function format(v, format, unit, field) {
	if (v == null) return null;
	if (typeof v === 'string') return v;
	if (typeof v === 'boolean') return bool(v);
	if (typeof v === 'number') return number(v, format, unit, field);
	return null;
}

/** @param {number} v @param {string | null | undefined} format @param {string | null | undefined} unit @param {string} [field] */
export function number(v, format, unit, field) {
	const [n, u] = parts(v, format, unit, field);
	return withUnit(n, u);
}

/**
 * A number by `format` and `unit`, the figure and its unit apart, for layouts that set the unit smaller.
 * @param {number} v @param {string | null | undefined} format @param {string | null | undefined} unit @param {string} [field]
 * @returns {[string, string | null]}
 */
export function parts(v, format, unit, field) {
	const u = unit || null;
	const m = PATTERN.exec(format || '');
	if (m) return [round(v, Number(m[1])), u];
	switch (format) {
		case 'integer':
			return [round(v, 0), u];
		case 'percent':
			return [round(v, 0), PERCENT];
		case 'bytes':
			return bytesParts(v);
		case 'rate':
			return rateParts(v, unit === BITS_PER_SECOND);
		case 'temp':
			return [round(v, 0), u || CELSIUS];
		case 'mhz':
			return Number(round(v, 0)) >= KILO ? [round(v / KILO, 2), GHZ] : [round(v, 0), MHZ];
		case 'watts':
			return [round(v, 1), u || WATT];
		case 'wh':
			return [round(v, 1), u || WATT_HOUR];
		case 'rpm':
			return [round(v, 0), u || RPM];
		case 'load':
			return [round(v, 2), u];
		case 'duration':
			return [duration(Math.trunc(v)), null];
		case 'time':
			return [time(v, field === 't'), null];
		case 'bool':
			return [bool(v !== 0), null];
		default:
			// "number", "text" and any other: up to one decimal.
			return [round(v, 1).replace(/\.0$/, ''), u];
	}
}

/**
 * `value` in the largest of `units` (each `base` times the one before) that keeps it at or above 1,
 * with two decimals under 10, one under 100, none above. The rounded figure decides, so
 * 1023.7 KiB reads "1.00 MiB", not "1024 KiB", and 9.996 reads "10.0", not "10.00".
 * @param {number} value @param {number} base @param {string[]} units @returns {[string, string]}
 */
function scaled(value, base, units) {
	let v = Math.abs(value);
	let i = 0;
	while (v >= base && i < units.length - 1) {
		v /= base;
		i++;
	}
	let text = round(v, decimals(i, v));
	const rounded = Number(text);
	if (rounded >= base && i < units.length - 1) {
		v /= base;
		i++;
		text = round(v, decimals(i, v));
	} else if (decimals(i, rounded) !== decimals(i, v)) {
		text = round(v, decimals(i, rounded));
	}
	return [value < 0 && /[1-9]/.test(text) ? `-${text}` : text, units[i]];
}

/** @param {number} unit @param {number} v */
function decimals(unit, v) {
	if (unit === 0) return 0;
	if (v < 10) return 2;
	if (v < 100) return 1;
	return 0;
}

/** @param {number} b */
export function bytesParts(b) {
	return scaled(b, KIBI, BYTE_UNITS);
}

/** @param {number} b */
export function bytes(b) {
	const [n, u] = bytesParts(b);
	return `${n} ${u}`;
}

/** A rate in bytes per second, or in bits (by 1000) where the service sends bits. @param {number} perSecond @param {boolean} bits @returns {[string, string]} */
export function rateParts(perSecond, bits) {
	const [v, unit] = bits ? scaled(perSecond, KILO, BIT_UNITS) : bytesParts(perSecond);
	return [v, unit + PER_SECOND];
}

/** @param {number} p */
export function percent(p) {
	return round(p, 0) + PERCENT;
}

/** A timeline's share: "5%", or "<1%". @param {number | 'under'} share */
export function share(share) {
	return share === 'under' ? UNDER + PERCENT : `${share}${PERCENT}`;
}

/** "3 d 4 h", "4 h 12 min", "12 min"; a moment over nothing is a minute. @param {number} seconds */
export function duration(seconds) {
	const d = Math.trunc(seconds / SECONDS_PER_DAY);
	const h = Math.trunc((seconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR);
	const m = Math.trunc((seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
	if (d > 0) return `${d} ${DAYS} ${h} ${HOURS}`;
	if (h > 0) return `${h} ${HOURS} ${m} ${MINUTES}`;
	return `${Math.max(m, seconds > 0 ? 1 : 0)} ${MINUTES}`;
}

/** A moment, the browser's way: from Unix seconds, or milliseconds. @param {number} v @param {boolean} millis */
export function time(v, millis) {
	const ms = millis ? v : v * MS_PER_SECOND;
	return new Date(ms).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' });
}

/** A time of day to the second, the browser's clock: a journal line's. @param {number} ms */
export function clockTime(ms) {
	return new Date(ms).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23' });
}

/** A track's place, from seconds: "3:07" or "1:02:45". @param {number} seconds */
export function clock(seconds) {
	const s = Math.max(0, Math.trunc(seconds));
	const h = Math.trunc(s / SECONDS_PER_HOUR);
	const m = Math.trunc((s % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
	const sec = s % SECONDS_PER_MINUTE;
	const two = (/** @type {number} */ n) => String(n).padStart(2, '0');
	return h > 0 ? `${h}:${two(m)}:${two(sec)}` : `${m}:${two(sec)}`;
}
