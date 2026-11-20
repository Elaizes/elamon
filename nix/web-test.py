import json
import re
import shlex

machine.wait_for_unit("multi-user.target")
machine.wait_for_unit("web-cert.service")

# The page as a browser reaches it: Firefox on Linux, saying it is the page 0.11, keeping its
# cookies in a jar as a browser keeps them. A phone says it is the app 0.11 and has no jar.
HOST = "localhost:47823"
ORIGIN = f"-H 'Origin: https://{HOST}'"
UA = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0"
JAR = "/tmp/browser.cookies"
BROWSER = f"-H 'Elamon-Client: web/0.11.0' -A '{UA}' -b {JAR} -c {JAR}"
PHONE = "-H 'Elamon-Client: android/0.11.0'"

# The settings the module writes, which login shells name for the commands; systemd-run's don't.
ELAMON_CONFIG = machine.succeed("grep -oh '/nix/store/[a-z0-9]*-elamond.toml' /etc/set-environment /etc/systemd/user/elamond.service | head -1").strip()
assert ELAMON_CONFIG.startswith("/nix/store/"), ELAMON_CONFIG
AS_ALICE = f"--uid=alice -p Environment=HOME=/home/alice -p Environment=ELAMON_CONFIG={ELAMON_CONFIG}"
# The main file as the admin API names it: the name of the one ELAMON_CONFIG names.
MAIN = ELAMON_CONFIG.rsplit("/", 1)[1]
DROPINS = "/home/alice/.config/elamon/elamond.d"
CERTS = "/var/lib/certs"

def request(method, path, who, body=None, extra=""):
	"""The status, the headers (by name in lowercase, a list each) and the body of a request."""
	data = f"-H 'Content-Type: application/json' --data-binary {shlex.quote(json.dumps(body))}" if body is not None else ""
	out = machine.succeed(f"curl -sk -D - -X {method} {who} {extra} {data} -w '\\n%{{http_code}}' 'https://{HOST}{path}'")
	head, _, rest = out.partition("\r\n\r\n")
	text, status = rest.rsplit("\n", 1)
	headers = {}
	for line in head.split("\r\n")[1:]:
		name, _, value = line.partition(":")
		headers.setdefault(name.strip().lower(), []).append(value.strip())
	return int(status), headers, text

def call(method, path, who, body=None, extra=""):
	"""The status, the headers and the JSON body of a request to the API."""
	status, headers, text = request(method, path, who, body, extra)
	return status, headers, json.loads(text) if text.strip() else None

def answer(method, path, who, body=None, extra=""):
	"""The status and the JSON body of a request to the API."""
	status, _, doc = call(method, path, who, body, extra)
	return status, doc

def ok(method, path, body=None):
	"""The answer to a request of the browser's from its page, which must be 200."""
	status, doc = answer(method, path, BROWSER, body, ORIGIN)
	assert status == 200, (method, path, status, doc)
	return doc

def ui():
	return ok("GET", "/api/v1/ui")

def dashboard(board):
	return next(d for d in ui()["dashboards"] if d["id"] == board)

def state():
	return ok("GET", "/api/v1/admin/state")

def editor():
	return ok("GET", "/api/v1/admin/editor")

def save(name, text, base):
	return answer("POST", "/api/v1/admin/file/save", BROWSER, {"name": name, "text": text, "base": base}, ORIGIN)

def edit(what, body):
	return answer("POST", f"/api/v1/admin/editor/{what}", BROWSER, body, ORIGIN)

def sha(path):
	return machine.succeed(f"sha256sum {path} | cut -d' ' -f1").strip()

def presented(*args):
	"""The SHA-256 of the certificate the service presents to `openssl s_client` connecting to
	127.0.0.1 with `args` (`-servername NAME` for SNI; an address alone sends none)."""
	return machine.succeed(f"openssl s_client -connect 127.0.0.1:47823 {' '.join(args)} </dev/null 2>/dev/null | openssl x509 -outform DER | sha256sum | cut -d' ' -f1").strip()

def as_alice(command, check=True):
	"""What `command` prints run as alice; with `check`, it must succeed."""
	run = f"systemd-run {AS_ALICE} --wait --pipe --quiet {command}"
	return machine.succeed(run) if check else machine.execute(run)[1]

