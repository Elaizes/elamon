// `value`: a number or text, large, coloured by its level; with `spark`, a small graph of it under it.
import { card, captionLine, figure, setLevel, numberOf, accent } from './common.js';
import { sharpCanvas, cssColor, levelColor } from './canvas.js';
import { drawChart } from './chart.js';
import { Hist } from '../history.js';
import * as F from '../format.js';

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	const { el, body } = card(w, env, { tile: true });
	const fig = figure(true);
	if (w.format === 'temp') fig.el.classList.add('temp');
	const cap = captionLine();
	body.append(fig.el);
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	/** @type {ReturnType<typeof sharpCanvas> | null} */
	let spark = null;
	const hist = new Hist(1, env.historyMs, env.intervalMs);
	if (w.spark) {
		spark = sharpCanvas('spark', (ctx, width, height) => {
			const color = levelColor(spark ? spark.el : el, last?.level, last?.blend, accent(w.color)) || cssColor(el, accent(w.color));
			const max = typeof last?.max === 'number' ? last.max : typeof w.max === 'number' ? w.max : w.format === 'percent' ? 100 : null;
			drawChart(ctx, width, height, {
				lines: [{ t: hist.t, v: hist.v[0], color, gapMs: hist.gapMs }],
				windowMs: hist.windowMs,
				maxValue: max,
				minScale: 1,
				binary: w.format === 'bytes' || w.format === 'rate',
			});
		});
		spark.el.setAttribute('aria-hidden', 'true');
		body.append(spark.el);
	}
	body.append(cap.el);
	return {
		el,
		update(s, data) {
			last = s;
			fig.set(w, s);
			setLevel(fig.el, s?.level, s?.blend);
			cap.set(captionOf(s));
			if (spark) {
				const entry = data.history ? data.history[w.id] : undefined;
				if (entry) hist.take(data.t, [numberOf(s)], entry);
				else if (s) hist.append(data.t, [numberOf(s)]);
				spark.redraw();
			}
			el.setAttribute('aria-description', [F.format(s?.v, w.format, w.unit), captionOf(s)].filter(Boolean).join(', '));
		},
		clear() {
			last = undefined;
			hist.clear();
			fig.set(w, undefined);
			setLevel(fig.el, null);
			cap.set(null);
			spark?.redraw();
		},
		destroy() {
			spark?.destroy();
		},
	};

	/** @param {import('./common.js').WidgetState | undefined} s */
	function captionOf(s) {
		return typeof w.caption === 'string' && w.caption ? w.caption : s && typeof s.caption === 'string' && s.caption ? s.caption : null;
	}
}
