// The settings files: the list and a new drop-in; an editor with line numbers and going to a line,
// checked as you type (the mistakes, the difference from the disk), saved with the hash it was read
// at (a file changed on disk is said, never written over silently); backups, renaming and deleting
// drop-ins; a preview of the drafts. File text goes in as text only: a textarea's value, a <pre>'s.
import { h, clear, append } from '../dom.js';
import { S, fill } from '../strings.js';
import { loadInto, head, button, card, chip, table, diffView, dialog, confirmed, debounce, failure, loading } from './ui.js';
import { errorSentence, isRefusal, mistakesOf } from './errors.js';
import { dropinName, dropinStem, isDropin, fileHref, fileRoute, DROPIN_DIR, SUFFIX } from './names.js';
import { lineCount, offsetOfLine, endOfLine, lineAt, sizeText } from './text.js';
import { dateTime } from './time.js';
import { filesCard } from './overview.js';
import { openPreview, mistakesList } from './preview.js';
import { CHECK_DEBOUNCE_MS } from './consts.js';

/** @typedef {import('./types.js').Env} Env */
/** @typedef {import('./types.js').AdminFile} AdminFile */
/** @typedef {import('./types.js').AdminState} AdminState */
/** @typedef {import('./types.js').Mistake} Mistake */

/**
 * Unsaved drafts by file name, kept while the page is open so going elsewhere in the panel loses
 * nothing; never in the browser's storage. `base` is the hash the draft was made on, null for a
 * file not yet written.
 * @type {Map<string, { text: string, base: string | null }>}
 */
const drafts = new Map();

let guarded = false;

/** Leaving the page with drafts unsaved asks first. */
function guardUnload() {
	if (guarded) return;
	guarded = true;
	window.addEventListener('beforeunload', (ev) => {
		if (!drafts.size) return;
		ev.preventDefault();
		ev.returnValue = '';
	});
}

/** @type {import('./index.js').Mount} */
export function mountFiles(main, env, rest) {
	const { name, line } = fileRoute(rest);
	if (!name) return mountList(main, env);
	return mountEditor(main, env, name, line);
}

/** When writing the settings is off, why. @param {AdminState} st */
function writesNote(st) {
	const T = S.admin.files;
	if (st.web && !st.web.write) return h('p', { class: 'note warning' }, T.writesOff);
	if (st.control && !st.control.enabled && st.control.off_reason === 'held_off') return h('p', { class: 'note warning' }, T.writesHeld);
	if (st.control && !st.control.enabled && st.control.off_reason === 'settings') return h('p', { class: 'note warning' }, T.writesNoControl);
	return null;
}

// ---------------------------------------------------------------- the list

/** @param {HTMLElement} main @param {Env} env */
function mountList(main, env) {
	const T = S.admin.files;
	loadInto(main, env, () => env.call('state'), (value, reload) => {
		const st = /** @type {AdminState} */ (value);
		const names = new Set((st.files || []).map((f) => f.name));
		const unsaved = [...drafts.keys()];
		append(
			main,
			head(S.admin.nav.files, button(S.admin.refresh, reload)),
			h('p', { class: 'muted' }, T.intro),
			writesNote(st),
			unsaved.length
				? h('div', { class: 'note warning' }, h('p', null, T.unsavedList), h('ul', { class: 'adm-plain-list' }, ...unsaved.map((n) => h('li', null, h('a', { class: 'mono', href: fileHref(n) }, n)))))
				: null,
			filesCard(st),
			newDropin(env, names),
		);
	});
	return () => {};
}

