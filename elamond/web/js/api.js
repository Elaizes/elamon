// Talking to the service that serves the page: same origin, the cookie it set at pairing (never
// seen here), and `Elamon-Client: web/<version>`.
import { VERSION } from './version.js';

export const CLIENT = `web/${VERSION}`;

/** An answer that isn't 2xx: its status, the service's `error` code and `message`, and the body. */
export class ApiError extends Error {
	/** @param {number} status @param {string} code @param {string | null} message @param {any} body */
	constructor(status, code, message, body) {
		super(message ? `${code}: ${message}` : code);
		this.status = status;
		this.code = code;
		this.detail = message;
		this.body = body;
	}
}

/**
 * A request to the API. A body is sent as JSON. Resolves with the answer's JSON (or null for an
 * empty one); rejects with an ApiError, or a TypeError when the computer can't be reached.
 * @param {string} path
 * @param {{ method?: string, body?: unknown, signal?: AbortSignal }} [opts]
 * @returns {Promise<any>}
 */
export async function api(path, { method, body, signal } = {}) {
	const headers = { 'Elamon-Client': CLIENT, Accept: 'application/json' };
	/** @type {RequestInit} */
	const init = { method: method || (body === undefined ? 'GET' : 'POST'), headers, credentials: 'same-origin', cache: 'no-store', signal };
	if (body !== undefined) {
		headers['Content-Type'] = 'application/json';
		init.body = JSON.stringify(body);
	}
	const res = await fetch(path, init);
	const text = await res.text();
	/** @type {any} */
	let json = null;
	if (text) {
		try {
			json = JSON.parse(text);
		} catch {
			json = null;
		}
	}
	if (!res.ok) {
		const code = json && typeof json.error === 'string' ? json.error : `http_${res.status}`;
		const message = json && typeof json.message === 'string' ? json.message : json ? null : text.slice(0, 200) || null;
		throw new ApiError(res.status, code, message, json);
	}
	return json;
}

/**
 * Opens a request whose answer is read as it comes (the stream). Same headers as `api`.
 * @param {string} path @param {AbortSignal} signal @returns {Promise<Response>}
 */
export async function open(path, signal) {
	const res = await fetch(path, { headers: { 'Elamon-Client': CLIENT, Accept: 'application/x-ndjson' }, credentials: 'same-origin', cache: 'no-store', signal });
	if (!res.ok) {
		const text = await res.text().catch(() => '');
		/** @type {any} */
		let json = null;
		try {
			json = JSON.parse(text);
		} catch {}
		throw new ApiError(res.status, json && json.error ? json.error : `http_${res.status}`, json && json.message ? json.message : null, json);
	}
	return res;
}
