// Every widget type the page draws, by its name in `ui`: one module each. A type that isn't here
// is drawn as an honest notice (unknown.js).
import * as value from './value.js';
import * as gauge from './gauge.js';
import * as meter from './meter.js';
import * as status from './status.js';
import * as graph from './graph.js';
import * as list from './list.js';
import * as bars from './bars.js';
import * as table from './table.js';
import * as button from './button.js';
import * as toggle from './toggle.js';
import * as select from './select.js';
import * as slider from './slider.js';
import * as text from './text.js';
import * as group from './group.js';
import * as media from './media.js';
import * as timeline from './timeline.js';
import * as log from './log.js';
import * as unknown from './unknown.js';

/** @type {Record<string, { create: (w: import('./common.js').Widget, env: import('./common.js').Env) => import('./common.js').Instance }>} */
export const REGISTRY = { value, gauge, meter, status, graph, list, bars, table, button, toggle, select, slider, text, group, media, timeline, log };

/** The types the page draws, as `ui`, the stream and a preview ask for them. */
export const TYPES = Object.keys(REGISTRY);

/**
 * A widget drawn by its type; one the page doesn't know, or one that breaks while it is made, says so
 * rather than taking the dashboard down with it.
 * @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance}
 */
export function make(w, env) {
	const m = Object.hasOwn(REGISTRY, w.type) ? REGISTRY[w.type] : unknown;
	let inst;
	try {
		inst = m.create(w, env);
	} catch (e) {
		console.error(`widget ${w.id}`, e);
		inst = unknown.create(w, env);
	}
	const update = inst.update;
	// A state a widget can't take (a newer service's) leaves it as it was, not the whole page broken.
	inst.update = (s, data) => {
		try {
			update(s, data);
		} catch (e) {
			console.error(`widget ${w.id}`, e);
		}
	};
	return inst;
}
