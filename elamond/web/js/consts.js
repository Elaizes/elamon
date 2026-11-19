// The page's numbers, in one place: the protocol's defaults where `ui` leaves a value out, how long
// notes stay, how often the editor's preview asks, the dashboards' grid. Pure.

/** A reading's interval when neither the profile nor the service says one (`interval_ms`). */
export const DEFAULT_INTERVAL_MS = 1000;

/** How far back graphs reach when the service doesn't say (`history_seconds`). */
export const DEFAULT_HISTORY_S = 600;

/** How long a note at the bottom of the page stays; one that warns, longer. */
export const TOAST_MS = 4000;
export const TOAST_LOUD_MS = 8000;

/** The editor's preview asks again at the reading's interval, never more often than this. */
export const EDITOR_PREVIEW_MIN_MS = 1000;

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
