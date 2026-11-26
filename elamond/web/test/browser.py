#!/usr/bin/env python3
"""The page in a headless Chromium, driven by Playwright, against the throwaway elamond browser.sh
starts: each scenario is a function below, run in this order (pair first, as the others use the
browser it pairs). Elements are found as a person finds them, by the words the page shows
(strings.js) and their ARIA roles. Each scenario asserts what the page shows and, where it says so,
what the service answers the page's own fetch or what is on disk.

A scenario fails on any console error or page error it doesn't expect: those it does (a 401 before
pairing, a save the service refuses) are named with it, by status and path.

From browser.sh, which sets BROWSER_BASE (https://127.0.0.1:PORT), BROWSER_WORK (its work folder:
cfg/, data/), BROWSER_ELAMOND (elamond with the test's settings and data) and BROWSER_SHOT_DIR
(the screenshots). Arguments: the scenarios to run (pair always runs first); none for all of them.
Exit 1 when any fails.
"""

import json
import os
import re
import subprocess
import sys
import time
import traceback
from pathlib import Path
from urllib.parse import urlsplit

from playwright.sync_api import expect, sync_playwright

BASE = os.environ['BROWSER_BASE']
WORK = Path(os.environ['BROWSER_WORK'])
ELAMOND = os.environ['BROWSER_ELAMOND']
SHOTS = Path(os.environ.get('BROWSER_SHOT_DIR') or WORK / 'shots')
CFG = WORK / 'cfg'
STATE = WORK / 'owner-state.json'

# The longest any one thing is waited for, in milliseconds.
WAIT_MS = 15_000
expect.set_options(timeout=WAIT_MS)

WIDE = {'width': 1366, 'height': 900}
PHONE = {'width': 390, 'height': 844}
# Every widget type the page draws, as panels.js asks for them.
WIDGETS = 'value,gauge,meter,status,graph,list,bars,table,button,toggle,select,slider,text,group,media,timeline,log'
DASH = '—'
ROLE = 'This browser’s role on the computer: {}'


class Failure(AssertionError):
	pass


def check(cond, message):
	if not cond:
		raise Failure(message)


def until(page, cond, what, timeout_ms=WAIT_MS):
	"""Waits for `cond()`, asked in Python every 100 ms meanwhile Playwright takes in the page's events."""
	deadline = time.monotonic() + timeout_ms / 1000
	while True:
		value = cond()
		if value:
			return value
		if time.monotonic() > deadline:
			raise Failure(f'timed out after {timeout_ms} ms waiting for {what}')
		page.wait_for_timeout(100)


# The page's own fetch: same origin, its cookie, `Elamon-Client` as api.js says it.
FETCH = """async ([path, method, body]) => {
	const { CLIENT } = await import('/web/js/api.js');
	const init = { method, credentials: 'same-origin', cache: 'no-store', headers: { 'Elamon-Client': CLIENT, Accept: 'application/json' } };
	if (body !== null) {
		init.headers['Content-Type'] = 'application/json';
		init.body = JSON.stringify(body);
	}
	const r = await fetch(path, init);
	const text = await r.text();
	let json = null;
	try { json = JSON.parse(text); } catch {}
	return { status: r.status, json, text };
}"""


def fetch(page, path, method='GET', body=None):
	return page.evaluate(FETCH, [path, method, body])


def api(page, path, method='GET', body=None):
	r = fetch(page, path, method, body)
	check(200 <= r['status'] < 300, f'{method} {path}: {r["status"]} {r["text"][:300]}')
	return r['json']


def ui(page):
	return api(page, f'/api/v1/ui?widgets={WIDGETS}')


def is_cookie(c):
	"""Whether a cookie is the service's: `__Host-elamon`, or since it is named per service `__Host-elamon-<id>`."""
	return c['name'] == '__Host-elamon' or c['name'].startswith('__Host-elamon-')


def colon(fp):
	return ':'.join(fp[i:i + 2] for i in range(0, len(fp), 2)).upper()


def shot(page, name, full=True):
	SHOTS.mkdir(parents=True, exist_ok=True)
	# Transitions finished: a gauge as it ends up, not on its way there.
	page.screenshot(path=str(SHOTS / f'{name}.png'), full_page=full, animations='disabled')


