import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dropinName, isDropin, dropinStem, fileHref, fileRoute } from '../js/admin/names.js';

test('a drop-in from what is typed', () => {
	assert.deepEqual(dropinName('mine'), { name: 'elamond.d/mine.toml' });
	assert.deepEqual(dropinName(' mine.toml '), { name: 'elamond.d/mine.toml' });
	assert.deepEqual(dropinName('elamond.d/mine.toml'), { name: 'elamond.d/mine.toml' });
	assert.deepEqual(dropinName('10-ups_eco.v2'), { name: 'elamond.d/10-ups_eco.v2.toml' });
	assert.deepEqual(dropinName('x'.repeat(64)), { name: `elamond.d/${'x'.repeat(64)}.toml` });
});

test('names the protocol refuses', () => {
	assert.deepEqual(dropinName(''), { error: 'empty' });
	assert.deepEqual(dropinName('   '), { error: 'empty' });
	assert.deepEqual(dropinName('.toml'), { error: 'empty' });
	assert.deepEqual(dropinName('elamond.d/'), { error: 'empty' });
	assert.deepEqual(dropinName(null), { error: 'empty' });
	assert.deepEqual(dropinName('x'.repeat(65)), { error: 'long' });
	assert.deepEqual(dropinName('-mine'), { error: 'start' });
	assert.deepEqual(dropinName('.hidden'), { error: 'start' });
	assert.deepEqual(dropinName('_x'), { error: 'start' });
	assert.deepEqual(dropinName('my file'), { error: 'chars' });
	assert.deepEqual(dropinName('sub/mine'), { error: 'chars' });
	assert.deepEqual(dropinName('../mine'), { error: 'start' });
	assert.deepEqual(dropinName('mïne'), { error: 'chars' });
	assert.deepEqual(dropinName('elamond.toml/x'), { error: 'chars' });
});

test('which names are drop-ins', () => {
	assert.equal(isDropin('elamond.d/mine.toml'), true);
	assert.equal(isDropin('elamond.d/zz-editor.toml'), true);
	assert.equal(isDropin('elamond.toml'), false);
	assert.equal(isDropin('mine.toml'), false);
	assert.equal(isDropin('elamond.d/-x.toml'), false);
	assert.equal(isDropin('elamond.d/a b.toml'), false);
	assert.equal(dropinStem('elamond.d/phones.toml'), 'phones');
});

test("a file's address, there and back", () => {
	assert.equal(fileHref('elamond.d/mine.toml'), '#/admin/files/elamond.d%2Fmine.toml');
	assert.equal(fileHref('elamond.d/mine.toml', 12), '#/admin/files/elamond.d%2Fmine.toml/12');
	assert.equal(fileHref('elamond.toml', 0), '#/admin/files/elamond.toml');
	// main.js splits the address at "/" and decodes each part.
	const parts = (/** @type {string} */ href) => href.replace(/^#\/?/, '').split('/').filter(Boolean).map(decodeURIComponent).slice(2);
	assert.deepEqual(fileRoute(parts(fileHref('elamond.d/mine.toml', 12))), { name: 'elamond.d/mine.toml', line: 12 });
	assert.deepEqual(fileRoute(parts(fileHref('elamond.toml'))), { name: 'elamond.toml', line: null });
	// Typed by hand, the name in several parts.
	assert.deepEqual(fileRoute(['elamond.d', 'mine.toml', '7']), { name: 'elamond.d/mine.toml', line: 7 });
	assert.deepEqual(fileRoute(['elamond.d', 'mine.toml']), { name: 'elamond.d/mine.toml', line: null });
	assert.deepEqual(fileRoute([]), { name: null, line: null });
	assert.deepEqual(fileRoute(['12']), { name: '12', line: null });
});