/** The form for a new drop-in. @param {Env} env @param {Set<string>} names the files there are */
function newDropin(env, names) {
	const T = S.admin.files;
	const input = h('input', { id: 'adm-new-name', class: 'mono', placeholder: T.namePlaceholder, autocomplete: 'off', spellcheck: 'false', autocapitalize: 'off' });
	const why = h('p', { class: 'adm-field-why', 'aria-live': 'polite' });
	const create = h('button', { type: 'button', class: 'primary', disabled: true }, T.create);
	const check = () => {
		create.disabled = true;
		why.classList.remove('bad');
		if (!input.value.trim()) {
			why.textContent = '';
			return null;
		}
		const r = dropinName(input.value);
		if (r.error) {
			why.textContent = /** @type {Record<string, string>} */ (T.nameErrors)[r.error];
			why.classList.add('bad');
			return null;
		}
		if (names.has(r.name) || drafts.has(r.name)) {
			why.textContent = fill(T.nameTaken, r.name);
			why.classList.add('bad');
			return null;
		}
		why.textContent = fill(T.willBe, r.name);
		create.disabled = false;
		return r.name;
	};
	const go = () => {
		const name = check();
		if (!name) return;
		drafts.set(name, { text: '', base: null });
		guardUnload();
		env.ctx.go(fileHref(name));
	};
	input.addEventListener('input', check);
	input.addEventListener('keydown', (ev) => {
		if (ev.key === 'Enter') go();
	});
	create.addEventListener('click', go);
	return card(
		T.newDropin,
		h('p', { class: 'muted' }, T.newIntro),
		h('label', { for: 'adm-new-name' }, T.newName),
		h('div', { class: 'adm-name-row' }, h('span', { class: 'mono muted' }, DROPIN_DIR), input, h('span', { class: 'mono muted' }, SUFFIX)),
		why,
		h('div', { class: 'adm-row' }, create),
	);
}

// ---------------------------------------------------------------- the editor

