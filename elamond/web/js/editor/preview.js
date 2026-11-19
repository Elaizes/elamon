// The live preview: the draft sent to the service (`POST /api/v1/admin/preview`) a moment after each
// change and every interval while the page is in sight; the service answers the `ui` the draft
// makes and the dashboard's data from the reading there is now, and writes nothing.
import { api } from '../api.js';
import { DRAWS } from '../panels.js';

export class Previewer {
	/**
	 * @param {{ body: () => Record<string, unknown>, interval: number, done: (answer: any) => void, failed: (e: unknown) => void }} opts
	 */
	constructor(opts) {
		this.opts = opts;
		/** @type {ReturnType<typeof setTimeout> | undefined} */
		this.timer = undefined;
		/** @type {ReturnType<typeof setTimeout> | undefined} */
		this.wait = undefined;
		this.busy = false;
		this.again = false;
		this.stopped = false;
		/** @type {AbortController | null} */
		this.ctrl = null;
		this.onVisible = () => {
			if (document.hidden) clearTimeout(this.timer);
			else this.soon(0);
		};
		document.addEventListener('visibilitychange', this.onVisible);
	}

	/** Asks again a moment from now (each change puts it off, so typing sends one). @param {number} [ms] */
	soon(ms = 350) {
		clearTimeout(this.wait);
		this.wait = setTimeout(() => this.run(), ms);
	}

	async run() {
		if (this.stopped) return;
		clearTimeout(this.timer);
		if (this.busy) {
			this.again = true;
			return;
		}
		this.busy = true;
		const ctrl = (this.ctrl = new AbortController());
		try {
			const answer = await api('/api/v1/admin/preview', { body: { ...this.opts.body(), widgets: DRAWS }, signal: ctrl.signal });
			if (!this.stopped) this.opts.done(answer);
		} catch (e) {
			if (!this.stopped && !(e instanceof DOMException && e.name === 'AbortError')) this.opts.failed(e);
		} finally {
			this.busy = false;
			this.ctrl = null;
			if (this.again) {
				this.again = false;
				this.run();
			} else this.schedule();
		}
	}

	schedule() {
		clearTimeout(this.timer);
		if (!this.stopped && !document.hidden) this.timer = setTimeout(() => this.run(), this.opts.interval);
	}

	stop() {
		this.stopped = true;
		clearTimeout(this.timer);
		clearTimeout(this.wait);
		if (this.ctrl) this.ctrl.abort();
		document.removeEventListener('visibilitychange', this.onVisible);
	}
}
