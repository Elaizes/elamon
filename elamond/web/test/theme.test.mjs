// Which theme the page shows: this browser's choice over the profile's, the system's for `system`,
// pure black making a dark theme black; what 0.11.2 kept (`auto`) is no choice.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { themeOf, isTheme, THEMES } from '../js/theme.js';

test('a choice is shown whatever the profile and the system say', () => {
	for (const c of THEMES) {
		assert.equal(themeOf(c, { theme: 'light', pure_black: false }, true), c);
		assert.equal(themeOf(c, { theme: 'dark', pure_black: true }, false), c);
		assert.equal(themeOf(c, null, false), c);
	}
});

test('without a choice: the profile’s theme, the system’s for system, black for pure black when dark', () => {
	assert.equal(themeOf(null, { theme: 'light', pure_black: true }, true), 'light');
	assert.equal(themeOf(null, { theme: 'dark', pure_black: false }, false), 'dark');
	assert.equal(themeOf(null, { theme: 'dark', pure_black: true }, false), 'black');
	assert.equal(themeOf(null, { theme: 'system', pure_black: false }, true), 'dark');
	assert.equal(themeOf(null, { theme: 'system', pure_black: true }, true), 'black');
	assert.equal(themeOf(null, { theme: 'system', pure_black: true }, false), 'light');
	assert.equal(themeOf(null, null, true), 'dark');
	assert.equal(themeOf(null, undefined, false), 'light');
});

test('only light, dark and black are choices', () => {
	for (const v of ['light', 'dark', 'black']) assert.ok(isTheme(v));
	for (const v of ['auto', 'system', '', null, undefined, 1]) assert.ok(!isTheme(v));
});