def pair_code(unit, *args):
	"""Runs `elamond pair --yes` as alice in a unit of its own and returns the code it prints."""
	machine.succeed(f"systemd-run {AS_ALICE} --unit={unit} /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair --yes {' '.join(args)}")
	pattern = "'^ +[0-9]{3} [0-9]{3} *$'"
	machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -Ec {pattern}", timeout=30)
	return machine.succeed(f"journalctl -u {unit} -o cat | grep -E {pattern} | sed -n 1p | tr -d ' \\n'")

with subtest("the page is served with the headers that keep it to itself, and hello's fingerprint is the certificate's"):
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	status, h, html = request("GET", "/", "")
	assert status == 200 and h["content-type"] == ["text/html; charset=utf-8"] and "<html" in html.lower(), (status, h)
	csp = h["content-security-policy"]
	assert len(csp) == 1, csp
	policy = csp[0].split("; ")
	assert "script-src 'self'" in policy and "frame-ancestors 'none'" in policy and "default-src 'self'" in policy, policy
	assert not any("unsafe" in p for p in policy), policy
	assert h["x-content-type-options"] == ["nosniff"] and h["referrer-policy"] == ["no-referrer"], h
	assert h["cache-control"] == ["no-cache"], h
	etag = h["etag"][0]
	assert re.fullmatch(r'"[0-9a-f]{16}"', etag), etag
	# Asked again with its tag: not modified, and nothing sent.
	status, h, html = request("GET", "/", "", extra=f"-H {shlex.quote('If-None-Match: ' + etag)}")
	assert (status, html) == (304, "") and h["etag"] == [etag], (status, h)
	# Its scripts under the same policy; nothing else of web/ (its tests) outside /api/.
	status, h, _ = request("GET", "/web/js/main.js", "")
	assert status == 200 and h["content-type"][0].startswith("text/javascript") and h["content-security-policy"] == csp, (status, h)
	assert answer("GET", "/web/test/mock.mjs", "") == (404, {"error": "not_found"})
	# hello needs no token; an answer of the API is never kept.
	status, h, hello = call("GET", "/api/v1/hello", "")
	assert status == 200 and hello["api"] == 2 and hello["version"] == "0.11.1", hello
	assert h["cache-control"] == ["no-store"] and h["x-content-type-options"] == ["nosniff"], h
	# The certificate a client gets by the address, as the app connects: what hello says.
	own = presented()
	assert re.fullmatch("[0-9a-f]{64}", hello["fingerprint"]) and hello["fingerprint"] == own, (hello, own)

# A browser's cookie is named after the service it pairs with: the first 8 hex characters of its id.
COOKIE = f"__Host-elamon-{hello['id'][:8]}"
SET_COOKIE = re.compile(rf"{re.escape(COOKIE)}=([A-Za-z0-9_-]+); Max-Age=2592000; Path=/; Secure; HttpOnly; SameSite=Strict")

with subtest("a browser pairs as owner with the code of elamond pair: by a cookie its script never sees, from its own page alone"):
	code = pair_code("pair-web", "--role", "owner")
	pair = {"code": code, "device": ""}
	# Without Origin (nor Sec-Fetch-Site), or from another site's page: refused, the code still good.
	assert answer("POST", "/api/v1/pair", BROWSER, pair) == (403, {"error": "bad_origin"})
	assert answer("POST", "/api/v1/pair", BROWSER, pair, "-H 'Origin: https://other.example.net:47823'") == (403, {"error": "bad_origin"})
	assert answer("POST", "/api/v1/pair", BROWSER, pair, f"-H 'Origin: http://{HOST}'") == (403, {"error": "bad_origin"})
	status, h, paired = call("POST", "/api/v1/pair", BROWSER, pair, ORIGIN)
	assert status == 200 and paired["role"] == "owner" and paired["profile"] == "firefox-on-linux", (status, paired)
	assert "token" not in paired and paired["id"] == hello["id"], paired
	# Where the computer is: the names of the settings, but the one the real certificate is for.
	assert "elamon.example.net" in paired["endpoints"] and "desk.example.net" not in paired["endpoints"], paired
	# elamond pair offers it alone to phones, and the page by that name to a browser.
	said = machine.succeed("journalctl -u pair-web -o cat")
	assert re.search(r"^ *Names: +elamon\.example\.net$", said, re.M), said
	assert re.search(r"^ *In a browser: +https://desk\.example\.net:47823/ ", said, re.M), said
	browser_id = paired["client_id"]
	assert len(h["set-cookie"]) == 1, h
	m = SET_COOKIE.fullmatch(h["set-cookie"][0])
	assert m, h["set-cookie"]
	token = m.group(1)
	# curl kept it as a browser does: for this host alone, HttpOnly, sent over HTTPS only.
	jar = machine.succeed(f"grep -F {COOKIE} {JAR}").strip().split("\t")
	assert jar[0] == "#HttpOnly_localhost" and jar[1:4] == ["FALSE", "/", "TRUE"] and jar[5:] == [COOKIE, token], jar
	machine.wait_until_succeeds("journalctl -u pair-web -o cat | grep -c 'Paired with \"Firefox on Linux\"'", timeout=30)