def elamond(*args, timeout=30):
	"""Runs elamond with the test's settings and data, as the person at the computer would."""
	r = subprocess.run([ELAMOND, *args], capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
	check(r.returncode == 0, f'elamond {" ".join(args)} exited {r.returncode}: {r.stdout}{r.stderr}')
	return r.stdout


class Pairing:
	"""`elamond pair` in the background: its code and the certificate it prints."""

	def __init__(self, *args):
		self.log = WORK / f'pair-{time.monotonic_ns()}.log'
		self.out = open(self.log, 'w')
		self.proc = subprocess.Popen([ELAMOND, 'pair', *args], stdout=self.out, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
		deadline = time.monotonic() + WAIT_MS / 1000
		while True:
			text = self.log.read_text(errors='replace')
			code = re.search(r'^\s+(\d{3}) (\d{3})\s*$', text, re.M)
			cert = re.search(r'^\s+Certificate:\s+(.+?)\s*$', text, re.M)
			if code and cert:
				self.code = code.group(1) + code.group(2)
				self.certificate = cert.group(1)
				return
			if self.proc.poll() is not None or time.monotonic() > deadline:
				self.stop()
				raise Failure(f'elamond pair printed no code: {text}')
			time.sleep(0.1)

	def wait(self, timeout=WAIT_MS / 1000):
		try:
			return self.proc.wait(timeout=timeout)
		except subprocess.TimeoutExpired:
			return None

	def stop(self):
		# Its own process, by the handle that started it: stopping withdraws the code.
		if self.proc.poll() is None:
			self.proc.terminate()
			try:
				self.proc.wait(timeout=10)
			except subprocess.TimeoutExpired:
				self.proc.kill()
				self.proc.wait()
		self.out.close()


class Watch:
	"""What the pages of a context say went wrong (console errors, uncaught errors) and every URL
	they asked for. `allow`: (status, path pattern) of the failed requests the scenario expects."""

	def __init__(self, context, allow):
		self.allow = [(status, re.compile(path)) for status, path in allow]
		self.errors = []
		self.urls = []
		self.pages = []
		context.on('page', self.attach)
		context.on('request', lambda r: self.urls.append(r.url))

	def attach(self, page):
		self.pages.append(page)
		page.on('console', lambda m: m.type == 'error' and self.errors.append(('console', m.text, (m.location or {}).get('url', ''))))
		page.on('pageerror', lambda e: self.errors.append(('pageerror', f'{e.name}: {e.message}', page.url)))

	def expected(self, text, url):
		m = re.search(r'status of (\d+)', text)
		path = urlsplit(url).path
		return bool(m) and any(int(m.group(1)) == status and pattern.search(path) for status, pattern in self.allow)

	def unexpected(self):
		return [f'{kind}: {text} ({url})' for kind, text, url in self.errors if not (kind == 'console' and self.expected(text, url))]


class Run:
	"""What the scenarios share: the browser, the hello of the service, the owner browser's cookie
	(STATE) and its profile; and each scenario's own contexts, closed after it."""

	def __init__(self, browser):
		self.browser = browser
		self.hello = None
		self.profile = None
		self.contexts = []

	def page(self, state=True, allow=(), viewport=WIDE, **kw):
		"""A page in a fresh context: with the owner browser's cookie (`state`), or none."""
		ctx = self.browser.new_context(ignore_https_errors=True, storage_state=str(STATE) if state else None, viewport=viewport, **kw)
		watch = Watch(ctx, allow)
		self.contexts.append((ctx, watch))
		page = ctx.new_page()
		page.watch = watch
		return page

	def finish(self, failed, name):
		"""Closes this scenario's contexts; what went wrong in them; a screenshot of each page if it failed."""
		problems = []
		for i, (ctx, watch) in enumerate(self.contexts):
			if failed:
				for j, page in enumerate(watch.pages):
					try:
						shot(page, f'FAIL-{name}-{i}{j}')
					except Exception:
						pass
			problems += watch.unexpected()
			ctx.close()
		self.contexts = []
		return problems


# ---------------------------------------------------------------------------------------- scenarios

def test_pair(T):
	"""The code of `elamond pair --role owner` typed into the page; the certificate shown is hello's;
	the token stays in an HttpOnly cookie no script, address or storage of the page holds."""
	pairing = Pairing('--yes', '--role', 'owner')
	try:
		page = T.page(state=False, allow=[(401, r'^/api/v1/ui$')])
		page.goto(BASE + '/')
		expect(page.get_by_role('heading', name='Pair this browser')).to_be_visible()
		T.hello = api(page, '/api/v1/hello')
		fp = T.hello.get('fingerprint') or ''
		check(re.fullmatch(r'[0-9a-f]{64}', fp), f'hello has no fingerprint: {T.hello}')
		check(T.hello.get('name') == 'Test computer', f'hello names another computer: {T.hello}')
		cert = page.get_by_role('region', name='This computer’s certificate')
		expect(cert).to_contain_text(colon(fp))
		# As `elamond pair` printed it, and so as the person compares it.
		expect(cert).to_contain_text(f'As elamond pair prints it: {pairing.certificate}')
		check(pairing.certificate.replace(' ', '').lower() == fp[:32], f'elamond pair printed {pairing.certificate}, hello says {fp}')
		shot(page, '01-pairing')

		page.get_by_label('Code from the computer').fill(pairing.code)
		with page.expect_response(lambda r: urlsplit(r.url).path == '/api/v1/pair') as answer:
			page.get_by_role('button', name='Pair', exact=True).click()
		check(answer.value.status == 200, f'pairing answered {answer.value.status}: {answer.value.text()}')
		body = answer.value.text()
		check('token' not in json.loads(body), f'pairing a browser answered a token in its body: {body}')
		expect(page.get_by_role('tablist', name='Dashboards')).to_be_visible()
		expect(page.get_by_role('tab', name='Overview')).to_be_visible()
		expect(page.get_by_label(ROLE.format('Owner'))).to_be_visible()
		check(pairing.wait() == 0, f'elamond pair did not end well: {pairing.log.read_text()}')

		cookies = [c for c in page.context.cookies() if is_cookie(c)]
		check(len(cookies) == 1, f'not one cookie of the service\'s: {page.context.cookies()}')
		c = cookies[0]
		# Named for the service (`__Host-elamon-<its id's first 8>`), or by the name before.
		check(c['name'] in ('__Host-elamon', f'__Host-elamon-{T.hello["id"][:8]}'), f'the cookie is named {c["name"]}, the service is {T.hello["id"]}')
		check(c['httpOnly'] and c['secure'] and c['sameSite'] == 'Strict' and c['path'] == '/', f'the cookie is not HttpOnly, Secure, SameSite=Strict on /: {c}')
		token = c['value']
		check(len(token) >= 16, 'the cookie holds no token')
		check(token not in body, 'the pairing answer holds the token')
		seen = page.evaluate('document.cookie')
		check(token not in seen and c['name'] not in seen, f'the page\'s script sees the cookie: {seen!r}')
		stored = page.evaluate('JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } })')
		check(token not in stored, f'the token is in the page\'s storage: {stored}')
		check(not any(token in u for u in page.watch.urls), 'the token is in a request\'s address')

		u = ui(page)
		check(u['role'] == 'owner', f'the browser paired as {u["role"]}, not owner')
		check('admin' in u['service']['features'] and 'editor' in u['service']['features'], f'no admin or editor in features: {u["service"]["features"]}')
		T.profile = u['profile']['id']
		clients = api(page, '/api/v1/admin/clients')['clients']
		me = [x for x in clients if x.get('you')]
		check(len(me) == 1 and me[0]['kind'] == 'web' and me[0]['app'].startswith('web/'), f'the service doesn\'t list this browser as kind web: {clients}')
		shot(page, '02-paired')
		page.context.storage_state(path=str(STATE))
	finally:
		pairing.stop()


STREAMS = re.compile(r'/api/v1/stream\?')
NUMBERS = "() => (document.querySelector('main').innerText.match(/\\d+(?:[.,]\\d+)?/g) || []).join(' ')"
HIDE = """() => {
	Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
	Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' });
	document.dispatchEvent(new Event('visibilitychange'));
}"""
SHOW = """() => {
	delete document.hidden;
	delete document.visibilityState;
	document.dispatchEvent(new Event('visibilitychange'));
}"""


class Streams:
	"""The page's requests of `GET /api/v1/stream`: those open now, how many were made, and the
	changes told to an open one (`POST /api/v1/stream/{id}`)."""

	def __init__(self, page):
		self.open = set()
		self.made = 0
		self.moves = 0
		page.on('request', self.request)
		page.on('requestfinished', self.end)
		page.on('requestfailed', self.end)

	def request(self, r):
		if STREAMS.search(r.url):
			self.open.add(r)
			self.made += 1
		elif r.method == 'POST' and re.search(r'/api/v1/stream/[^/?]+$', r.url):
			self.moves += 1

	def end(self, r):
		self.open.discard(r)


def test_blocked(T):
	"""A content blocker stopping one of the page's files by its name (EasyPrivacy stopped
	`/fingerprint.js` in 0.11.0): the browser runs none of the modules, and the page says why
	instead of staying blank."""
	page = T.page(state=False)
	page.route('**/web/js/cert.js', lambda route: route.abort('blockedbyclient'))
	page.goto(BASE + '/')
	alert = page.get_by_role('alert')
	expect(alert).to_contain_text("Part of the page didn't load")
	expect(alert).to_contain_text('content blocker')
	shot(page, '00-blocked')
	# The browser's own line about the stopped file is what this scenario wants.
	page.watch.errors = [e for e in page.watch.errors if 'ERR_BLOCKED_BY_CLIENT' not in e[1]]


def test_panels(T):
	"""The overview live: its numbers change; one stream for every tab, closed while the page is
	hidden; light, dark and a phone's width."""
	page = T.page(color_scheme='light')
	streams = Streams(page)
	page.goto(BASE + '/#/tab/overview')
	cpu = page.get_by_role('meter', name='CPU')
	expect(cpu).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	expect(page.get_by_role('meter', name='Memory')).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	first = page.evaluate(NUMBERS)
	page.wait_for_function(f'(t) => ({NUMBERS})() !== t', arg=first, timeout=WAIT_MS)
	until(page, lambda: len(streams.open) == 1, 'one open stream')
	shot(page, '03-overview-wide-light')
	light = page.evaluate('getComputedStyle(document.body).backgroundColor')

	# Other tabs: the same stream, told to send another dashboard.
	for tab, sign in [('Control', page.get_by_role('button', name='Greet', exact=True)), ('Sensors', page.get_by_role('region', name='Sensors')), ('Overview', cpu)]:
		page.get_by_role('tab', name=tab).click()
		expect(page.get_by_role('tab', name=tab)).to_have_attribute('aria-selected', 'true')
		expect(sign).to_be_visible()
		check(len(streams.open) <= 1, f'{len(streams.open)} streams open on {tab}')
	expect(cpu).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	until(page, lambda: streams.moves >= 3, 'the stream told of each tab')
	check(streams.made == 1 and len(streams.open) == 1, f'switching tabs opened {streams.made} streams ({len(streams.open)} open), not one kept')

	# Hidden: no stream, and no stale numbers.
	page.evaluate(HIDE)
	until(page, lambda: not streams.open, 'the stream to close while the page is hidden', 5000)
	expect(cpu).to_have_attribute('aria-valuetext', DASH)
	page.evaluate(SHOW)
	until(page, lambda: len(streams.open) == 1, 'a stream once the page shows again', 5000)
	expect(cpu).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	check(streams.made == 2, f'{streams.made} streams made: one, then one more after showing again')

	page.emulate_media(color_scheme='dark')
	until(page, lambda: page.evaluate('getComputedStyle(document.body).backgroundColor') != light, 'the dark theme')
	shot(page, '04-overview-wide-dark')

	phone = T.page(viewport=PHONE, is_mobile=True, has_touch=True, device_scale_factor=2, color_scheme='light')
	phone.goto(BASE + '/#/tab/overview')
	expect(phone.get_by_role('meter', name='CPU')).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	width = phone.evaluate('[document.documentElement.scrollWidth, document.documentElement.clientWidth]')
	check(width[0] <= width[1] + 1, f'the page is wider than a phone: {width[0]} px in {width[1]}')
	shot(phone, '05-overview-phone')


THEME = 'document.documentElement.dataset.theme'
BG = 'getComputedStyle(document.body).backgroundColor'


def test_theme(T):
	"""The theme menu: Light, Dark or Black for this browser, over its profile; kept across a reload
	and in place before the page has asked the computer anything (no other theme flashes first).
	Without a choice (and with 0.11.2's `auto`), the profile's: here `system`, the browser's own."""
	page = T.page(color_scheme='light')
	page.goto(BASE + '/#/tab/overview')
	expect(page.get_by_role('tablist', name='Dashboards')).to_be_visible()
	check(page.evaluate(THEME) == 'light', f'a light system shows {page.evaluate(THEME)}')
	button = page.get_by_role('button', name=re.compile('^Theme: '))
	expect(button).to_have_attribute('aria-label', 'Theme: Light')
	button.click()
	items = page.get_by_role('menuitemradio')
	expect(items).to_have_count(3)
	expect(page.get_by_role('menuitemradio', name='Light')).to_have_attribute('aria-checked', 'true')
	page.get_by_role('menuitemradio', name='Black').click()
	until(page, lambda: page.evaluate(THEME) == 'black', 'the black theme')
	check(page.evaluate(BG) == 'rgb(0, 0, 0)', f'black, but the page is {page.evaluate(BG)}')
	expect(button).to_have_attribute('aria-label', 'Theme: Black')
	check(page.evaluate("document.querySelector('meta[name=\"theme-color\"]').content") == '#000000', 'theme-color is not the black theme\'s')
	shot(page, '04-theme-black', full=False)

	# A reload: black before the computer is asked anything, and after.
	held = []
	page.route(re.compile(r'/api/v1/hello'), lambda r: held.append(r))
	page.reload()
	until(page, lambda: held, 'the page asking for hello')
	check(page.evaluate(THEME) == 'black', f'before the computer answers, the page is {page.evaluate(THEME)}')
	for r in held:
		r.continue_()
	page.unroute(re.compile(r'/api/v1/hello'))
	expect(page.get_by_role('tablist', name='Dashboards')).to_be_visible()
	check(page.evaluate(THEME) == 'black', 'the choice is gone after a reload')

	# What 0.11.2 kept is no choice: the profile's (system), following the browser.
	page.evaluate("localStorage.setItem('elamon.theme', JSON.stringify('auto'))")
	page.reload()
	expect(page.get_by_role('tablist', name='Dashboards')).to_be_visible()
	until(page, lambda: page.evaluate(THEME) == 'light', 'the profile\'s theme again')
	page.emulate_media(color_scheme='dark')
	until(page, lambda: page.evaluate(THEME) == 'dark', 'the system\'s dark theme')
	page.get_by_role('button', name='Theme: Dark').click()
	expect(page.get_by_role('menuitemradio', name='Dark')).to_have_attribute('aria-checked', 'true')
	page.keyboard.press('Escape')
	expect(page.get_by_role('menuitemradio', name='Dark')).to_be_hidden()


# The frame as drawn: the side bar's right edge (0 without one), the outermost cards of the dashboard
# (its own, not a group's), the window's width, the grid's columns, a card's padding, the page's box
# and html's attributes.
FRAME = """() => {
	const R = (e) => e.getBoundingClientRect();
	const side = document.querySelector('.side');
	const cards = [...document.querySelectorAll('.dashboard > .dash-grid > .dash-row > .card')].map(R).filter((r) => r.width > 0);
	const grid = document.querySelector('.dashboard > .dash-grid');
	const page = R(document.querySelector('.page'));
	const head = document.querySelector('.page-top');
	return {
		side: side && getComputedStyle(side).display !== 'none' ? R(side).right : 0,
		left: Math.min(...cards.map((c) => c.left)), right: Math.max(...cards.map((c) => c.right)),
		vw: document.documentElement.clientWidth,
		cols: grid ? Number(getComputedStyle(grid).getPropertyValue('--cols')) : 0,
		pad: cards.length ? parseFloat(getComputedStyle(document.querySelector('.dashboard > .dash-grid > .dash-row > .card')).paddingLeft) : 0,
		gap: grid ? parseFloat(getComputedStyle(grid).rowGap) : 0,
		page: [page.left, page.right], head: head ? R(head).top : null,
		html: { ...document.documentElement.dataset },
	};
}"""


def near(a, b, what):
	check(abs(a - b) <= 1, f'{what}: {a} px, not {b}')


def held_hello(page, then):
	"""Reloads the page with its hello held: `then()` while the computer hasn't been asked anything."""
	held = []
	page.route(re.compile(r'/api/v1/hello'), lambda r: held.append(r))
	page.reload()
	until(page, lambda: held, 'the page asking for hello')
	then()
	for r in held:
		r.continue_()
	page.unroute(re.compile(r'/api/v1/hello'))


def test_layout(T):
	"""The layout menu: the page's width, the dashboards' columns, the density and the space at the
	window's edges, each doing what it says; kept across a reload and in place before the computer is
	asked anything (no jump); Reset; a kept value that is no pick counts as none; on a phone's width
	two columns whatever is picked."""
	page = T.page(viewport={'width': 1920, 'height': 1080})
	page.goto(BASE + '/#/tab/overview')
	expect(page.get_by_role('meter', name='CPU')).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	f = page.evaluate(FRAME)
	check(f['html'].get('width') == 'full' and f['html'].get('columns') == 'auto' and f['html'].get('density') == 'normal' and f['html'].get('inset') == 'normal', f'until picked: {f["html"]}')
	# The whole window: its edges 12 px from the side bar and from the window; 8 columns on 1920.
	near(f['left'] - f['side'], 12, 'the page\'s space beside the side bar')
	near(f['vw'] - f['right'], 12, 'the page\'s space at the window\'s edge')
	check(f['cols'] == 8, f'{f["cols"]} columns on 1920 px, not 8')
	near(f['pad'], 12, 'a card\'s padding')
	shot(page, '06-layout-default', full=False)

	button = page.get_by_role('button', name='Layout')
	button.click()
	menu = page.get_by_role('menu')
	expect(menu.get_by_role('menuitemradio')).to_have_count(12)

	def pick(group, item, done, what):
		menu.get_by_role('group', name=group).get_by_role('menuitemradio', name=item, exact=True).click()
		expect(menu.get_by_role('group', name=group).get_by_role('menuitemradio', name=item, exact=True)).to_have_attribute('aria-checked', 'true')
		until(page, lambda: done(page.evaluate(FRAME)), what)

	pick('Columns', '4', lambda f: f['cols'] == 4, 'four columns')
	pick('Columns', '8 where they fit', lambda f: f['cols'] == 8, 'eight columns')
	pick('Density', 'Compact', lambda f: f['pad'] == 8 and f['gap'] == 8, 'compact cards')
	pick('Density', 'Roomy', lambda f: f['pad'] == 16 and f['gap'] == 16, 'roomy cards')
	pick('Space at the edges', 'None', lambda f: abs(f['left'] - f['side']) <= 1 and abs(f['vw'] - f['right']) <= 1, 'no space at the edges')
	pick('Space at the edges', 'Large', lambda f: abs(f['left'] - f['side'] - 24) <= 1 and abs(f['vw'] - f['right'] - 24) <= 1, '24 px at the edges')
	pick('Width', 'At most 1280 px', lambda f: abs(f['page'][1] - f['page'][0] - 1280) <= 1, 'a page 1280 px wide')
	f = page.evaluate(FRAME)
	near(f['page'][0] - f['side'], f['vw'] - f['page'][1], 'the narrow page between the side bar and the window\'s edge')
	page.keyboard.press('Escape')
	expect(menu).to_be_hidden()
	shot(page, '06-layout-picked', full=False)

	# A reload: every pick in place before the computer is asked anything, and after.
	picked = {'width': 'fixed', 'columns': '8', 'density': 'roomy', 'inset': 'large'}
	held_hello(page, lambda: check({k: page.evaluate(f'document.documentElement.dataset.{k}') for k in picked} == picked, 'the picks before the computer answers'))
	expect(page.get_by_role('meter', name='CPU')).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	f = page.evaluate(FRAME)
	check(f['pad'] == 16 and abs(f['page'][1] - f['page'][0] - 1280) <= 1, f'after a reload: {f}')

	# Reset: what is there until picked.
	button.click()
	menu.get_by_role('menuitem', name='Reset').click()
	until(page, lambda: page.evaluate(FRAME)['html'].get('width') == 'full' and page.evaluate(FRAME)['pad'] == 12, 'the layout reset')
	check(page.evaluate("localStorage.getItem('elamon.layout')") is None, 'Reset kept a layout')
	page.keyboard.press('Escape')

	# What is no pick counts as none; a pick that is, as picked.
	page.evaluate("""localStorage.setItem('elamon.layout', JSON.stringify({ density: 'huge', columns: '12', inset: 'small' }))""")
	page.reload()
	expect(page.get_by_role('meter', name='CPU')).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	f = page.evaluate(FRAME)
	check(f['html'].get('density') == 'normal' and f['html'].get('columns') == 'auto' and f['html'].get('inset') == 'small', f'kept nonsense: {f["html"]}')
	near(f['vw'] - f['right'], 8, 'small space at the edges')

	# A phone: two columns, eight picked or not.
	phone = T.page(viewport=PHONE, is_mobile=True, has_touch=True, device_scale_factor=2)
	phone.add_init_script("localStorage.setItem('elamon.layout', JSON.stringify({ columns: '8' }))")
	phone.goto(BASE + '/#/tab/overview')
	expect(phone.get_by_role('meter', name='CPU')).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	check(phone.evaluate(FRAME)['cols'] == 2, 'a phone with eight columns picked')


SIDE = re.compile(r'dashboards and tools$')


def test_shell(T):
	"""The frame on a wide window: one side bar with the dashboards, the admin panel's sections and
	the editor; the admin panel and the editor in it, Escape back to the dashboards (a menu's Escape
	closes the menu); keys 1–9; the side bar folded to its icons and back, kept before the computer is
	asked anything; the bar over the page in place while it scrolls, the editor's tools under it; Tab
	from the side bar to the bar to the page. On a phone's width: no side bar, the tabs at the top."""
	page = T.page()
	page.goto(BASE + '/#/tab/overview')
	side = page.get_by_role('navigation', name=SIDE)
	tablist = side.get_by_role('tablist', name='Dashboards')
	expect(tablist).to_be_visible()
	expect(tablist.get_by_role('tab', name='Overview')).to_have_attribute('aria-selected', 'true')
	u = ui(page)
	titles = {d['id']: d.get('title') or d['id'] for d in u['dashboards']}
	tabs = [t for t in dict.fromkeys(u['tabs']) if t in titles]
	check(len(tabs) >= 2, f'the test profile has {len(tabs)} tabs')
	# One navigation: no "Back" to the dashboards anywhere, Edit and Admin only in the side bar.
	expect(page.locator('.page-top').get_by_role('link', name='Admin')).to_have_count(0)
	expect(page.locator('.page-top').get_by_role('link', name='Edit this dashboard')).to_have_count(0)

	# 2: the second tab, the same stream (the address follows).
	page.locator('main').focus()
	page.keyboard.press('2')
	expect(tablist.get_by_role('tab', name=titles[tabs[1]], exact=True)).to_have_attribute('aria-selected', 'true')
	expect(page).to_have_url(re.compile(f'#/tab/{re.escape(tabs[1])}$'))

	# The admin panel from the side bar, its section marked there; no header of its own.
	side.get_by_role('navigation', name='Admin sections').get_by_role('link', name='Clients').click()
	expect(page).to_have_url(re.compile(r'#/admin/clients$'))
	expect(side.get_by_role('link', name='Clients')).to_have_attribute('aria-current', 'page')
	expect(page.get_by_role('link', name=re.compile('Back'))).to_have_count(0)
	expect(page.get_by_role('heading', name='Clients')).to_be_visible()
	# The arrows move along the dashboards here without leaving the admin panel.
	tablist.get_by_role('tab', name=titles[tabs[0]], exact=True).focus()
	page.keyboard.press('ArrowDown')
	expect(tablist.get_by_role('tab', name=titles[tabs[1]], exact=True)).to_be_focused()
	expect(page).to_have_url(re.compile(r'#/admin/clients$'))
	# A menu the focus leaves (Tab out of it) closes: Escape is the page's again.
	page.get_by_role('button', name='Layout').click()
	expect(page.get_by_role('menuitem', name='Reset')).to_be_visible()
	page.get_by_role('menuitem', name='Reset').focus()
	page.keyboard.press('Tab')
	expect(page.get_by_role('menuitem', name='Reset')).to_be_hidden()
	# A menu open: Escape closes it and stays.
	page.get_by_role('button', name='More').click()
	expect(page.get_by_role('menuitem', name='Forget this browser')).to_be_visible()
	page.keyboard.press('Escape')
	expect(page.get_by_role('menuitem', name='Forget this browser')).to_be_hidden()
	expect(page).to_have_url(re.compile(r'#/admin/clients$'))
	# Escape: back to the dashboard last open.
	page.keyboard.press('Escape')
	expect(page).to_have_url(re.compile(f'#/tab/{re.escape(tabs[1])}$'))
	expect(tablist.get_by_role('tab', name=titles[tabs[1]], exact=True)).to_have_attribute('aria-selected', 'true')
	# 1 from the admin panel: the first tab.
	page.goto(BASE + '/#/admin/overview')
	expect(page.get_by_role('heading', name='Overview')).to_be_visible()
	page.keyboard.press('1')
	expect(page).to_have_url(re.compile(f'#/tab/{re.escape(tabs[0])}$'))
	expect(tablist.get_by_role('tab', name=titles[tabs[0]], exact=True)).to_have_attribute('aria-selected', 'true')

	# A page opened over a tab: the tab marked, Edit for the page.
	page.get_by_role('meter', name='CPU').click()
	expect(page).to_have_url(re.compile(r'#/tab/cpu$'))
	expect(tablist.get_by_role('tab', name=titles[tabs[0]], exact=True)).to_have_attribute('aria-selected', 'true')
	expect(side.get_by_role('link', name='Edit this dashboard')).to_have_attribute('href', '#/edit/cpu')
	page.go_back()
	expect(page).to_have_url(re.compile(f'#/tab/{re.escape(tabs[0])}$'))

	# The editor from the side bar, for the dashboard on screen; its tools stick under the bar.
	side.get_by_role('link', name='Edit this dashboard').click()
	expect(page).to_have_url(re.compile(f'#/edit/{re.escape(tabs[0])}$'))
	expect(page.get_by_role('heading', name=re.compile('^Edit: '))).to_be_visible()
	expect(side.get_by_role('link', name='Edit this dashboard')).to_have_attribute('aria-current', 'page')
	# The wheel over the page, not over the side bar (which scrolls itself where it is taller than the window).
	page.mouse.move(900, 400)
	page.mouse.wheel(0, 2000)
	until(page, lambda: page.evaluate('scrollY') > 0, 'the editor scrolled')
	until(page, lambda: page.evaluate("Math.abs(document.querySelector('.ed-tools').getBoundingClientRect().top - document.querySelector('.page-top').getBoundingClientRect().bottom) <= 1"), 'the editor\'s tools under the bar')
	check(page.evaluate("document.querySelector('.page-top').getBoundingClientRect().top") == 0, 'the bar left the top while the editor scrolled')
	page.keyboard.press('Escape')
	expect(page).to_have_url(re.compile(f'#/tab/{re.escape(tabs[0])}$'))

	# The bar over the page stays while it scrolls.
	page.set_viewport_size({'width': 1366, 'height': 480})
	page.mouse.move(900, 300)
	page.mouse.wheel(0, 1500)
	until(page, lambda: page.evaluate('scrollY') > 0, 'the dashboard scrolled')
	check(page.evaluate("document.querySelector('.page-top').getBoundingClientRect().top") == 0, 'the bar left the top while the page scrolled')
	page.set_viewport_size(WIDE)

	# Tab: the side bar first, then the bar, then the page.
	order = page.evaluate("""() => {
		const all = [...document.querySelectorAll('button, a[href], [tabindex="0"], input, select')].filter((e) => e.getBoundingClientRect().width > 0 && !e.closest('[hidden]'));
		const at = (sel) => all.findIndex((e) => e.closest(sel));
		return [at('.side'), at('.page-top'), at('.page-body')];
	}""")
	check(order[0] < order[1] < order[2], f'Tab goes side bar, bar, page at {order}')

	# Folded to its icons: names for the screen reader and in the tooltips, kept across a reload.
	wide = page.evaluate("document.querySelector('.side').getBoundingClientRect().width")
	side.get_by_role('button', name='Fold the side bar to its icons').click()
	until(page, lambda: page.evaluate('document.documentElement.dataset.side') == 'rail', 'the side bar folded')
	until(page, lambda: page.evaluate("document.querySelector('.side').getBoundingClientRect().width") < wide / 2, 'a narrow side bar')
	expect(tablist.get_by_role('tab', name=titles[tabs[0]], exact=True)).to_be_visible()
	check(page.evaluate("document.querySelector('.side .tab').title") != '', 'a folded tab has no tooltip')
	held_hello(page, lambda: check(page.evaluate('document.documentElement.dataset.side') == 'rail', 'unfolded before the computer answers'))
	expect(tablist).to_be_visible()
	side.get_by_role('button', name='Open the side bar out').click()
	until(page, lambda: page.evaluate('document.documentElement.dataset.side') == 'open', 'the side bar open again')
	shot(page, '07-shell-wide', full=False)

	# A phone's width: no side bar; the tabs at the top; the admin panel's sections a row of their own.
	phone = T.page(viewport=PHONE, is_mobile=True, has_touch=True, device_scale_factor=2)
	phone.goto(BASE + '/#/tab/overview')
	expect(phone.get_by_role('tablist', name='Dashboards')).to_be_visible()
	expect(phone.get_by_role('navigation', name=SIDE)).to_be_hidden()
	phone.locator('.page-top').get_by_role('link', name='Admin').click()
	expect(phone).to_have_url(re.compile(r'#/admin$'))
	expect(phone.get_by_role('navigation', name='Admin sections')).to_be_visible()
	expect(phone.get_by_role('tablist', name='Dashboards')).to_be_visible()
	width = phone.evaluate('[document.documentElement.scrollWidth, document.documentElement.clientWidth]')
	check(width[0] <= width[1] + 1, f'the page is wider than a phone: {width[0]} px in {width[1]}')
	shot(phone, '07-shell-phone-admin')


def activity(page, what):
	return [e for e in api(page, '/api/v1/admin/activity?limit=200')['entries'] if e.get('what') == what]


def test_action(T):
	"""An action with `confirm`: its question; cancelled, nothing runs; confirmed, its output."""
	page = T.page()
	posts = []
	page.on('request', lambda r: r.method == 'POST' and '/api/v1/actions/' in r.url and posts.append(r.url))
	page.goto(BASE + '/#/tab/control')
	button = page.get_by_role('button', name='Greet', exact=True)
	expect(button).to_be_enabled()
	before = len(activity(page, 'action Greet'))

	button.click()
	question = page.get_by_role('dialog', name='Greet?')
	expect(question).to_be_visible()
	shot(page, '06-action-question', full=False)
	question.get_by_role('button', name='Cancel').click()
	expect(question).to_be_hidden()
	# The run is over once the button is back: it asked nothing of the computer.
	expect(button).to_be_enabled()
	check(not posts, f'cancelled, yet the page asked: {posts}')
	check(len(activity(page, 'action Greet')) == before, 'cancelled, yet the activity log has a run')

	button.click()
	expect(question).to_be_visible()
	question.get_by_role('button', name='Greet', exact=True).click()
	outcome = page.get_by_role('button', name='Greet: Done')
	expect(outcome).to_be_visible()
	outcome.click()
	output = page.get_by_role('dialog', name='Greet', exact=True)
	expect(output).to_contain_text('hello from the browser test')
	shot(page, '07-action-output', full=False)
	output.get_by_role('button', name='Close').click()
	expect(output).to_be_hidden()
	check(len(posts) == 1, f'{len(posts)} requests to run it, not one')
	runs = activity(page, 'action Greet')
	check(len(runs) == before + 1 and runs[0]['result'] == 'ok', f'the activity log has not one run more, ok: {runs}')


def kv(page):
	"""The admin panel's labels and values, the first of each label."""
	return page.evaluate("""() => {
		const out = {};
		for (const dt of document.querySelectorAll('main dl dt')) if (!(dt.textContent in out)) out[dt.textContent] = dt.nextElementSibling ? dt.nextElementSibling.textContent : null;
		return out;
	}""")


def test_admin(T):
	"""The overview; a second browser paired from the page's own code as monitor, its role changed,
	then revoked: it is back at pairing on its next request."""
	a = T.page()
	a.goto(BASE + '/#/admin')
	expect(a.get_by_role('heading', name='Overview')).to_be_visible()
	expect(a.get_by_text('0.11.4', exact=True)).to_be_visible()
	values = kv(a)
	check(values.get('Version') == '0.11.4', f'the overview says version {values.get("Version")}')
	check(values.get('Certificate') == colon(T.hello['fingerprint']), f'the overview says the certificate {values.get("Certificate")}')
	check(values.get('Name') == 'Test computer', f'the overview names {values.get("Name")}')
	shot(a, '08-admin-overview')

	a.goto(BASE + '/#/admin/pairing')
	a.get_by_role('combobox', name='Role').select_option('monitor')
	a.get_by_role('button', name='Make a code').click()
	code = a.get_by_label(re.compile(r'^The code: '))
	expect(code).to_be_visible()
	digits = re.sub(r'\D', '', code.get_attribute('aria-label'))
	check(len(digits) == 6, f'the page shows no six-digit code: {digits}')
	expect(a.get_by_text('Made from this page.')).to_be_visible()
	expect(a.get_by_text('Nobody has given the code yet.')).to_be_visible()
	shot(a, '09-admin-pairing')

	b = T.page(state=False, allow=[(401, r'^/api/v1/(ui|stream(/.*)?)$')])
	b.goto(BASE + '/')
	b.get_by_label('Code from the computer').fill(digits)
	b.get_by_label('Name of this browser (optional)').fill('Second browser')
	b.get_by_role('button', name='Pair', exact=True).click()
	expect(b.get_by_role('status').filter(has_text='Confirm on the computer')).to_be_visible()
	ask = a.get_by_role('heading', name=re.compile(r'^Let Second browser( from \S+)? in\?$'))
	expect(ask).to_be_visible()
	shot(a, '10-admin-ask')
	a.get_by_role('button', name='Allow').click()
	expect(b.get_by_role('tablist', name='Dashboards')).to_be_visible()
	expect(b.get_by_label(ROLE.format('Monitor'))).to_be_visible()

	a.goto(BASE + '/#/admin/clients')
	card = a.get_by_role('article').filter(has=a.get_by_role('heading', name='Second browser', exact=True))
	expect(card).to_be_visible()
	card.get_by_role('combobox', name='Role of Second browser').select_option('operate')
	card.get_by_role('button', name='Apply').click()
	expect(a.get_by_text('Changed 1 client.')).to_be_visible()
	second = [c for c in api(a, '/api/v1/admin/clients')['clients'] if c['name'] == 'Second browser']
	check(len(second) == 1 and second[0]['role'] == 'operate' and second[0]['kind'] == 'web', f'the second browser isn\'t one of kind web, operate: {second}')
	# Its open stream got the new ui at once.
	expect(b.get_by_label(ROLE.format('Operate'))).to_be_visible()
	shot(a, '11-admin-clients')

	card.get_by_role('button', name='Revoke').click()
	sure = a.get_by_role('dialog').filter(has_text='Revoke Second browser?')
	expect(sure).to_be_visible()
	sure.get_by_role('button', name='Revoke').click()
	expect(a.get_by_text('Revoked 1 client.')).to_be_visible()
	check(not [c for c in api(a, '/api/v1/admin/clients')['clients'] if c['name'] == 'Second browser'], 'the second browser is still paired')
	# Its next request (another tab) takes it back to pairing, if its stream ending hasn't already.
	try:
		b.get_by_role('tab', name='Control').click(timeout=3000)
	except Exception:
		pass
	expect(b.get_by_role('heading', name='Pair this browser')).to_be_visible()
	shot(b, '12-revoked-back-to-pairing')


BROKEN = """[dashboards.pagetest]
title = "Page test
icon = "rocket"
"""


def fixed(profile):
	return f"""[dashboards.pagetest]
title = "Page test"
icon = "rocket"

[[dashboards.pagetest.widgets]]
id = "load"
type = "value"
value = "cpu.usage"
label = "Test load"

[profiles.{profile}]
dashboards = ["overview", "processes", "sensors", "control", "pagetest"]
"""


STAMP = """
[actions.stamp]
name = "Stamp"
command = ["echo", "stamped by the page"]
confirm = false

[[dashboards.pagetest.widgets]]
id = "stamp"
type = "button"
action = "stamp"
label = "Stamp"
"""

DROPIN = 'elamond.d/test-page.toml'


def editor_of(page, name):
	return page.get_by_label(f'The text of {name}')


def test_files(T):
	"""A new drop-in: broken, its mistake by its line and the save refused; sound, saved and taken
	up (a new tab); an action and its button added and run from the dashboard; the backup restored;
	elamond.toml, from Nix, read-only with its reason."""
	# A new file isn't there to read until it is saved: 404.
	page = T.page(allow=[(404, r'^/api/v1/admin/file$'), (422, r'^/api/v1/admin/file/save$'), (403, r'^/api/v1/admin/file/save$')])
	page.goto(BASE + '/#/admin/files')
	page.get_by_label('Name', exact=True).fill('test-page')
	expect(page.get_by_text(f'It will be {DROPIN}.')).to_be_visible()
	page.get_by_role('button', name='Create').click()
	text = editor_of(page, DROPIN)
	expect(text).to_be_editable()
	expect(page.get_by_text('New: not written yet.')).to_be_visible()

	text.fill(BROKEN)
	expect(page.get_by_text('1 mistake.', exact=True)).to_be_visible()
	expect(page.get_by_role('button').filter(has_text=f'{DROPIN}:2')).to_be_visible()
	shot(page, '13-files-mistake')
	page.get_by_role('button', name='Save', exact=True).click()
	expect(page.get_by_text('The settings with this change don\'t parse (1 mistake): nothing was written.')).to_be_visible()
	check(not (CFG / DROPIN).exists(), f'{DROPIN} was written though the settings with it don\'t parse')

	first = fixed(T.profile)
	text.fill(first)
	expect(page.get_by_text('The settings with this text are sound.')).to_be_visible()
	page.get_by_role('button', name='Save', exact=True).click()
	expect(page.get_by_text(re.compile(r'^Saved; the service took the settings up \(generation \d+\)\.$')).first).to_be_visible()
	check((CFG / DROPIN).read_text() == first, f'{DROPIN} on disk isn\'t what was saved')
	shot(page, '14-files-saved')
	page.goto(BASE + '/#/')
	expect(page.get_by_role('tab', name='Page test')).to_be_visible()

	# An action and a button for it, written from the page, run from the dashboard.
	page.goto(BASE + f'/#/admin/files/{DROPIN.replace("/", "%2F")}')
	expect(text).to_have_value(first)
	text.fill(first + STAMP)
	expect(page.get_by_text('The settings with this text are sound.')).to_be_visible()
	# The save itself, not its words: those of the first save may still be on the page in its note.
	with page.expect_response(lambda r: urlsplit(r.url).path == '/api/v1/admin/file/save') as saved:
		page.get_by_role('button', name='Save', exact=True).click()
	check(saved.value.status == 200, f'saving the action and its button answered {saved.value.status}: {saved.value.text()[:300]}')
	check((CFG / DROPIN).read_text() == first + STAMP, f'{DROPIN} on disk isn\'t what was saved the second time')
	page.goto(BASE + '/#/tab/pagetest')
	stamp = page.get_by_role('button', name='Stamp', exact=True)
	expect(stamp).to_be_enabled()
	stamp.click()
	outcome = page.get_by_role('button', name='Stamp: Done')
	expect(outcome).to_be_visible()
	outcome.click()
	expect(page.get_by_role('dialog', name='Stamp', exact=True)).to_contain_text('stamped by the page')
	shot(page, '15-files-new-action', full=False)
	page.get_by_role('dialog', name='Stamp', exact=True).get_by_role('button', name='Close').click()

	# Back to the backup: the text saved first, the action gone.
	page.goto(BASE + f'/#/admin/files/{DROPIN.replace("/", "%2F")}')
	expect(text).to_have_value(first + STAMP)
	backups = page.get_by_role('button', name='Restore')
	expect(backups).to_have_count(1)
	backups.click()
	sure = page.get_by_role('dialog').filter(has_text='Restore this backup?')
	sure.get_by_role('button', name='Restore').click()
	expect(page.get_by_text(re.compile(r'^Restored; the service took the settings up')).first).to_be_visible()
	expect(text).to_have_value(first)
	check((CFG / DROPIN).read_text() == first, f'{DROPIN} on disk isn\'t the backup')
	check(not [a for a in ui(page)['actions'] if a['id'] == 'stamp'], 'the action is still there after restoring the backup')

	# The main file, from "Nix": read-only, its reason said, no Save; the service refuses it too.
	page.goto(BASE + '/#/admin/files/elamond.toml')
	main = editor_of(page, 'elamond.toml')
	expect(main).to_have_value(re.compile(r'^#:schema|^# Settings', re.M))
	expect(main).not_to_be_editable()
	expect(page.get_by_text(re.compile(r'^Read-only: .+\.$'))).to_be_visible()
	expect(page.get_by_role('button', name='Save', exact=True)).to_be_hidden()
	shot(page, '16-files-main-read-only')
	f = api(page, '/api/v1/admin/file?name=elamond.toml')
	check(not f['writable'] and f['why'], f'elamond.toml is writable from the page: {f}')
	r = fetch(page, '/api/v1/admin/file/save', 'POST', {'name': 'elamond.toml', 'text': f['text'] + '\n', 'base': f['hash']})
	check(r['status'] == 403 and r['json'] and r['json'].get('error') == 'read_only', f'saving elamond.toml answered {r["status"]} {r["text"]}')


def test_control(T):
	"""Control held off from the page; a save is then refused (writes_off); `elamond control on`
	at the computer, and the save goes."""
	page = T.page(allow=[(403, r'^/api/v1/admin/file/save$')])
	page.goto(BASE + '/#/admin/control')
	expect(page.get_by_text('Control is on.')).to_be_visible()
	try:
		page.get_by_role('button', name='Turn control off').click()
		sure = page.get_by_role('dialog').filter(has_text='Turn control off?')
		sure.get_by_role('button', name='Turn control off').click()
		expect(page.get_by_text(re.compile(r'Control is off: held off \(elamond control off\), since '))).to_be_visible()
		expect(page.get_by_text('elamond control on', exact=True)).to_be_visible()
		shot(page, '17-control-off')
		state = api(page, '/api/v1/admin/state')
		check(not state['control']['enabled'] and state['control']['off_reason'] == 'held_off', f'control isn\'t held off: {state["control"]}')

		page.goto(BASE + f'/#/admin/files/{DROPIN.replace("/", "%2F")}')
		expect(page.get_by_text('Control is held off at the computer: nothing can be saved until it is on again there (elamond control on).')).to_be_visible()
		text = editor_of(page, DROPIN)
		expect(text).to_be_editable()
		before = (CFG / DROPIN).read_text()
		text.fill(before + '# held off\n')
		page.get_by_role('button', name='Save', exact=True).click()
		expect(page.get_by_text(re.compile(r'^Control is held off at the computer \(elamond control off\): nothing is written'))).to_be_visible()
		check((CFG / DROPIN).read_text() == before, f'{DROPIN} was written with control held off')
		shot(page, '18-control-save-refused')

		elamond('control', 'on')
		until(page, lambda: api(page, '/api/v1/admin/state')['control']['enabled'], 'control on after elamond control on')
		page.get_by_role('button', name='Save', exact=True).click()
		expect(page.get_by_text(re.compile(r'^Saved; the service took the settings up')).first).to_be_visible()
		check((CFG / DROPIN).read_text() == before + '# held off\n', f'{DROPIN} wasn\'t written once control was on')
	finally:
		# Never left held off for the scenarios after, whatever failed.
		elamond('control', 'on')


def cells(page):
	"""The editor's widgets, as its canvas names them, in order."""
	return page.get_by_role('list').first.get_by_role('listitem').evaluate_all('els => els.map((e) => e.getAttribute("aria-label"))')


def add_widget(page, label):
	page.get_by_role('button', name='Add widget').click()
	dialog = page.get_by_role('dialog', name='Add a widget')
	expect(dialog).to_be_visible()
	dialog.get_by_role('button', name=re.compile(rf'^{label}\s+{label.lower()}\b')).click()
	expect(dialog).to_be_hidden()


def test_editor(T):
	"""A new dashboard of a gauge, a value and a button for the confirm action, made from forms:
	reordered and widened by keyboard, a tab of this browser's, saved and shown in that order. A
	standard dashboard's title changed, and back to the files. The new one deleted."""
	page = T.page(allow=[(404, r'^/api/v1/admin/preview$'), (422, r'^/api/v1/admin/preview$')], viewport={'width': 1440, 'height': 1000})
	page.goto(BASE + '/#/edit')
	expect(page.get_by_role('heading', name='New dashboard')).to_be_visible()
	page.get_by_label('Id', exact=True).fill('made')
	page.get_by_label('Title', exact=True).fill('Made in the page')
	tab = page.get_by_label(re.compile(r'^Add it to the tabs of '))
	tab.check()
	page.get_by_role('button', name='Make it').click()
	expect(page.get_by_role('heading', name=re.compile(r'^New: Made in the page'))).to_be_visible()

	add_widget(page, 'Gauge')
	add_widget(page, 'Value')
	add_widget(page, 'Button')
	# The button's form is open: it runs the confirm action.
	action = page.get_by_role('combobox', name=re.compile(r'^Action'))
	action.fill('greet')
	option = page.get_by_role('option', name=re.compile(r'^greet\b'))
	if option.count():
		option.first.dispatch_event('pointerdown')
	expect(action).to_have_value('greet')
	names = cells(page)
	check([n.split(',')[0] for n in names] == ['CPU', 'Uptime', 'Run'], f'the widgets added are {names}')

	# By keyboard: the button up twice, the value one column wider.
	page.get_by_role('listitem', name=re.compile(r'^Run, ')).focus()
	page.keyboard.press('Alt+ArrowUp')
	page.keyboard.press('Alt+ArrowUp')
	until(page, lambda: [n.split(',')[0] for n in cells(page)] == ['Run', 'CPU', 'Uptime'], 'the button moved first by Alt+Up')
	uptime = page.get_by_role('listitem', name=re.compile(r'^Uptime, '))
	before = uptime.get_attribute('aria-label')
	uptime.focus()
	page.keyboard.press('Alt+ArrowRight')
	expect(page.get_by_role('listitem', name=re.compile(r'^Uptime, .*, 3 wide$'))).to_be_visible()
	check(before.endswith(', 2 wide'), f'the value started {before}')
	# The tab of this browser's profile, from the form before: the last of its tabs, and the one change
	# its profile's form has.
	tabs = page.get_by_role('combobox', name=re.compile(r'^Dashboards, item \d+$'), include_hidden=True)
	expect(tabs.last).to_have_value('made')
	# The service's preview of the draft, drawn in its cells with the readings now.
	expect(page.get_by_role('listitem', name=re.compile(r'^CPU, '))).to_contain_text(re.compile(r'\d+\s*%'))
	expect(page.get_by_role('listitem', name=re.compile(r'^Uptime, ')).get_by_role('region', name='Uptime', exact=True)).to_contain_text(re.compile(r'\d+ (d|h|min|s)\b'))
	shot(page, '19-editor-new')
	page.get_by_role('button', name='Save', exact=True).click()
	expect(page.get_by_text(re.compile(r'^Saved; the service took it up \(generation \d+\)\.$'))).to_be_visible()

	u = ui(page)
	check('made' in u['tabs'], f'the new dashboard isn\'t among the tabs: {u["tabs"]}')
	made = [d for d in u['dashboards'] if d['id'] == 'made'][0]
	types = [w['type'] for w in made['widgets']]
	check(types == ['button', 'gauge', 'value'], f'the widgets in ui are {types}')
	check(made['widgets'][0].get('action') == 'greet', f'the button runs {made["widgets"][0].get("action")}')
	check(made['widgets'][2].get('span') == 3, f'the value is {made["widgets"][2].get("span")} wide')
	editor_file = (CFG / 'elamond.d/zz-editor.toml').read_text()
	check('[dashboards.made]' in editor_file, f'zz-editor.toml has no [dashboards.made]: {editor_file}')

	page.goto(BASE + '/#/')
	page.get_by_role('tab', name='Made in the page').click()
	dash = page.get_by_role('region', name='Made in the page')
	expect(dash).to_be_visible()
	expect(dash.get_by_role('button', name='Run', exact=True)).to_be_visible()
	expect(dash.get_by_role('meter', name='CPU')).to_have_attribute('aria-valuetext', re.compile(r'\d'))
	order = dash.evaluate("el => [...el.querySelectorAll('.w')].map((w) => [...w.classList].find((c) => /^w-/.test(c) && c !== 'w-body'))")
	check(order == ['w-button', 'w-gauge', 'w-value'], f'the dashboard draws {order}')
	shot(page, '20-editor-saved-dashboard')

	# A standard dashboard's title, overridden, and back to the files.
	page.goto(BASE + '/#/edit/sensors')
	expect(page.get_by_role('heading', name=re.compile(r'^Edit: Sensors'))).to_be_visible()
	title = page.get_by_role('textbox', name=re.compile(r'^Title'))
	expect(title).to_have_value('Sensors')
	title.fill('Sensors, edited')
	page.get_by_role('button', name='Save', exact=True).click()
	expect(page.get_by_text(re.compile(r'^Saved; the service took it up'))).to_be_visible()
	check([d['title'] for d in ui(page)['dashboards'] if d['id'] == 'sensors'] == ['Sensors, edited'], 'the title wasn\'t taken up')
	expect(page.get_by_text(re.compile(r"^From: elamond\.d/dashboards\.toml:\d+ · the editor's file\. The editor's version overrides it\.$"))).to_be_visible()
	page.get_by_role('button', name='Back to the files').click()
	sure = page.get_by_role('dialog', name='Back to the files?')
	sure.get_by_role('button', name='Back to the files').click()
	expect(page.get_by_text(re.compile(r'^Back to the files; the service took it up'))).to_be_visible()
	expect(title).to_have_value('Sensors')
	check([d['title'] for d in ui(page)['dashboards'] if d['id'] == 'sensors'] == ['Sensors'], 'the standard title isn\'t back')
	page.goto(BASE + '/#/')
	expect(page.get_by_role('tab', name='Sensors', exact=True)).to_be_visible()

	# The new one deleted, and its tab with it.
	page.goto(BASE + '/#/edit/made')
	page.get_by_role('button', name='Delete', exact=True).click()
	sure = page.get_by_role('dialog', name='Delete this dashboard?')
	sure.get_by_role('button', name='Delete').click()
	expect(page.get_by_role('tablist', name='Dashboards')).to_be_visible()
	expect(page.get_by_role('tab', name='Made in the page')).to_have_count(0)
	u = ui(page)
	check('made' not in u['tabs'] and not [d for d in u['dashboards'] if d['id'] == 'made'], 'the deleted dashboard is still there')


def test_forget(T):
	"""Forget this browser: back to pairing, and its old cookie is refused."""
	page = T.page(allow=[(401, r'^/api/v1/(ui|stream(/.*)?)$')])
	# The same cookie in another context, let in before.
	old = T.page()
	url = BASE + f'/api/v1/ui?widgets={WIDGETS}'
	client = {'Elamon-Client': 'web/' + T.hello['version']}
	r = old.request.get(url, headers=client)
	check(r.status == 200, f'the cookie got {r.status} before forgetting')
	page.goto(BASE + '/#/')
	expect(page.get_by_role('tablist', name='Dashboards')).to_be_visible()
	page.get_by_role('button', name='More').click()
	page.get_by_role('menuitem', name='Forget this browser').click()
	sure = page.get_by_role('dialog', name='Forget this browser?')
	expect(sure).to_be_visible()
	sure.get_by_role('button', name='Forget this browser').click()
	expect(page.get_by_role('heading', name='Pair this browser')).to_be_visible()
	shot(page, '21-forgotten')
	check(not [c for c in page.context.cookies() if is_cookie(c)], 'the cookie is still there after forgetting')
	# The cookie it had, from the other context: refused, and cleared.
	r = old.request.get(url, headers=client)
	check(r.status == 401 and r.json().get('error') == 'unauthorized', f'the old cookie got {r.status} {r.text()}')
	cleared = [h['value'] for h in r.headers_array if h['name'].lower() == 'set-cookie']
	check(any(v.startswith('__Host-elamon') and 'Max-Age=0' in v for v in cleared), f'the 401 doesn\'t clear the cookie: {cleared}')
	listed = elamond('clients')
	check('web/' not in listed, f'elamond clients still lists a browser: {listed}')


SCENARIOS = [test_pair, test_blocked, test_panels, test_theme, test_layout, test_shell, test_action, test_admin, test_files, test_control, test_editor, test_forget]


def main():
	wanted = set(sys.argv[1:])
	known = {f.__name__.removeprefix('test_') for f in SCENARIOS}
	if wanted - known:
		print(f'no scenario {", ".join(sorted(wanted - known))}; there are {", ".join(f.__name__.removeprefix("test_") for f in SCENARIOS)}', file=sys.stderr)
		return 2
	results = []
	with sync_playwright() as p:
		# Chromium as nixpkgs has it, headless (its own headless shell isn't there).
		browser = p.chromium.launch(channel='chromium')
		T = Run(browser)
		for fn in SCENARIOS:
			name = fn.__name__.removeprefix('test_')
			if wanted and name not in wanted and name != 'pair':
				continue
			t0 = time.monotonic()
			why = None
			try:
				fn(T)
			except Exception as e:
				why = f'{type(e).__name__}: {e}'
				traceback.print_exc()
			problems = T.finish(why is not None, name)
			if problems and why is None:
				why = 'unexpected errors in the page:\n    ' + '\n    '.join(problems)
			elif problems:
				why += '\n  and in the page:\n    ' + '\n    '.join(problems)
			results.append((name, why, time.monotonic() - t0))
			print(f'{"PASS" if why is None else "FAIL"} {name} ({time.monotonic() - t0:.1f} s)', flush=True)
		browser.close()
	print()
	for name, why, secs in results:
		print(f'{"PASS" if why is None else "FAIL"}  {name:8} {secs:5.1f} s' + ('' if why is None else f'\n  {why}'))
	failed = [r for r in results if r[1] is not None]
	print(f'\n{len(results) - len(failed)} passed, {len(failed)} failed; screenshots in {SHOTS}')
	return 1 if failed else 0


if __name__ == '__main__':
	sys.exit(main())
