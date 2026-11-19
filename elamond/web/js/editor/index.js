// The dashboard editor: forms the service describes (`GET /api/v1/admin/editor`), a live preview
// drawn with panels.js, and writes the service makes (`editor` in `features`).
import { h } from '../dom.js';

/**
 * @param {HTMLElement} root @param {import('../types.js').Ctx} ctx @param {string | null} dashboard the one to edit, none for a new one
 * @returns {() => void} what tears it down
 */
export function mountEditor(root, ctx, dashboard) {
	root.append(h('p', null, 'Editor: to be written.'));
	return () => {};
}