with subtest("by its cookie the page reads ui with admin and editor, and changes nothing but from itself"):
	status, h, doc = call("GET", "/api/v1/ui", BROWSER)
	assert status == 200 and doc["role"] == "owner" and doc["profile"]["id"] == "firefox-on-linux", (status, doc)
	features = doc["service"]["features"]
	assert "admin" in features and "editor" in features, features
	# Each ui by the cookie gives it again: a page in use stays paired.
	assert h["set-cookie"] == [f"{COOKIE}={token}; Max-Age=2592000; Path=/; Secure; HttpOnly; SameSite=Strict"], h
	machine.wait_until_succeeds(f"systemd-run {AS_ALICE} --wait --pipe --quiet /run/current-system/sw/bin/elamond clients | grep -E '^{browser_id} +owner +firefox-on-linux +Firefox on Linux +web 0\\.11\\.0 '", timeout=30)
	# A change by the cookie from anywhere but the page: refused, and nothing written.
	page = {"name": "elamond.d/page.toml", "text": '[links.notes]\nname = "Notes"\nurl = "http://{host}:8080"\n', "base": None}
	for elsewhere in ["", "-H 'Origin: https://other.example.net:47823'", "-H 'Origin: null'", "-H 'Sec-Fetch-Site: cross-site'", "-H 'Sec-Fetch-Site: same-site'"]:
		assert answer("POST", "/api/v1/admin/file/save", BROWSER, page, elsewhere) == (403, {"error": "bad_origin"}), elsewhere
	assert answer("POST", "/api/v1/admin/control", BROWSER, {"on": False}) == (403, {"error": "bad_origin"})
	assert ui()["control"] == {"enabled": True, "off_reason": None}
	machine.fail(f"test -e {DROPINS}/page.toml")
	# Without Origin, Sec-Fetch-Site: same-origin is the page's own too.
	status, checked = answer("POST", "/api/v1/admin/file/check", BROWSER, {"name": page["name"], "text": page["text"]}, "-H 'Sec-Fetch-Site: same-origin'")
	assert status == 200 and checked["sound"] is True and checked["diff"][0] == "@@ -0,0 +1,3 @@", (status, checked)
	# From the page: written, 600 as a new drop-in is.
	saved = ok("POST", "/api/v1/admin/file/save", page)
	assert saved["ok"] is True and saved["backup"] is None and saved["hash"] == sha(f"{DROPINS}/page.toml")[:16], saved
	machine.succeed(f"test $(stat -c %a:%U {DROPINS}/page.toml) = 600:alice")
	assert any(link["name"] == "Notes" for link in ok("GET", "/api/v1/admin/settings")["settings"]["links"].values())

