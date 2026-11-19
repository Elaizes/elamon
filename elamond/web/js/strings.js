// Every word the page shows, in one place (English). Each part of the page has its own section;
// add to yours only. `{0}`, `{1}` … are filled in by `fill`.

export const S = {
	// ---------------------------------------------------------------- the page itself
	core: {
		title: 'Elamon',
		loading: 'Loading…',
		noConnection: 'No connection to the computer. Trying again…',
		unreachable: "Can't reach the computer.",
		tooOld: 'This page speaks API {0}; the computer speaks {1}. Update elamond.',
		admin: 'Admin',
		back: 'Back',
		themeSystem: 'Theme: as the system',
		themeLight: 'Theme: light',
		themeDark: 'Theme: dark',
		close: 'Close',
		cancel: 'Cancel',
		ok: 'OK',
		retry: 'Try again',
		notFound: 'Nothing here.',
	},

	// ---------------------------------------------------------------- pairing and dashboards (panels)
	panels: {
	},

	// ---------------------------------------------------------------- the admin panel
	admin: {
	},

	// ---------------------------------------------------------------- the dashboard editor
	editor: {
	},
};

/** `text` with `{0}`, `{1}` … replaced by `args`. @param {string} text @param {...unknown} args */
export function fill(text, ...args) {
	return text.replace(/\{(\d+)\}/g, (m, i) => (Number(i) < args.length ? String(args[Number(i)]) : m));
}
