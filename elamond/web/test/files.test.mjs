// The page's file names as content blockers see them: a name their lists stop (EasyPrivacy stops
// any `/fingerprint.js`) keeps the browser from running any of the page's modules.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const WEB = fileURLToPath(new URL('..', import.meta.url));

/** Words the common lists stop files by, as a whole part of a name. */
const STOPPED = ['ad', 'ads', 'adv', 'advert', 'adserver', 'banner', 'beacon', 'collect', 'fingerprint', 'fp',
	'analytics', 'metrics', 'pixel', 'popup', 'sponsor', 'stats', 'telemetry', 'track', 'tracker', 'tracking'];

test("no file the page loads has a name content blockers stop", () => {
	const files = readdirSync(WEB, { recursive: true, encoding: 'utf8' })
		.filter((f) => !f.startsWith('test') && /\.(js|css|svg|html|webmanifest)$/.test(f));
	assert.ok(files.includes('index.html') && files.length > 50, `found ${files.length} files`);
	for (const f of files) {
		const parts = f.toLowerCase().split(/[/\\._-]/);
		const hit = parts.filter((p) => STOPPED.includes(p) || p.startsWith('fingerprint'));
		assert.deepEqual(hit, [], `${f} has a name content blockers stop`);
	}
});