with subtest("a phone paired as admin is kept out of the admin API, and the cookie's token is no phone's Bearer"):
	code = pair_code("pair-phone")
	# The page sees the code waiting: the app's link without the name the real certificate is for,
	# the page's by that name, as elamond pair shows them.
	pending = ok("GET", "/api/v1/admin/pairing")
	assert pending["code"] == code and pending["from_page"] is False and pending["ask"] is None, pending
	app_link = pending["app"]["link"]
	assert "elamon.example.net" in app_link and "desk.example.net" not in app_link, app_link
	assert pending["web"]["link"] == f"https://desk.example.net:47823/#pair={code}", pending["web"]
	status, phone = answer("POST", "/api/v1/pair", PHONE, {"code": code, "device": "Pixel 9"})
	assert status == 200 and phone["role"] == "admin" and phone["token"], (status, phone)
	assert "elamon.example.net" in phone["endpoints"] and "desk.example.net" not in phone["endpoints"], phone
	bearer = f"{PHONE} -H 'Authorization: Bearer {phone['token']}'"
	assert answer("GET", "/api/v1/admin/state", bearer) == (403, {"error": "not_allowed"})
	assert answer("POST", "/api/v1/admin/file/save", bearer, dict(page, name="elamond.d/phone.toml")) == (403, {"error": "not_allowed"})
	machine.fail(f"test -e {DROPINS}/phone.toml")
	features = answer("GET", "/api/v1/ui", bearer)[1]["service"]["features"]
	assert "admin" not in features and "editor" not in features, features
	# The browser's token as a Bearer: no client is let in so.
	assert answer("GET", "/api/v1/ui", f"{PHONE} -H 'Authorization: Bearer {token}'") == (401, {"error": "unauthorized"})
	assert answer("GET", "/api/v1/admin/state", f"{PHONE} -H 'Authorization: Bearer {token}'") == (401, {"error": "unauthorized"})
	# The phone's token as a cookie: no, and the cookie cleared.
	status, h, refused = call("GET", "/api/v1/ui", f"-H 'Elamon-Client: web/0.11.0' -H 'Cookie: {COOKIE}={phone['token']}'")
	assert (status, refused) == (401, {"error": "unauthorized"}), (status, refused)
	assert h["set-cookie"][0].startswith(f"{COOKIE}=; Max-Age=0;"), h
	# The owner's page reads the state, and the phone's refusal in the activity log.
	assert state()["you"] == {"id": browser_id, "role": "owner"}
	entries = ok("GET", "/api/v1/admin/activity")["entries"]
	assert any(e["phone"] == "Pixel 9" and e["what"] == "admin: GET /api/v1/admin/state" and e["result"] == "not_allowed" for e in entries), entries

MINE = f"{DROPINS}/mine.toml"

with subtest("a drop-in saved from the page is in ui at once; one that is broken is refused with its file and line, and not written"):
	assert "mine" not in ui()["tabs"]
	generation = state()["settings"]["generation"]
	good = '[dashboards.mine]\ntitle = "Mine"\nwidgets = [{ type = "gauge", value = "cpu.usage", label = "CPU", max = 100 }]\n\n[profiles.firefox-on-linux]\ndashboards = ["overview", "mine"]\n'
	status, saved = save("elamond.d/mine.toml", good, None)
	assert status == 200 and saved["generation"] > generation, (status, saved)
	# Taken up at once: the dashboard, and the tabs of the browser's own profile.
	doc = ui()
	assert doc["tabs"] == ["overview", "mine"], doc["tabs"]
	mine = next(d for d in doc["dashboards"] if d["id"] == "mine")
	assert mine["title"] == "Mine" and [w["type"] for w in mine["widgets"]] == ["gauge"], mine
	# Saved over a text it read before another write: refused, with the file's hash now.
	assert save("elamond.d/mine.toml", good + "# again\n", "0000000000000000") == (409, {"error": "changed_on_disk", "hash": saved["hash"]})
	# Broken: refused with its file and line, the file as it was.
	before = sha(MINE)
	status, refused = save("elamond.d/mine.toml", good.replace('value = "cpu.usage"', 'value = "cpu.usge"'), saved["hash"])
	assert status == 422 and refused["error"] == "bad_settings", (status, refused)
	mistake = refused["mistakes"][0]
	assert (mistake["file"], mistake["line"], mistake["path"]) == ("elamond.d/mine.toml", 3, "dashboards.mine.widgets[0].value"), refused
	assert mistake["message"] == 'no value "cpu.usge"; did you mean cpu.usage?', refused
	assert sha(MINE) == before
	# A new drop-in that's broken isn't made at all.
	status, refused = save("elamond.d/broken.toml", '[dashboards.broken]\ntitle = "Broken"\nwidgets = [{ type = "gauge", value = "cpu.usge" }]\n', None)
	assert status == 422 and (refused["mistakes"][0]["file"], refused["mistakes"][0]["line"]) == ("elamond.d/broken.toml", 3), (status, refused)
	machine.fail(f"test -e {DROPINS}/broken.toml")
	# The service goes on with the settings as they were.
	assert ui()["tabs"] == ["overview", "mine"]
	assert "settings: they don't parse" not in machine.succeed("journalctl _UID=1000 -o cat")

