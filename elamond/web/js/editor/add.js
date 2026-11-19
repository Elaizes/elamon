// "Add widget": the types the service describes, each with its label, its example's keys and a
// small drawing of the example (a preview of a dashboard of examples only, never written).
import { api } from '../api.js';
import { h, clear } from '../dom.js';
import { S, fill } from '../strings.js';
import { dashboardView, DRAWS } from '../panels.js';
import { exampleOf, search } from './describe.js';

/**
 * @param {{ desc: import('./describe.js').Editor, ui: import('../types.js').Ui | null, editor: () => Record<string, unknown>,
 *   into: string | null, pick: (type: string) => void }} opts
 * `editor` is the preview body's `editor` with the draft; `into` the group it adds into, by name
 * @returns {() => void} what closes it
 */
export function addDialog(opts) {
	const { desc } = opts;
	const title = h('h2', { id: 'ed-add-title' }, opts.into ? fill(S.editor.addIntoTitle, opts.into) : S.editor.addTitle);
	const find = h('input', { type: 'search', placeholder: S.editor.findType, 'aria-label': S.editor.findType });
	const list = h('ul', { class: 'ed-types', role: 'list' });
	const dialog = h(
		'dialog',
		{ class: 'ed-add', 'aria-labelledby': 'ed-add-title' },
		h('div', { class: 'ed-add-head' }, title, h('button', { type: 'button', class: 'quiet', onclick: () => dialog.close() }, S.core.close)),
		find,
		list,
	);
	/** @type {import('../types.js').DashboardView[]} */
	const views = [];
	/** @type {Map<string, HTMLElement>} */
	const minis = new Map();
	const ex = exampleId(desc);
	/** @param {string} t */
	const widgetId = (t) => t.toLowerCase().replace(/[^a-z0-9_-]/g, '-').slice(0, 32) || 'x';

	const draw = () => {
		clear(list);
		minis.clear();
		const opts2 = desc.types.map((t) => ({ value: t.type, label: t.label || t.type, detail: '' }));
		const shown = new Set(search(opts2, find.value, 1000).map((o) => o.value));
		for (const t of desc.types) {
			if (!shown.has(t.type)) continue;
			const example = exampleOf(desc, t.type);
			const keys = Object.entries(example)
				.filter(([k]) => k !== 'type')
				.map(([k, v]) => `${k} = ${JSON.stringify(v)}`);
			const mini = h('div', { class: 'ed-mini', inert: true, 'aria-hidden': 'true' });
			minis.set(t.type, mini);
			list.append(
				h(
					'li',
					null,
					h(
						'button',
						{ type: 'button', class: 'ed-type', onclick: () => { dialog.close(); opts.pick(t.type); } },
						h('span', { class: 'ed-type-name' }, t.label || t.type, ' ', h('small', { class: 'muted mono' }, t.type)),
						mini,
						keys.length ? h('span', { class: 'ed-type-keys mono' }, keys.join('\n')) : null,
					),
				),
			);
		}
		if (!list.children.length) list.append(h('li', { class: 'muted' }, S.editor.noTypes));
		paint();
	};

	/** @type {{ ui: import('../types.js').Ui, data: any } | null} */
	let drawn = null;
	const paint = () => {
		for (const v of views.splice(0)) v.destroy();
		if (!drawn) return;
		const d = drawn.ui.dashboards.find((x) => x.id === ex);
		if (!d) return;
		for (const [type, mini] of minis) {
			const w = (d.widgets || []).filter((x) => x.id === `${ex}/${widgetId(type)}` || x.id.startsWith(`${ex}/${widgetId(type)}.`));
			if (!w.length) continue;
			try {
				const ui = { ...drawn.ui, tabs: [ex], header: null, dashboards: drawn.ui.dashboards.map((x) => (x.id === ex ? { ...x, widgets: w.map((y) => ({ ...y, span: 4 })) } : x)) };
				const view = dashboardView(ui, ex, { preview: true });
				if (drawn.data) view.update(drawn.data);
				views.push(view);
				clear(mini);
				mini.append(view.el);
			} catch {}
		}
	};

	find.addEventListener('input', draw);
	dialog.addEventListener('close', () => {
		for (const v of views.splice(0)) v.destroy();
		dialog.remove();
	});
	document.body.append(dialog);
	draw();
	dialog.showModal();
	find.focus();

	// The examples, drawn: one preview of a dashboard holding them all. Without it, their keys say enough.
	const editor = opts.editor();
	const dashboards = { .../** @type {Record<string, unknown>} */ (editor.dashboards || {}) };
	dashboards[ex] = { title: S.editor.addTitle, widgets: desc.types.map((t) => ({ ...exampleOf(desc, t.type), id: widgetId(t.type) })) };
	api('/api/v1/admin/preview', { body: { editor: { ...editor, dashboards }, dashboard: ex, widgets: DRAWS } })
		.then((answer) => {
			if (!answer || !answer.ui || !dialog.isConnected) return;
			const own = opts.ui;
			drawn = { ui: { ...(own || {}), ...answer.ui, service: answer.ui.service || own?.service, profile: answer.ui.profile || own?.profile }, data: answer.data };
			paint();
		})
		.catch(() => {});
	return () => dialog.close();
}

/** An id for the dashboard of examples no dashboard has. @param {import('./describe.js').Editor} desc */
function exampleId(desc) {
	let id = 'editor-examples';
	for (let n = 2; desc.dashboards.some((d) => d.id === id); n++) id = `editor-examples-${n}`;
	return id;
}
