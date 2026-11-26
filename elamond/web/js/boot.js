// A plain script, run before the page's modules. First the theme, before anything is drawn: this
// browser's choice, else the one shown last, else the system's (theme.js decides once the profile
// comes), so a load doesn't flash another theme. Then: when one of the modules can't be fetched (a content
// blocker stopping a file by its name, a connection dropped while the page loads), the browser runs
// none of them and the page would stay blank: this says so instead. Its words are its own, as the
// module of the page's strings may be the one missing.
(() => {
	let theme = null;
	try {
		for (const key of ['elamon.theme', 'elamon.shown']) {
			const v = JSON.parse(localStorage.getItem(key) || 'null');
			if (v === 'light' || v === 'dark' || v === 'black') {
				theme = v;
				break;
			}
		}
	} catch {
		// No storage (a private window may refuse it): the system's.
	}
	document.documentElement.dataset.theme = theme || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
	// The layout this browser picked (arrange.js, `layoutOf`: each value one of its choice's, else the
	// first, what is there until picked) and whether the side bar is folded, so the page doesn't jump
	// once the modules run.
	const PICKS = { width: ['full', 'fixed'], columns: ['auto', '4', '8'], density: ['normal', 'compact', 'roomy'], inset: ['normal', 'none', 'small', 'large'] };
	/** @param {string} key */
	const kept = (key) => {
		try {
			return JSON.parse(localStorage.getItem(key) || 'null');
		} catch {
			// None kept, or none readable: what is there until picked.
			return null;
		}
	};
	const layout = kept('elamon.layout');
	const side = kept('elamon.side');
	const picked = layout && typeof layout === 'object' && !Array.isArray(layout) ? layout : {};
	for (const [key, values] of Object.entries(PICKS)) document.documentElement.dataset[key] = values.includes(picked[key]) ? picked[key] : values[0];
	document.documentElement.dataset.side = side === 'rail' ? 'rail' : 'open';
	// style.css is in by now (a script waits for the stylesheets before it): the browser's bar in this theme's background.
	const meta = document.querySelector('meta[name="theme-color"]');
	if (meta) meta.setAttribute('content', getComputedStyle(document.documentElement).getPropertyValue('--bg').trim());

	window.addEventListener('error', (event) => {
		const script = event.target;
		if (!(script instanceof HTMLScriptElement) || script.type !== 'module') return;
		const root = document.getElementById('app');
		if (!root || root.childElementCount > 0) return;
		const title = document.createElement('h1');
		title.textContent = "Part of the page didn't load";
		const text = document.createElement('p');
		text.textContent = "The browser couldn't fetch one of the page's files, so none of them run. "
			+ 'A content blocker (AdGuard, uBlock Origin and the like) may be stopping it: allow this '
			+ "address in it, then reload. If there's none, reload once the computer can be reached.";
		const box = document.createElement('section');
		box.className = 'card pair';
		box.setAttribute('role', 'alert');
		box.append(title, text);
		root.append(box);
	}, true);
})();
