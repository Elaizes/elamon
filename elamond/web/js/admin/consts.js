// The admin panel's numbers, in one place: how often it asks, how long it waits, how big it draws.

/** How often an open pairing asks what is pending (a code, someone waiting to be let in). */
export const PAIRING_POLL_MS = 1500;

/** How often a countdown (a code's expiry) is drawn again. */
export const TICK_MS = 1000;

/** How long after the last key a draft is checked. */
export const CHECK_DEBOUNCE_MS = 600;

/** How long after the last key the settings' filter is applied. */
export const FILTER_DEBOUNCE_MS = 150;

/** A preview asks again at the profile's interval, never more often than this… */
export const PREVIEW_MIN_MS = 1000;

/** …and at this when the answer doesn't say its interval. */
export const PREVIEW_FALLBACK_MS = 2000;

/** How many entries of the activity log and of the events are asked for. */
export const LIST_LIMIT = 200;

/** A QR code: the quiet zone around it, in modules; about how wide it is drawn, in CSS pixels; its colours (always dark on light, for the cameras). */
export const QR_QUIET = 4;
export const QR_PIXELS = 240;
export const QR_DARK = '#000000';
export const QR_LIGHT = '#ffffff';

/** A list of plain values this long or shorter is shown on one line in the settings' tree. */
export const INLINE_LIST_MAX = 80;

/** At most this many characters of a drop-in's name before `.toml` (the protocol's limit). */
export const DROPIN_STEM_MAX = 64;

/** Below this many seconds a time is "just now". */
export const JUST_NOW_S = 60;
