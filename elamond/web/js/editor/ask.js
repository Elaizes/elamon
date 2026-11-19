// A question before something that can't be undone here: leaving with changes, going back to the
// files, deleting a dashboard.
import { h } from '../dom.js';
import { S } from '../strings.js';

let seq = 0;

/**
 * @param {string} title @param {string} text @param {string} go what the button that goes on says
 * @param {boolean} [danger] @returns {Promise<boolean>}
 */
export function ask(title, text, go, danger = false) {
	return new Promise((done) => {
		const id = `ed-ask-${++seq}`;
		const dialog = h('dialog', { class: 'ed-ask', 'aria-labelledby': id });
		const cancel = h('button', { type: 'button', onclick: () => dialog.close('cancel') }, S.core.cancel);
		const ok = h('button', { type: 'button', class: danger ? 'danger' : 'primary', onclick: () => dialog.close('go') }, go);
		dialog.append(h('h2', { id }, title), h('p', null, text), h('div', { class: 'actions' }, cancel, ok));
		dialog.addEventListener('close', () => {
			dialog.remove();
			done(dialog.returnValue === 'go');
		});
		document.body.append(dialog);
		dialog.showModal();
		cancel.focus();
	});
}