/** @param {HTMLElement} main @param {Env} env @param {string} name @param {number | null} startLine */
function mountEditor(main, env, name, startLine) {
	const T = S.admin.files;
	const kinds = /** @type {Record<string, string>} */ (S.admin.kinds);
	/** @type {AdminFile | null} */
	let file = null;
	/** The text on disk, as last read. */
	let disk = '';
	/** The hash the text in the editor was made on: what a write names as its `base`. */
	/** @type {string | null} */
	let base = null;
	let isNew = false;
	let readOnly = true;
	let loaded = false;
	let destroyed = false;
	/** @type {Mistake[]} */
	let mistakes = [];
	/** @type {string[]} */
	let warnings = [];
	/** @type {string[]} */
	let diff = [];
	let checkSeq = 0;
	let gutterCount = 0;
	let curLine = 0;
	let closePreview = () => {};
	const later = debounce(() => runCheck(), CHECK_DEBOUNCE_MS);

	const ta = h('textarea', { class: 'adm-ta mono', wrap: 'off', spellcheck: 'false', autocapitalize: 'off', autocomplete: 'off', 'aria-label': fill(T.textOf, name), readonly: true });
	const gutter = h('div', { class: 'adm-gutter mono', 'aria-hidden': 'true' });
	const kindChips = h('span', { class: 'adm-chips' });
	const writesBox = h('div', { class: 'adm-ed-notes' });
	const infoNotes = h('div', { class: 'adm-ed-notes' });
	const conflictBox = h('div', { class: 'adm-ed-notes' });
	const stateLine = h('span');
	const checkLine = h('span', { class: 'adm-check-line' });
	const status = h('p', { class: 'adm-ed-status', role: 'status' }, stateLine, ' ', checkLine);
	const mistakesBox = h('div');
	const diffBox = h('div');
	const backupsBox = h('div');
	const side = h('nav', { class: 'adm-ed-side', 'aria-label': T.allFiles });
	const lineInput = h('input', { type: 'number', min: '1', inputmode: 'numeric', class: 'adm-line-input', 'aria-label': T.goLine, placeholder: T.line });

	const saveBtn = h('button', { type: 'button', class: 'primary', title: T.saveKeys }, T.save);
	const checkBtn = h('button', { type: 'button' }, T.check);
	const previewBtn = h('button', { type: 'button' }, T.preview);
	const goBtn = h('button', { type: 'button' }, T.go);
	const undoBtn = h('button', { type: 'button', class: 'quiet' }, T.undo);
	const renameBtn = h('button', { type: 'button', class: 'quiet' }, T.rename);
	const deleteBtn = h('button', { type: 'button', class: 'quiet adm-danger-text' }, T.delete);

	main.append(
		h(
			'div',
			{ class: 'adm-editor' },
			h('div', { class: 'adm-ed-head' }, h('a', { class: 'button quiet', href: '#/admin/files' }, fill(S.admin.marks.back, T.allFiles)), h('h2', { class: 'mono adm-ed-name' }, name), kindChips),
			writesBox,
			infoNotes,
			conflictBox,
			h('div', { class: 'adm-ed-bar' }, saveBtn, checkBtn, previewBtn, h('span', { class: 'adm-ed-go' }, lineInput, goBtn), undoBtn, renameBtn, deleteBtn),
			status,
			h(
				'div',
				{ class: 'adm-ed-body' },
				h(
					'div',
					{ class: 'adm-ed-main' },
					h('div', { class: 'adm-code' }, gutter, ta),
					card(T.mistakesTitle, mistakesBox),
					card(T.diffTitle, diffBox),
					card(T.backupsTitle, backupsBox),
				),
				side,
			),
		),
	);

	// ------------------------------------------------ the text and its lines
	const dirty = () => isNew || ta.value !== disk;

	/** Keeps the draft while it differs from the disk. */
	const remember = () => {
		if (readOnly || !loaded) return;
		if (!isNew && ta.value === disk) {
			drafts.delete(name);
			if (file) base = file.hash;
			clear(conflictBox);
		} else {
			drafts.set(name, { text: ta.value, base });
			guardUnload();
		}
	};

	/** The editor's text, the cursor at its start. @param {string} text */
	const setText = (text) => {
		ta.value = text;
		ta.setSelectionRange(0, 0);
		ta.scrollTop = 0;
		drawGutter();
	};

	function drawGutter() {
		const n = lineCount(ta.value);
		if (n !== gutterCount) {
			if (n > gutterCount) {
				const frag = document.createDocumentFragment();
				for (let i = gutterCount + 1; i <= n; i++) frag.append(h('span', null, i));
				gutter.append(frag);
			} else while (gutter.childElementCount > n) gutter.lastElementChild?.remove();
			gutterCount = n;
			markGutter();
		}
		markCur();
		syncScroll();
	}

	function markGutter() {
		const bad = new Set(mistakes.filter((m) => m.file === name && m.line).map((m) => m.line));
		const spans = gutter.children;
		for (let i = 0; i < spans.length; i++) spans[i].classList.toggle('bad', bad.has(i + 1));
	}

	function markCur() {
		const l = lineAt(ta.value, ta.selectionStart);
		if (l === curLine && gutter.children[l - 1]?.classList.contains('cur')) return;
		for (const s of gutter.querySelectorAll('.cur')) s.classList.remove('cur');
		gutter.children[l - 1]?.classList.add('cur');
		curLine = l;
	}

	function syncScroll() {
		gutter.scrollTop = ta.scrollTop;
	}

	/** @param {number} n */
	function goToLine(n) {
		const text = ta.value;
		const l = Math.min(Math.max(1, Math.floor(n) || 1), lineCount(text));
		ta.focus({ preventScroll: true });
		ta.setSelectionRange(offsetOfLine(text, l), endOfLine(text, l));
		const lh = parseFloat(getComputedStyle(ta).lineHeight) || 20;
		ta.scrollTop = Math.max(0, (l - 1) * lh - ta.clientHeight / 3);
		ta.scrollLeft = 0;
		syncScroll();
		markCur();
		ta.scrollIntoView({ block: 'nearest' });
	}

	/** A mistake's place: here, or another file at its line. @param {string} f @param {number | null} line */
	function openMistake(f, line) {
		if (f === name) {
			if (line) goToLine(line);
		} else env.ctx.go(fileHref(f, line));
	}

	// ------------------------------------------------ what is drawn around it
	function updateState() {
		const d = dirty();
		const dropin = isDropin(name) && !isNew;
		saveBtn.disabled = !loaded || readOnly || !d;
		saveBtn.hidden = readOnly;
		checkBtn.hidden = readOnly;
		checkBtn.disabled = !loaded;
		previewBtn.disabled = !loaded;
		undoBtn.hidden = readOnly;
		undoBtn.disabled = !d;
		undoBtn.textContent = isNew ? T.discard : T.undo;
		renameBtn.hidden = readOnly || !dropin;
		deleteBtn.hidden = readOnly || !dropin;
		renameBtn.disabled = d;
		deleteBtn.disabled = d;
		renameBtn.title = d ? T.saveFirst : '';
		deleteBtn.title = d ? T.saveFirst : '';
		ta.readOnly = readOnly || !loaded;
		stateLine.textContent = !loaded ? '' : readOnly ? T.stateReadOnly : isNew ? T.stateNew : d ? T.stateDirty : T.stateClean;
		stateLine.className = d && !readOnly ? 'adm-dirty' : 'muted';
	}

	function drawHead() {
		clear(kindChips);
		if (isNew) kindChips.append(chip(T.chipNew, 'good'));
		else if (file) append(kindChips, chip(kinds[file.kind] || file.kind), readOnly ? chip(T.readOnly, 'off') : null, file.note === 'phones' ? chip(T.chipPhones) : null, file.note === 'editor' ? chip(T.chipEditor) : null);
	}

	function drawNotes() {
		clear(infoNotes);
		if (isNew) infoNotes.append(h('p', { class: 'note' }, T.newNote));
		if (readOnly && file) infoNotes.append(h('p', { class: 'note warning' }, file.why ? fill(T.readOnlyWhy, file.why) : T.readOnlyNoWhy));
		if (file?.note === 'phones') infoNotes.append(h('p', { class: 'note warning' }, T.notePhones));
		if (file?.note === 'editor') infoNotes.append(h('p', { class: 'note warning' }, T.noteEditor));
	}

	/** @param {string} text @param {'good' | 'bad' | ''} [level] */
	function setCheckLine(text, level = '') {
		checkLine.textContent = text;
		checkLine.className = `adm-check-line${level ? ` ${level}` : ''}`;
	}

	/** @param {boolean | null} sound null: not checked */
	function drawChecked(sound) {
		clear(mistakesBox);
		if (sound === null) mistakesBox.append(h('p', { class: 'muted' }, T.notChecked));
		else if (!mistakes.length) mistakesBox.append(h('p', { class: 'muted' }, T.noMistakes));
		else mistakesBox.append(mistakesList(mistakes, openMistake));
		if (warnings.length) mistakesBox.append(h('ul', { class: 'adm-warnings' }, ...warnings.map((w) => h('li', { class: 'note warning' }, w))));
		clear(diffBox);
		diffBox.append(diff.length ? diffView(diff, goToLine) : h('p', { class: 'muted' }, sound === null ? T.diffNotYet : T.noDiff));
		markGutter();
	}

	function drawBackups() {
		clear(backupsBox);
		const list = file?.backups || [];
		if (!list.length) {
			backupsBox.append(h('p', { class: 'muted' }, isNew ? T.noBackupsNew : T.noBackups));
			return;
		}
		backupsBox.append(
			table(
				[T.backupAt, T.size, ''],
				list.map((b) => [dateTime(b.at, true), sizeText(b.size), h('span', { class: 'adm-cell-row' }, button(T.view, () => viewBackup(b)), readOnly ? null : button(T.restore, () => restore(b)))]),
				'adm-backups',
			),
		);
	}

	/** The other files, to go to; drafts marked. */
	async function drawSide() {
		try {
			const st = /** @type {AdminState} */ (await env.call('state'));
			if (destroyed) return;
			clear(side);
			const note = writesNote(st);
			side.append(
				h('h3', null, T.allFiles),
				h(
					'ul',
					{ class: 'adm-side-list' },
					...(st.files || []).map((f) =>
						h(
							'li',
							null,
							h('a', { class: 'mono', href: fileHref(f.name), 'aria-current': f.name === name ? 'page' : null }, f.name, drafts.has(f.name) ? h('span', { class: 'adm-dot', title: T.stateDirty }, S.admin.marks.dirty) : null),
						),
					),
					isNew ? h('li', null, h('a', { class: 'mono', href: fileHref(name), 'aria-current': 'page' }, name, h('span', { class: 'adm-dot' }, S.admin.marks.dirty))) : null,
				),
				h('a', { class: 'adm-side-new', href: '#/admin/files' }, T.newDropin),
			);
			clear(writesBox);
			if (note) writesBox.append(note);
		} catch (e) {
			// The list beside is a convenience; the editor works without it.
		}
	}

	// ------------------------------------------------ reading
	/** @param {number | null} [goTo] */
	async function load(goTo) {
		loaded = false;
		updateState();
		setCheckLine(S.core.loading);
		/** @type {AdminFile} */
		let f;
		try {
			f = await env.call(`file?name=${encodeURIComponent(name)}`);
		} catch (e) {
			if (env.quiet(e)) return;
			setCheckLine('');
			if (isRefusal(e) && e.code === 'not_found') {
				const d = drafts.get(name);
				if (d && d.base === null) return startNew(goTo);
				return showMissing();
			}
			clear(infoNotes);
			infoNotes.append(failure(e, () => load(goTo)));
			return;
		}
		if (destroyed) return;
		file = f;
		isNew = false;
		disk = typeof f.text === 'string' ? f.text : '';
		base = f.hash;
		readOnly = !f.writable;
		clear(conflictBox);
		const d = drafts.get(name);
		let text = disk;
		let back = false;
		if (d && !readOnly && d.text !== disk) {
			text = d.text;
			if (d.base === f.hash) back = true;
			else {
				// Made on another version of the file (or as a new file, and one of its name is there now).
				base = d.base;
				showConflict(d.base === null ? T.existsNow : T.draftOld);
			}
		} else drafts.delete(name);
		loaded = true;
		setText(text);
		drawHead();
		drawNotes();
		if (back) infoNotes.append(h('p', { class: 'note' }, T.draftBack));
		drawBackups();
		mistakes = [];
		warnings = [];
		diff = [];
		drawChecked(null);
		setCheckLine('');
		updateState();
		if (goTo) goToLine(goTo);
		if (text !== disk) runCheck();
	}

	/** @param {number | null} [goTo] */
	function startNew(goTo) {
		const d = drafts.get(name);
		file = null;
		isNew = true;
		disk = '';
		base = null;
		readOnly = false;
		loaded = true;
		setText(d ? d.text : '');
		drawHead();
		drawNotes();
		drawBackups();
		drawChecked(null);
		setCheckLine('');
		updateState();
		drawSide();
		if (goTo) goToLine(goTo);
		else ta.focus();
	}

	function showMissing() {
		clear(infoNotes);
		const make = isDropin(name)
			? button(T.createIt, () => {
					drafts.set(name, { text: '', base: null });
					guardUnload();
					clear(infoNotes);
					startNew(null);
				}, 'primary')
			: null;
		infoNotes.append(h('div', { class: 'note warning' }, h('p', null, fill(T.missing, name)), make));
	}

	/** After a write: the file's backups and hash, the list beside, the page's `ui`. */
	async function afterWrite() {
		try {
			const f = /** @type {AdminFile} */ (await env.call(`file?name=${encodeURIComponent(name)}`));
			if (destroyed) return;
			if (f.hash === base) {
				file = f;
				disk = f.text;
				readOnly = !f.writable;
				drawHead();
				drawNotes();
				drawBackups();
				updateState();
			}
		} catch {
			// Said when it is next read.
		}
		drawSide();
		env.ctx.reloadUi().catch(() => null);
	}

	// ------------------------------------------------ checking
	async function runCheck() {
		later.cancel();
		if (readOnly || !loaded) return;
		const text = ta.value;
		const my = ++checkSeq;
		setCheckLine(T.checking);
		try {
			const a = /** @type {import('./types.js').CheckAnswer} */ (await env.call('file/check', { name, text }));
			if (my !== checkSeq || destroyed) return;
			mistakes = a.mistakes || [];
			warnings = a.warnings || [];
			diff = a.diff || [];
			drawChecked(!!a.sound);
			if (a.sound) setCheckLine(T.sound, 'good');
			else setCheckLine(fill(mistakes.length === 1 ? T.mistakeOne : T.mistakes, mistakes.length), 'bad');
		} catch (e) {
			if (my !== checkSeq || env.quiet(e)) return;
			setCheckLine(errorSentence(e), 'bad');
		}
	}

	// ------------------------------------------------ writing
	async function save() {
		if (readOnly || !loaded || !dirty()) return;
		later.cancel();
		checkSeq++;
		const text = ta.value;
		saveBtn.disabled = true;
		setCheckLine(T.saving);
		try {
			const a = /** @type {import('./types.js').SaveAnswer} */ (await env.call('file/save', { name, text, base }));
			drafts.delete(name);
			isNew = false;
			disk = text;
			base = a.hash;
			if (file) file.hash = a.hash;
			clear(conflictBox);
			mistakes = [];
			warnings = [];
			diff = [];
			drawChecked(true);
			drawNotes();
			drawHead();
			const words = fill(T.saved, a.generation);
			setCheckLine(words, 'good');
			env.ctx.toast(words);
			afterWrite();
		} catch (e) {
			if (env.quiet(e)) return;
			writeFailed(e);
		} finally {
			updateState();
		}
	}

	/** A write refused: its mistakes, the file changed on disk, read-only, or the service's words. @param {unknown} e */
	function writeFailed(e) {
		if (env.quiet(e)) return;
		setCheckLine(errorSentence(e), 'bad');
		if (!isRefusal(e)) return env.fail(e);
		if (e.code === 'bad_settings') {
			mistakes = mistakesOf(e);
			warnings = [];
			drawChecked(false);
		} else if (e.code === 'changed_on_disk') {
			showConflict(isNew ? T.existsNow : T.changedOnDisk);
		} else if (e.code === 'read_only') {
			readOnly = true;
			if (file) file.why = e.detail || file.why;
			drawHead();
			drawNotes();
			updateState();
		}
		// Any other refusal: said in the status line, beside the buttons.
	}

	/** Says the file on disk isn't what the text was made on; offers to look, to reload, or to write over it. @param {string} text */
	function showConflict(text) {
		clear(conflictBox);
		conflictBox.append(
			h(
				'div',
				{ class: 'note critical adm-conflict', role: 'alert' },
				h('p', null, text),
				h('div', { class: 'adm-row' }, button(T.seeDisk, () => seeDisk()), button(T.reloadDisk, () => reloadFromDisk()), button(T.saveOver, () => saveOver(), 'danger')),
			),
		);
	}

	/** The file as it is on disk now, and what saving the text over it would change. */
	async function seeDisk() {
		try {
			const f = /** @type {AdminFile} */ (await env.call(`file?name=${encodeURIComponent(name)}`));
			/** @type {import('./types.js').CheckAnswer | null} */
			let c = null;
			try {
				c = await env.call('file/check', { name, text: ta.value });
			} catch (e) {
				if (env.quiet(e)) return;
			}
			dialog(env, {
				title: fill(T.diskTitle, name),
				wide: true,
				body: [
					h('h3', null, T.diskDiff),
					c ? (c.diff.length ? diffView(c.diff) : h('p', { class: 'muted' }, T.noDiff)) : h('p', { class: 'muted' }, T.diffUnavailable),
					h('details', null, h('summary', null, T.diskText), h('pre', { class: 'adm-pre mono' }, f.text)),
				],
				buttons: [
					{ text: S.core.close },
					{ text: T.reloadDisk, run: () => reloadFromDisk() },
					{ text: T.saveOver, cls: 'danger', run: () => saveOver() },
				],
			});
		} catch (e) {
			if (isRefusal(e) && e.code === 'not_found') env.ctx.toast(T.goneFromDisk, 'warning');
			else env.fail(e);
		}
	}

	async function reloadFromDisk() {
		if (dirty() && !(await confirmed(env, { title: T.reloadTitle, body: [T.reloadText], confirm: T.reloadDisk, danger: true }))) return;
		drafts.delete(name);
		await load(null);
		drawSide();
	}

	async function saveOver() {
		if (!(await confirmed(env, { title: T.saveOverTitle, body: [T.saveOverText], confirm: T.saveOver, danger: true, loud: true }))) return;
		try {
			const f = /** @type {AdminFile} */ (await env.call(`file?name=${encodeURIComponent(name)}`));
			base = f.hash;
			file = f;
			disk = f.text;
			isNew = false;
		} catch (e) {
			if (!(isRefusal(e) && e.code === 'not_found')) return writeFailed(e);
			// Gone from disk meanwhile: written as a new file.
			base = null;
			isNew = true;
		}
		if (!isNew && ta.value === disk) {
			// The same text as on disk now: nothing to write.
			drafts.delete(name);
			clear(conflictBox);
			setCheckLine('');
			updateState();
			return;
		}
		drafts.set(name, { text: ta.value, base });
		await save();
	}

	// ------------------------------------------------ backups
	/** @param {import('./types.js').Backup} b */
	async function viewBackup(b) {
		try {
			const bk = await env.call(`file/backup?name=${encodeURIComponent(name)}&id=${encodeURIComponent(b.id)}`);
			const text = typeof bk?.text === 'string' ? bk.text : '';
			/** @type {import('./types.js').CheckAnswer | null} */
			let c = null;
			try {
				c = await env.call('file/check', { name, text });
			} catch (e) {
				if (env.quiet(e)) return;
			}
			/** @type {{ text: string, cls?: string, run?: () => unknown }[]} */
			const buttons = [{ text: S.core.close }];
			if (!readOnly) {
				buttons.push({
					text: T.openAsDraft,
					run: () => {
						setText(text);
						remember();
						updateState();
						runCheck();
						goToLine(1);
					},
				});
				buttons.push({ text: T.restore, cls: 'primary', run: () => restore(b) });
			}
			dialog(env, {
				title: fill(T.backupTitle, dateTime(b.at, true)),
				wide: true,
				body: [
					h('p', { class: 'muted' }, T.backupDiffNote),
					c ? (c.diff.length ? diffView(c.diff) : h('p', { class: 'muted' }, T.backupSame)) : h('p', { class: 'muted' }, T.diffUnavailable),
					c && !c.sound ? h('p', { class: 'note warning' }, T.backupUnsound) : null,
					h('details', null, h('summary', null, T.backupText), h('pre', { class: 'adm-pre mono' }, text)),
				],
				buttons,
			});
		} catch (e) {
			env.fail(e);
		}
	}

	/** @param {import('./types.js').Backup} b */
	async function restore(b) {
		const ok = await confirmed(env, {
			title: T.restoreTitle,
			body: [fill(T.restoreText, name, dateTime(b.at, true)), dirty() ? h('p', { class: 'note warning' }, T.restoreDrops) : null],
			confirm: T.restore,
		});
		if (!ok) return;
		try {
			const a = /** @type {import('./types.js').SaveAnswer} */ (await env.call('file/restore', { name, backup: b.id, base: file ? file.hash : base }));
			drafts.delete(name);
			env.ctx.toast(fill(T.restored, a.generation));
			await load(null);
			setCheckLine(fill(T.restored, a.generation), 'good');
			afterWrite();
		} catch (e) {
			writeFailed(e);
		}
	}

	// ------------------------------------------------ renaming, deleting, undoing
	async function rename() {
		const input = h('input', { class: 'mono', autocomplete: 'off', spellcheck: 'false', autocapitalize: 'off', 'aria-label': T.newName });
		input.value = dropinStem(name);
		const why = h('p', { class: 'adm-field-why', 'aria-live': 'polite' });
		/** @type {string | null} */
		let to = null;
		const check = () => {
			const r = dropinName(input.value);
			to = null;
			why.classList.toggle('bad', !!r.error || r.name === name);
			if (r.error) why.textContent = /** @type {Record<string, string>} */ (T.nameErrors)[r.error];
			else if (r.name === name) why.textContent = T.sameName;
			else {
				to = r.name;
				why.textContent = fill(T.willBe, r.name);
			}
		};
		input.addEventListener('input', check);
		check();
		dialog(env, {
			title: fill(T.renameTitle, name),
			body: [h('div', { class: 'adm-name-row' }, h('span', { class: 'mono muted' }, DROPIN_DIR), input, h('span', { class: 'mono muted' }, SUFFIX)), why, h('p', { class: 'muted' }, T.renameNote)],
			buttons: [
				{ text: S.core.cancel },
				{
					text: T.rename,
					cls: 'primary',
					run: async () => {
						if (!to) return true;
						try {
							await env.call('file/rename', { name, to, base });
							drafts.delete(name);
							env.ctx.toast(fill(T.renamed, to));
							env.ctx.go(fileHref(to));
						} catch (e) {
							writeFailed(e);
						}
					},
				},
			],
		});
		input.focus();
	}

	async function remove() {
		const ok = await confirmed(env, { title: fill(T.deleteTitle, name), body: [T.deleteText], confirm: T.delete, danger: true, loud: true });
		if (!ok) return;
		try {
			await env.call('file/delete', { name, base });
			drafts.delete(name);
			env.ctx.toast(fill(T.deleted, name));
			env.ctx.go('#/admin/files');
		} catch (e) {
			writeFailed(e);
		}
	}

	async function undo() {
		if (isNew) {
			if (ta.value && !(await confirmed(env, { title: T.discardTitle, body: [T.discardText], confirm: T.discard, danger: true }))) return;
			drafts.delete(name);
			env.ctx.go('#/admin/files');
			return;
		}
		if (!(await confirmed(env, { title: T.undoTitle, body: [T.undoText], confirm: T.undo, danger: true }))) return;
		setText(disk);
		remember();
		mistakes = [];
		warnings = [];
		diff = [];
		drawChecked(null);
		setCheckLine('');
		updateState();
	}

	// ------------------------------------------------ the wiring
	ta.addEventListener('input', () => {
		remember();
		drawGutter();
		updateState();
		setCheckLine('');
		later();
	});
	ta.addEventListener('scroll', syncScroll);
	for (const ev of ['keyup', 'click', 'select', 'focus']) ta.addEventListener(ev, markCur);
	ta.addEventListener('keydown', (ev) => {
		if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 's') {
			ev.preventDefault();
			save();
		}
	});
	const goFromInput = () => {
		const n = Number(lineInput.value);
		if (n > 0) goToLine(n);
	};
	lineInput.addEventListener('keydown', (ev) => {
		if (ev.key === 'Enter') goFromInput();
	});
	goBtn.addEventListener('click', goFromInput);
	saveBtn.addEventListener('click', () => save());
	checkBtn.addEventListener('click', () => runCheck());
	previewBtn.addEventListener('click', () => {
		/** @type {{ name: string, text: string }[]} */
		const files = [{ name, text: ta.value }];
		for (const [n, d] of drafts) if (n !== name) files.push({ name: n, text: d.text });
		closePreview = openPreview(env, files, openMistake);
	});
	undoBtn.addEventListener('click', () => undo());
	renameBtn.addEventListener('click', () => rename());
	deleteBtn.addEventListener('click', () => remove());

	setText('');
	drawChecked(null);
	updateState();
	clear(backupsBox);
	backupsBox.append(loading());
	load(startLine).then(() => drawSide());

	return () => {
		destroyed = true;
		later.cancel();
		closePreview();
	};
}