EDITOR = f"{DROPINS}/zz-editor.toml"

with subtest("the dashboard editor: a dashboard of its own in ui, a default one's title over the files and back, and never a command"):
	d = editor()
	assert (d["file"], d["hash"], d["writable"], d["profile"]) == ("elamond.d/zz-editor.toml", None, True, "firefox-on-linux"), d
	desk = {"title": "Desk", "icon": "gauge", "widgets": [
		{"type": "gauge", "label": "CPU", "value": "cpu.usage", "max": 100},
		{"type": "value", "label": "Memory", "value": "mem.percent"},
	]}
	status, w = edit("dashboard", {"id": "desk", "dashboard": desk, "base": None})
	assert status == 200 and w["ok"] is True, (status, w)
	text = machine.succeed(f"cat {EDITOR}")
	assert text.startswith("# Written by Elamon's dashboard editor") and "\n[dashboards.desk]\n" in text and text.count("[[dashboards.desk.widgets]]") == 2, text
	machine.succeed(f"test $(stat -c %a:%U {EDITOR}) = 600:alice")
	described = next(x for x in editor()["dashboards"] if x["id"] == "desk")
	assert described["editor"] is True and described["own"] is True and described["title"] == "Desk", described
	# Its tab, with the part of the profile the editor writes, over the drop-in's: in ui.
	status, w = edit("profile", {"id": "firefox-on-linux", "profile": {"dashboards": ["overview", "mine", "desk"]}, "base": w["hash"]})
	assert status == 200, (status, w)
	assert ui()["tabs"] == ["overview", "mine", "desk"]
	board = dashboard("desk")
	assert board["title"] == "Desk" and [x["type"] for x in board["widgets"]] == ["gauge", "value"], board
	# A standard dashboard's title over what the main file says, and back.
	d = editor()
	overview = next(x for x in d["dashboards"] if x["id"] == "overview")
	assert overview["editor"] is False and overview["title"] == "Overview", overview
	mine_overview = dict(overview["def"], title="My overview")
	status, w = edit("dashboard", {"id": "overview", "dashboard": mine_overview, "base": d["hash"]})
	assert status == 200, (status, w)
	assert dashboard("overview")["title"] == "My overview"
	# Its icon left out: the main file sets it, so the editor may change it but not take it away.
	before = sha(EDITOR)
	status, refused = edit("dashboard", {"id": "overview", "dashboard": {k: v for k, v in mine_overview.items() if k != "icon"}, "base": w["hash"]})
	assert status == 422 and (refused["mistakes"][0]["path"], refused["mistakes"][0]["file"]) == ("dashboards.overview.icon", MAIN), (status, refused)
	assert isinstance(refused["mistakes"][0]["line"], int) and sha(EDITOR) == before, refused
	status, w = edit("revert", {"dashboard": "overview", "base": w["hash"]})
	assert status == 200, (status, w)
	assert dashboard("overview")["title"] == "Overview"
	text = machine.succeed(f"cat {EDITOR}")
	assert "[dashboards.overview]" not in text and "[dashboards.desk]" in text, text
	# A command, on the dashboard or on a widget: refused, nothing written, nothing run.
	before = sha(EDITOR)
	run = ["/run/current-system/sw/bin/touch", "/tmp/ran"]
	status, refused = edit("dashboard", {"id": "desk", "dashboard": dict(desk, command=run), "base": w["hash"]})
	assert status == 422 and refused["error"] == "bad_settings" and refused["mistakes"][0]["path"] == "dashboards.desk.command", (status, refused)
	status, refused = edit("dashboard", {"id": "desk", "dashboard": dict(desk, widgets=[{"type": "button", "label": "Run", "command": run}]), "base": w["hash"]})
	assert status == 422 and refused["error"] == "bad_settings", (status, refused)
	assert all(m["path"].startswith("dashboards.desk.widgets[0]") for m in refused["mistakes"]), refused
	assert sha(EDITOR) == before
	machine.fail(f"grep -q command {EDITOR}")
	machine.fail("test -e /tmp/ran")

