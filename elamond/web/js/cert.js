// The certificate's fingerprint (`hello.fingerprint`, SHA-256 as 64 lowercase hex characters) as
// people compare it: in pairs with colons, as browsers show a certificate's, and in groups of four
// of its first 32 characters, as `elamond pair` prints it. Pure.

/** Characters of the fingerprint `elamond pair` prints, and how many to a group. */
const PRINTED = 32;
const GROUP = 4;

/** Whether `fp` is a fingerprint: 64 hex characters. @param {unknown} fp @returns {fp is string} */
export function isFingerprint(fp) {
	return typeof fp === 'string' && /^[0-9a-fA-F]{64}$/.test(fp);
}

/** "1F:0E:8D:…", all of it. @param {string} fp */
export function colonFingerprint(fp) {
	return (fp.toUpperCase().match(/../g) || []).join(':');
}

/** "1F0E 8D2C …", its first 32 characters, as `elamond pair` prints them. @param {string} fp */
export function printedFingerprint(fp) {
	return (fp.slice(0, PRINTED).toUpperCase().match(new RegExp(`.{1,${GROUP}}`, 'g')) || []).join(' ');
}
