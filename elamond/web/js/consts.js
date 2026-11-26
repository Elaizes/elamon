// The page's numbers, in one place: the protocol's defaults where `ui` leaves a value out, the
// dashboards' grid (as style.css and the app have it), how long things wait and how often they ask,
// how much is asked for and drawn. Pure.

// ---------------------------------------------------------------- the protocol's defaults

/** A reading's interval when neither the profile nor the service says one (`interval_ms`). */
export const DEFAULT_INTERVAL_MS = 1000;

/** How far back graphs reach when the service doesn't say (`history_seconds`). */
export const DEFAULT_HISTORY_S = 600;

/** How far back a timeline reaches when its widget doesn't say (`range`). */
export const TIMELINE_DEFAULT_S = 86_400;

/** Lines a log keeps when its widget doesn't say (`lines`). */
export const LOG_LINES_DEFAULT = 200;

// ---------------------------------------------------------------- the grid (style.css, the app's Grid)

/** The columns a widget's span counts in (the service's), and the grid's on a screen that isn't narrow; a narrow one has half. */
export const COLUMNS = 4;

/** The most columns a wide grid has: a span of 4 is half a row there. */
export const WIDE_COLUMNS = 8;

/** A column of the grid is never narrower than this (CSS px): four of them at NARROW_PX. */
export const COLUMN_MIN_PX = 150;

/** From this width of the grid (CSS px) on, the layout's "Auto" has WIDE_COLUMNS: a span of 1 is then about 190 px. */
export const AUTO_WIDE_PX = 1500;

/** Narrower than this (CSS px), the grid has two columns. */
export const NARROW_PX = 600;

/** Narrower than this, one: two would leave a tile too narrow for a name beside its icon. */
export const ONE_COLUMN_PX = 320;

/** The least width of a widget, in columns of four (the most is `COLUMNS`). */
export const SPAN_MIN = 1;

// ---------------------------------------------------------------- the page

/** How long the page waits for the person at the computer to let a browser in: longer than elamond pair asks there. */
export const PAIR_WAIT_MS = 100_000;

/** How long a stream outlives its page when the address changes, for the next page to take it over. */
export const ADOPT_MS = 10_000;

/** How long a note at the bottom of the page stays; one that warns, longer. */
export const TOAST_MS = 4000;
export const TOAST_LOUD_MS = 8000;

/** A preview (the editor's, the admin panel's) asks again at the reading's interval, never more often than this. */
export const PREVIEW_MIN_MS = 1000;

/** The editor's preview, after a change: soon after widgets came, went or moved; after typing, once it stops. */
export const PREVIEW_AFTER_SHAPE_MS = 150;
export const PREVIEW_AFTER_TYPING_MS = 400;
/** …and after anything else that asks for one. */
export const PREVIEW_SOON_MS = 350;

// ---------------------------------------------------------------- the admin panel

/** How often an open pairing asks what is pending (a code, someone waiting to be let in). */
export const PAIRING_POLL_MS = 1500;

/** How often a countdown (a code's expiry) is drawn again. */
export const TICK_MS = 1000;

/** How long after the last key a draft is checked. */
export const CHECK_DEBOUNCE_MS = 600;

/** How long after the last key the settings' filter is applied. */
export const FILTER_DEBOUNCE_MS = 150;

/** How many entries of the activity log and of the events are asked for. */
export const LIST_LIMIT = 200;

/** A QR code: the quiet zone around it, in modules; about how wide it is drawn, in CSS pixels. Its colours are style.css's --qr-ink and --qr-paper (dark on light in every theme, for the cameras). */
export const QR_QUIET = 4;
export const QR_PIXELS = 240;

/** A list of plain values this long or shorter is shown on one line in the settings' tree. */
export const INLINE_LIST_MAX = 80;

/** At most this many characters of a drop-in's name before `.toml` (the protocol's limit). */
export const DROPIN_STEM_MAX = 64;

/** Below this many seconds a time is "just now". */
export const JUST_NOW_S = 60;

// ---------------------------------------------------------------- from `ui`

/** @typedef {{ profile?: { interval_ms?: number } | null, service?: { interval_ms?: number, history_seconds?: number } | null }} Timed */

/** The interval of readings `ui` gives: its profile's, else the service's, else the default. @param {Timed | null | undefined} ui */
export function intervalOf(ui) {
	const p = ui && ui.profile && ui.profile.interval_ms;
	const s = ui && ui.service && ui.service.interval_ms;
	return (typeof p === 'number' && p > 0 && p) || (typeof s === 'number' && s > 0 && s) || DEFAULT_INTERVAL_MS;
}

/** How far back the service keeps graphs' readings, in ms. @param {Timed | null | undefined} ui */
export function historyMsOf(ui) {
	const s = ui && ui.service && ui.service.history_seconds;
	return (typeof s === 'number' && s > 0 ? s : DEFAULT_HISTORY_S) * 1000;
}
