// A plain script, run before the page's modules. When one of them can't be fetched (a content
// blocker stopping a file by its name, a connection dropped while the page loads), the browser runs
// none of them and the page would stay blank: this says so instead. Its words are its own, as the
// module of the page's strings may be the one missing.
(() => {
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
