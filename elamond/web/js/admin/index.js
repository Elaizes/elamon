// The admin panel: for the roles the service opens its admin API to (`admin` in `features`).
import { h } from '../dom.js';

/**
 * @param {HTMLElement} root @param {import('../types.js').Ctx} ctx @param {string[]} path what follows `#/admin/`
 * @returns {() => void} what tears it down
 */
export function mountAdmin(root, ctx, path) {
	root.append(h('p', null, 'Admin: to be written.'));
	return () => {};
}