with subtest("the main file from the Nix store is only read"):
	before = sha(ELAMON_CONFIG)
	files = state()["files"]
	main = files[0]
	assert (main["name"], main["kind"], main["writable"]) == (MAIN, "main", False), files
	assert "Nix" in main["why"], main
	assert [f["name"] for f in files[1:]] == ["elamond.d/mine.toml", "elamond.d/page.toml", "elamond.d/phones.toml", "elamond.d/zz-editor.toml"], files
	status, read = answer("GET", f"/api/v1/admin/file?name={MAIN}", BROWSER)
	assert status == 200 and read["kind"] == "main" and read["writable"] is False and read["text"] == machine.succeed(f"cat {ELAMON_CONFIG}"), (status, read)
	status, refused = save(MAIN, read["text"] + "port = 47900\n", read["hash"])
	assert status == 403 and refused["error"] == "read_only" and "Nix" in refused["message"], (status, refused)
	# Under ELAMON_CONFIG elamond.toml is no file of the set: no name the admin API takes.
	assert save("elamond.toml", "schema = 3\n", None) == (400, {"error": "bad_name"})
	machine.fail("test -e /home/alice/.config/elamon/elamond.toml")
	assert sha(ELAMON_CONFIG) == before
	# The editor says which files are from Nix: a standard dashboard's is.
	cpu = next(x for x in editor()["dashboards"] if x["id"] == "cpu")
	assert [(f["file"], f["nix"]) for f in cpu["from"]] == [(MAIN, True)], cpu["from"]

with subtest("elamond control off stops the admin API's writes, and only elamond control on at the computer lets go of it"):
	text = machine.succeed(f"cat {MINE}")
	base = sha(MINE)[:16]
	as_alice("/run/current-system/sw/bin/elamond control off")
	assert save("elamond.d/mine.toml", text + "# held off\n", base) == (403, {"error": "writes_off", "message": "held_off"})
	assert edit("dashboard", {"id": "desk", "dashboard": desk, "base": sha(EDITOR)[:16]}) == (403, {"error": "writes_off", "message": "held_off"})
	# The page may hold control off, never let go of it.
	assert answer("POST", "/api/v1/admin/control", BROWSER, {"on": True}, ORIGIN) == (403, {"error": "at_computer"})
	# The service looks at the hold as often as at the settings files: within seconds.
	machine.wait_until_succeeds(f"curl -sk {BROWSER} https://{HOST}/api/v1/admin/state | jq -e '.control.enabled == false and .control.off_reason == \"held_off\" and .control.held_off_since > 0'", timeout=15)
	assert sha(MINE)[:16] == base
	as_alice("/run/current-system/sw/bin/elamond control on")
	status, saved = save("elamond.d/mine.toml", text + "# on again\n", base)
	assert status == 200, (status, saved)
	# Held off from the page: the same, until the computer lets go.
	assert ok("POST", "/api/v1/admin/control", {"on": False}) == {"enabled": False, "off_reason": "held_off"}
	assert save("elamond.d/mine.toml", text, saved["hash"]) == (403, {"error": "writes_off", "message": "held_off"})
	assert "held off" in as_alice("/run/current-system/sw/bin/elamond control status")
	assert answer("POST", "/api/v1/admin/control", BROWSER, {"on": True}, ORIGIN) == (403, {"error": "at_computer"})
	as_alice("/run/current-system/sw/bin/elamond control on")
	assert save("elamond.d/mine.toml", text, saved["hash"])[0] == 200
	machine.succeed("journalctl _UID=1000 -o cat | grep -c 'control: held off from the admin panel by \"Firefox on Linux\"'")
	# With control = false in the settings, no settings file is written from here (it may add a
	# command); the dashboard editor's writes, which can't, still are.
	machine.succeed(f"echo 'control = false' > {DROPINS}/off.toml && chown alice:users {DROPINS}/off.toml")
	machine.wait_until_succeeds(f"curl -sk {BROWSER} https://{HOST}/api/v1/ui | jq -e '.control.off_reason == \"settings\"'", timeout=30)
	text = machine.succeed(f"cat {MINE}")
	assert save("elamond.d/mine.toml", text + "# control off\n", sha(MINE)[:16]) == (403, {"error": "writes_off", "message": "control"})
	status, w = edit("dashboard", {"id": "desk", "dashboard": dict(desk, title="Desk, control off"), "base": sha(EDITOR)[:16]})
	assert status == 200 and dashboard("desk")["title"] == "Desk, control off", (status, w)
	machine.succeed(f"rm {DROPINS}/off.toml")
	machine.wait_until_succeeds(f"curl -sk {BROWSER} https://{HOST}/api/v1/ui | jq -e '.control.enabled'", timeout=30)
	assert save("elamond.d/mine.toml", text + "# control on\n", sha(MINE)[:16])[0] == 200

