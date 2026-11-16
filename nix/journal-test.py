import json

# Every request says which app it is, and which widget types it draws, as the 0.8 app does.
APP = "-H 'Elamon-Client: android/0.8.0'"
WIDGETS = "value,gauge,meter,status,graph,list,bars,table,button,toggle,select,slider,text,group,media,timeline,log"

machine.wait_for_unit("multi-user.target")
machine.wait_for_unit("token-echo.service")
machine.wait_for_open_port(8080)

# The settings the module writes, which login shells name for the commands; systemd-run's don't.
ELAMON_CONFIG = machine.succeed("grep -oh '/nix/store/[a-z0-9]*-elamond.toml' /etc/set-environment /etc/systemd/user/elamond.service | head -1").strip()
assert ELAMON_CONFIG.startswith("/nix/store/"), ELAMON_CONFIG
AS_ALICE = f"--uid=alice -p Environment=HOME=/home/alice -p Environment=ELAMON_CONFIG={ELAMON_CONFIG}"

# The web service's token, made at boot: looked for below everywhere elamond writes or answers.
TOKEN = machine.succeed("cat /home/alice/.config/elamon/app.token").strip()
assert TOKEN.startswith("tok-") and len(TOKEN) == 28, TOKEN
seen = []

def call(method, path, auth="", body=None):
	"""The status and the JSON body of a request to the service."""
	data = f"-H 'Content-Type: application/json' -d '{json.dumps(body)}'" if body is not None else ""
	out = machine.succeed(f"curl -sk -X {method} {APP} {auth} {data} -w '\\n%{{http_code}}' 'https://localhost:47823{path}'")
	text, status = out.rsplit("\n", 1)
	seen.append(text)
	return int(status), json.loads(text) if text.strip() else None

def ui(auth):
	status, doc = call("GET", f"/api/v1/ui?widgets={WIDGETS}", auth)
	assert status == 200, (status, doc)
	return doc

def snapshot(auth, dashboard):
	status, snap = call("GET", f"/api/v1/snapshot?dashboard={dashboard}", auth)
	assert status == 200 and snap["dashboard"] == dashboard, (status, snap)
	return snap["widgets"]

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

def stream_lines(path):
	"""A stream's lines so far, but the last when it was cut off mid-line."""
	lines = []
	for l in machine.succeed(f"cat {path}").splitlines():
		try:
			lines.append(json.loads(l))
		except json.JSONDecodeError:
			pass
	seen.extend(json.dumps(l) for l in lines)
	return lines

def watch(unit, auth, dashboard):
	"""A stream of `dashboard` held open by a unit of its own, written to /tmp/<unit>.out."""
	machine.succeed(f"systemd-run --unit={unit} /bin/sh -c \"/run/current-system/sw/bin/curl -sNk --max-time 120 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard={dashboard}&widgets={WIDGETS}' > /tmp/{unit}.out\"")
	machine.wait_until_succeeds(f"grep -q '\"type\":\"data\"' /tmp/{unit}.out", timeout=30)

def log_lines(unit, wid):
	"""Every line the stream's states of the log widget `wid` carried, and its notes."""
	lines, notes = [], set()
	for l in stream_lines(f"/tmp/{unit}.out"):
		state = l.get("widgets", {}).get(wid) if l.get("type") == "data" else None
		if state is None:
			continue
		if state.get("reset"):
			lines = []
		lines += state["lines"]
		if "note" in state:
			notes.add(state["note"])
	return lines, notes

def wait_for_line(unit, wid, text, timeout=30):
	for _ in range(timeout):
		lines, _ = log_lines(unit, wid)
		if any(text in l["text"] for l in lines):
			return lines
		machine.sleep(1)
	raise AssertionError(f"{text!r} never reached {wid}: {log_lines(unit, wid)}")

def echoed():
	"""How many requests the web service has had."""
	return int(machine.succeed("wc -l < /var/lib/token-echo/seen || echo 0").strip())

with subtest("the service says it speaks the widget dictionary 3"):
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	hello = json.loads(machine.succeed("curl -sk https://localhost:47823/api/v1/hello"))
	assert hello["api"] == 2 and hello["widgets"] == 3, hello

with subtest("a phone pairs and sees the values of a command, a file and a web service"):
	code = pair_code("pair", "--profile", "vm")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Test phone"})
	assert status == 200 and paired["role"] == "admin", paired
	auth = f"-H 'Authorization: Bearer {paired['token']}'"
	doc = ui(auth)
	assert doc["tabs"] == ["mine", "control"], doc["tabs"]
	machine.succeed("echo 41 > /home/alice/mark && chown alice /home/alice/mark")
	for _ in range(30):
		w = snapshot(auth, "mine")
		if w.get("mine/load", {}).get("v") == 0.5 and w.get("mine/mark", {}).get("v") == 41 and w.get("mine/app", {}).get("v") == "ok" and w.get("mine/queue", {}).get("v") == 3:
			break
		machine.sleep(1)
	else:
		print(machine.execute("ls -la /home/alice /home/alice/mark")[1])
		print(as_alice("/run/current-system/sw/bin/elamond values mark", check=False))
		raise AssertionError(f"the sources' values never came: {w}")
	# A new value of the file within its interval.
	machine.succeed("echo 42 > /home/alice/mark")
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} 'https://localhost:47823/api/v1/snapshot?dashboard=mine' | jq -e '.widgets[\"mine/mark\"].v == 42'", timeout=20)
	# The token reached the web service, in its header.
	machine.succeed(f"grep -qxF 'Bearer {TOKEN}' /var/lib/token-echo/seen")

