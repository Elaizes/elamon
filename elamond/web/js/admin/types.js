// The shapes of the admin API the panel reads (docs/protocol.md, "Admin API"), for `tsc --checkJs`.
// Nothing runs here.

/**
 * @typedef {{ name: string, kind: 'main' | 'own' | 'dropin' | string, writable: boolean, why: string | null, hash: string,
 *   size: number, modified: number }} StateFile
 *
 * @typedef {{ enabled: boolean, off_reason: string | null, held_off_since?: number | null }} ControlState
 *
 * @typedef {{ name: string, ok: boolean, until: number | null, message: string | null }} Certificate
 *
 * @typedef {{
 *   service: { id: string, name: string, version: string, started: number, features: string[], fingerprint: string,
 *     listening: string[], endpoints: string[] },
 *   control: ControlState,
 *   web: { page: boolean, admin: boolean, write: boolean, editor: boolean, role: string, editor_file: string, certificate: Certificate | null },
 *   settings: { sound: boolean, message: string | null, generation: number },
 *   setup: { ok: boolean, text: string }[],
 *   files: StateFile[],
 *   you: { id: string, role: string }
 * }} AdminState
 *
 * @typedef {{ files: string[], settings: Record<string, unknown>, whence: Record<string, [number, number]>,
 *   changes: { kind: 'added' | 'changed' | 'removed' | string, path: string, file?: string | null, line?: number | null }[],
 *   warnings: string[] }} AdminSettings
 *
 * @typedef {{ id: string, name: string, role: string, profile: string | null, own_profile: string | null, kind: 'web' | 'android' | string,
 *   app: string | null, address: string | null, seen: number | null, created: number | null, from: string | null, added: string[], you: boolean }} Client
 *
 * @typedef {{ clients: Client[], roles: string[], can_give: string[], profiles: string[] }} AdminClients
 *
 * @typedef {{ size: number, modules: string }} Qr
 *
 * @typedef {{ id: string, name: string, address: string | null, at: number | null, replaces: any, offer: any }} PairAsk
 *
 * @typedef {{ code: string | null, expires?: number, role?: string, profile?: string | null, from_page?: boolean,
 *   app?: { link: string, qr: Qr }, web?: { link: string, qr: Qr }, ask?: PairAsk | null }} Pending
 *
 * @typedef {{ file: string, line: number | null, path?: string | null, message: string }} Mistake
 *
 * @typedef {{ id: string, at: number, size: number }} Backup
 *
 * @typedef {{ name: string, kind: string, text: string, hash: string, writable: boolean, why: string | null,
 *   note: 'phones' | 'editor' | null | string, backups: Backup[] }} AdminFile
 *
 * @typedef {{ sound: boolean, mistakes: Mistake[], warnings: string[], diff: string[] }} CheckAnswer
 *
 * @typedef {{ ok: boolean, hash: string, backup: string | null, generation: number }} SaveAnswer
 *
 * What a section of the panel is handed: the page's ctx; a signal that ends with the section;
 * asking the admin API (GET without a body, POST with one), by the path after `/api/v1/admin/`;
 * whether a failure is one to say nothing of (given up on, or the browser unpaired, which `call`
 * has taken back to pairing already); saying a failure in a note at the bottom; and keeping a
 * dialog to close with the section.
 * @typedef {{ ctx: import('../types.js').Ctx, signal: AbortSignal, call: (path: string, body?: unknown) => Promise<any>,
 *   quiet: (e: unknown) => boolean, fail: (e: unknown) => void, keep: (dialog: HTMLDialogElement) => void }} Env
 */

export {};
