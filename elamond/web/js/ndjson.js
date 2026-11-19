// Newline-delimited JSON as it comes in pieces: whole lines out, the rest kept for the next piece.

/**
 * Splits `buffered + chunk` into whole lines and what is left of an unfinished one. Empty lines
 * are dropped.
 * @param {string} buffered @param {string} chunk @returns {{ lines: string[], rest: string }}
 */
export function splitLines(buffered, chunk) {
	const all = buffered + chunk;
	const parts = all.split('\n');
	const rest = parts.pop() ?? '';
	return { lines: parts.map((l) => l.trim()).filter((l) => l.length > 0), rest };
}

/**
 * Each JSON object of an NDJSON body, as it arrives. A line that isn't JSON is skipped.
 * @param {ReadableStream<Uint8Array>} body @returns {AsyncGenerator<any>}
 */
export async function* objects(body) {
	const reader = body.getReader();
	const decoder = new TextDecoder();
	let rest = '';
	try {
		for (;;) {
			const { value, done } = await reader.read();
			if (done) break;
			const split = splitLines(rest, decoder.decode(value, { stream: true }));
			rest = split.rest;
			for (const line of split.lines) {
				let obj;
				try {
					obj = JSON.parse(line);
				} catch {
					continue;
				}
				yield obj;
			}
		}
	} finally {
		reader.releaseLock();
	}
}