with subtest("a source nobody looks at isn't read"):
	machine.sleep(3)
	before = echoed()
	machine.sleep(8)
	assert echoed() == before, (before, echoed())
	snapshot(auth, "mine")
	machine.wait_until_succeeds(f"test $(wc -l < /var/lib/token-echo/seen) -gt {before}", timeout=10)

with subtest("elamond values reads a source now, and check sees nothing wrong"):
	out = as_alice("/run/current-system/sw/bin/elamond values app")
	assert "app.queue.length" in out and TOKEN not in out, out
	out = as_alice("/run/current-system/sw/bin/elamond values")
	assert "Sources: " in out and "app" in out, out
	out = as_alice("/run/current-system/sw/bin/elamond check")
	assert TOKEN not in out, out

with subtest("without the systemd-journal group: the user's own journal, and a note of what's missing"):
	out = as_alice("/run/current-system/sw/bin/elamond setup --check", check=False)
	assert "only this user's own journal" in out and "systemd-journal" in out, out
	doc = ui(auth)
	assert any(d["id"] == "journal" for d in doc["dashboards"]), [d["id"] for d in doc["dashboards"]]
	watch("journal-a", auth, "journal")
	machine.succeed("systemd-run --uid=alice --wait /run/current-system/sw/bin/logger -t elamon-vm 'hello from alice'")
	# The system journal widget shows warnings and up at first: ask it for everything.
	stream_id = stream_lines("/tmp/journal-a.out")[0]["stream"]
	status, _ = call("POST", f"/api/v1/stream/{stream_id}", auth, {"log": {"journal/system": {"level": "debug"}}})
	assert status == 200, status
	wait_for_line("journal-a", "journal/system", "hello from alice")
	machine.succeed("logger -t elamon-root -p user.warning 'from root, for the system journal'")
	machine.sleep(5)
	lines, notes = log_lines("journal-a", "journal/system")
	assert not any("from root" in l["text"] for l in lines), lines
	assert any("systemd-journal" in n for n in notes), notes
	machine.succeed("systemctl stop journal-a || true")
	# Nobody looks: journalctl ends with the stream.
	machine.wait_until_succeeds("! pgrep -u alice -x journalctl", timeout=15)

with subtest("a unit's journal opens from its link's row"):
	doc = ui(auth)
	links = next(w for d in doc["dashboards"] if d["id"] == "control" for w in d["widgets"] if w["id"] == "control/links")
	assert {"open": "unit_log", "label": "Journal"} in links["row_actions"], links
	board = next(d for d in doc["dashboards"] if d["id"] == "unit_log.echo")
	w = board["widgets"][0]
	assert w["type"] == "log" and w["scope"] == "unit" and w["unit"] == "token-echo.service", w

with subtest("in the systemd-journal group: the system journal, a unit's, older lines and a level"):
	machine.succeed("usermod -aG systemd-journal alice")
	machine.succeed("systemctl restart user@1000.service")
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	out = as_alice("/run/current-system/sw/bin/elamond setup --check", check=False)
	assert "the system journal is readable" in out, out
	watch("journal-b", auth, "journal")
	machine.succeed("logger -t elamon-root -p user.warning 'from root, now readable'")
	wait_for_line("journal-b", "journal/system", "from root, now readable")
	_, notes = log_lines("journal-b", "journal/system")
	assert not notes, notes
	# Only warnings and up come, as the widget says.
	machine.succeed("logger -t elamon-root -p user.info 'an info line, below warning'")
	machine.sleep(4)
	lines, _ = log_lines("journal-b", "journal/system")
	assert all(l["level"] in ("warning", "error", "critical") for l in lines), lines
	machine.succeed("systemctl stop journal-b || true")
	# The unit's journal, a page of it.
	status, page = call("GET", "/api/v1/log?widget=unit_log.echo/log&level=debug", auth)
	assert status == 200 and any("token-echo" in l["text"] or l.get("unit") == "token-echo.service" for l in page["lines"]), (status, page)
	# Older than the oldest: fewer, or none and no next page.
	oldest = page["lines"][0]["c"]
	status, older = call("GET", f"/api/v1/log?widget=unit_log.echo/log&level=debug&before={oldest}", auth)
	assert status == 200 and all(l["c"] < oldest for l in older["lines"]), older
	# A glance phone has no journal at all.
	code = pair_code("pair-glance", "--role", "glance")
	status, glance = call("POST", "/api/v1/pair", body={"code": code, "device": "Glance phone"})
	g = f"-H 'Authorization: Bearer {glance['token']}'"
	assert not any(w["type"] == "log" for d in ui(g)["dashboards"] for w in d["widgets"])
	status, _ = call("GET", "/api/v1/log?widget=journal/system", g)
	assert status == 404, status
	machine.wait_until_succeeds("! pgrep -u alice -x journalctl", timeout=15)

with subtest("the token is nowhere but its file and the web service"):
	machine.succeed(f"systemd-run {AS_ALICE} --wait /run/current-system/sw/bin/elamond backup /home/alice/test.backup --plain")
	machine.fail(f"grep -rlF {TOKEN} /home/alice --exclude=app.token")
	machine.fail(f"journalctl --no-pager | grep -F {TOKEN}")
	assert not any(TOKEN in s for s in seen)