with subtest("a certificate from files goes to a browser that asks for its name, the service's own to every other client"):
	crt = f"{CERTS}/desk.example.net.crt"
	real = machine.succeed(f"openssl x509 -in {crt} -outform DER | sha256sum | cut -d' ' -f1").strip()
	end = machine.succeed(f"openssl x509 -enddate -noout -in {crt} | cut -d= -f2").strip()
	until = int(machine.succeed(f"date -u -d '{end}' +%s").strip())
	day = machine.succeed(f"date -u -d '{end}' +%F").strip()
	machine.wait_until_succeeds(f"journalctl _UID=1000 -o cat | grep -cF 'certificate: browsers that reach this computer as desk.example.net get the one from [web]: valid until {day}'", timeout=30)
	# By its name, trusting the test CA alone, as a browser that trusts it would: no -k.
	by_name = json.loads(machine.succeed(f"curl -sf --resolve desk.example.net:47823:127.0.0.1 --cacert {CERTS}/ca.pem https://desk.example.net:47823/api/v1/hello"))
	assert by_name["id"] == hello["id"], by_name
	assert presented("-servername desk.example.net") == real and real != own
	machine.succeed(f"curl -sf -o /dev/null --resolve desk.example.net:47823:127.0.0.1 --cacert {CERTS}/ca.pem https://desk.example.net:47823/")
	# By the address, by another name: the service's own, which hello names and the CA never signed.
	assert presented() == own == ok("GET", "/api/v1/hello")["fingerprint"]
	assert presented("-servername localhost") == own
	status, out = machine.execute(f"curl -sS --cacert {CERTS}/ca.pem https://127.0.0.1:47823/api/v1/hello 2>&1")
	assert status == 60, (status, out)
	# What elamond check and the admin API say of it.
	out = as_alice("/run/current-system/sw/bin/elamond check")
	assert f"Web certificate for desk.example.net: valid until {day} (browsers that reach the computer by that name get it; others the service's own)." in out, out
	assert state()["web"]["certificate"] == {"name": "desk.example.net", "ok": True, "until": until, "message": f"valid until {day}"}

with subtest("a key others may read isn't presented, and is once it is 600 again"):
	key = f"{CERTS}/desk.example.net.key"
	machine.succeed(f"chmod 644 {key}")
	machine.succeed("systemctl --user -M alice@ restart elamond")
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	machine.wait_until_succeeds(f"journalctl _UID=1000 -o cat | grep -cF 'certificate: the one from [web] for desk.example.net: {key} may be read by others: chmod 600 it'", timeout=30)
	assert presented("-servername desk.example.net") == own
	out = as_alice("/run/current-system/sw/bin/elamond check")
	assert f"  ! web certificate for desk.example.net: {key} may be read by others: chmod 600 it" in out, out
	certificate = state()["web"]["certificate"]
	assert certificate["ok"] is False and "chmod 600" in certificate["message"], certificate
	# 600 again: read again, as the files are when they change, and presented.
	machine.succeed(f"chmod 600 {key}")
	machine.wait_until_succeeds(f"test $(openssl s_client -connect 127.0.0.1:47823 -servername desk.example.net </dev/null 2>/dev/null | openssl x509 -outform DER | sha256sum | cut -d' ' -f1) = {real}", timeout=30)
	assert state()["web"]["certificate"]["ok"] is True
