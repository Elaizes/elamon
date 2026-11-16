import base64
import json

# Every request says which app it is, and which widget types it draws, as the 0.7 app does.
APP = "-H 'Elamon-Client: android/0.7.0'"
WIDGETS = "value,gauge,meter,status,graph,list,bars,table,button,toggle,select,slider,text,group,media,timeline"

machine.wait_for_unit("multi-user.target")
machine.wait_for_unit("nutflow.service")
machine.wait_for_open_port(3493)

# The settings the module writes, which login shells name for the commands; systemd-run's don't.
ELAMON_CONFIG = machine.succeed("grep -oh '/nix/store/[a-z0-9]*-elamond.toml' /etc/set-environment /etc/systemd/user/elamond.service | head -1").strip()
assert ELAMON_CONFIG.startswith("/nix/store/"), ELAMON_CONFIG
AS_ALICE = f"--uid=alice -p Environment=HOME=/home/alice -p Environment=ELAMON_CONFIG={ELAMON_CONFIG}"

# The NUT user's password, made at boot: looked for below everywhere elamond writes.
PASSWORD = machine.succeed("cat /var/lib/secrets/nut-elamon").strip()
assert PASSWORD.startswith("nut-") and len(PASSWORD) == 28, PASSWORD
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

def act(auth, id, params, confirm_text=None):
	body = {"params": params}
	if confirm_text is not None:
		body["confirm_text"] = confirm_text
	return call("POST", f"/api/v1/actions/{id}", auth, body)

def upsc(var):
	return machine.succeed(f"upsc ups@localhost {var} 2>/dev/null").strip()

def station_says(var, value):
	"""Waits for nutflow to report `var` as `value`: the station's Display shows a change about a
	second after it acknowledged it."""
	machine.wait_until_succeeds(f"upsc ups@localhost {var} 2>/dev/null | grep -qx '{value}'", timeout=15)

def widget(dashboard, wid):
	"""The widget `wid` of `dashboard` in a ui document, nested groups included."""
	todo = list(dashboard["widgets"])
	while todo:
		w = todo.pop()
		if w["id"] == wid:
			return w
		todo += w.get("widgets", [])
	return None

def pair_code(unit, *args):
	"""Runs `elamond pair --yes` as alice in a unit of its own and returns the code it prints."""
	machine.succeed(f"systemd-run {AS_ALICE} --unit={unit} /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair --yes {' '.join(args)}")
	pattern = "'^ +[0-9]{3} [0-9]{3} *$'"
	machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -Ec {pattern}", timeout=30)
	return machine.succeed(f"journalctl -u {unit} -o cat | grep -E {pattern} | sed -n 1p | tr -d ' \\n'")

def stream_lines(out):
	"""A stream's lines, but the last when it was cut off mid-line."""
	lines = []
	for l in out.splitlines():
		try:
			lines.append(json.loads(l))
		except json.JSONDecodeError:
			pass
	return lines

def activity(auth, entries):
	"""The activity log's rows, once each of `entries` (phone, what, result) is among them: a
	reading is up to a tick old."""
	for _ in range(30):
		rows = [r["f"] for r in snapshot(auth, "activity")["activity/log"]["rows"]]
		if set(entries) <= {(r.get("phone"), r["what"], r["result"]) for r in rows}:
			return rows
		machine.sleep(1)
	raise AssertionError(f"{entries} not all in the activity log: {rows}")

def now_ms():
	return int(machine.succeed("date +%s%3N").strip())

with subtest("nutflow serves the mock station; the password is root's for it, and alice's alone for her service"):
	machine.wait_until_succeeds("upsc -l localhost 2>/dev/null | grep -qx ups")
	machine.wait_until_succeeds("upsc ups@localhost ups.realpower 2>/dev/null | grep -Eq '^[0-9]+'")
	print(machine.succeed("upsc ups@localhost 2>/dev/null"))
	print(machine.succeed("upscmd -l ups@localhost 2>/dev/null"))
	machine.succeed("test $(stat -c %a:%U /var/lib/secrets/nut-elamon) = 600:root")
	machine.succeed("test $(stat -c %a:%U /home/alice/.config/elamon/nut.password) = 600:alice")
	machine.fail(f"grep -F {PASSWORD} {ELAMON_CONFIG} /etc/nutflow/nutflow.toml")
	assert upsc("outlet.1.status") == upsc("outlet.2.status") == upsc("outlet.3.status") == "on"
	assert upsc("ups.beeper.status") == "enabled"

with subtest("a phone pairs, and its ui has the station's dashboard and actions as far as the source allows"):
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	code = pair_code("pair")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Test phone"})
	assert status == 200 and paired["role"] == "admin", paired
	auth = f"-H 'Authorization: Bearer {paired['token']}'"
	doc = ui(auth)
	# The dashboard is there from the start, whether upsd answers or not.
	assert "ups" in doc["service"]["features"], doc["service"]
	assert "ups.eco" in doc["tabs"], doc["tabs"]
	# The actions come once upsd has said what the station offers, the widgets once it has had
	# their values (the outlets, the model in the title).
	for _ in range(60):
		doc = ui(auth)
		ids = {a["id"] for a in doc["actions"]}
		board = next(d for d in doc["dashboards"] if d["id"] == "ups.eco")
		switches = widget(board, "ups.eco/switches")
		if {"nut.command.eco", "nut.set.eco.experimental.battery.charge.max"} <= ids and board["title"] != "eco" and switches and widget(board, "ups.eco/now"):
			break
		machine.sleep(1)
	else:
		raise AssertionError(f"the station's actions and widgets never came: {json.dumps(doc)}")
	assert not any(i.startswith("nut.") for i in ids - {"nut.command.eco", "nut.set.eco.experimental.battery.charge.max"}), ids
	command = next(a for a in doc["actions"] if a["id"] == "nut.command.eco")
	assert command["kind"] == "nut" and command["name"] == "DELTA 3 Max Plus" and command["confirm"], command
	choices = {c["value"]: c for c in command["params"][0]["choices"]}
	# Outlets 1 and 2 and the beeper; outlet 3 the station has, but the source doesn't list.
	assert set(choices) == {f"outlet.{n}.load.{s}" for n in (1, 2) for s in ("on", "off")} | {"beeper.enable", "beeper.disable"}, choices
	# Outlet 1 powers this computer: turning it off is typed in, turning it on and outlet 2 aren't.
	assert [v for v, c in choices.items() if "confirm_text" in c] == ["outlet.1.load.off"], choices
	assert choices["outlet.1.load.off"]["confirm_text"] == "outlet.1.load.off"
	charge = next(a for a in doc["actions"] if a["id"] == "nut.set.eco.experimental.battery.charge.max")
	assert charge["kind"] == "nut" and charge["params"] == [{"id": "value", "kind": "range", "label": "Value", "min": 50, "max": 100, "step": 1, "unit": "%"}], charge
	assert board["title"] == "DELTA 3 Max Plus", board["title"]
	for wid, kind in [("ups.eco/status", "status"), ("ups.eco/power", "timeline"), ("ups.eco/watts", "graph"), ("ups.eco/station/charge_max", "slider"), ("ups.eco/station/beeper", "toggle")]:
		w = widget(board, wid)
		assert w and w["type"] == kind, (wid, w)
	assert widget(board, "ups.eco/watts")["ranges"] == ["1h", "24h", "7d", "30d", "1y", "all"]
	# A switch for each outlet the source may switch, and none for one it may not.
	assert sorted(w["id"] for w in switches["widgets"]) == ["ups.eco/switches/outlet.eco.1", "ups.eco/switches/outlet.eco.2"], switches
	# Settings the source doesn't list are left out.
	assert widget(board, "ups.eco/station/xboost") is None and widget(board, "ups.eco/station/charge_min") is None
	output = next(w["id"] for w in widget(board, "ups.eco/now")["widgets"] if w["id"].endswith("/output"))

# A stream kept open on the dashboard, as the app on screen: the source is read every second.
machine.succeed(f"systemd-run --unit=stream-watch /bin/sh -c \"/run/current-system/sw/bin/curl -sNk --max-time 600 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard=ups.eco&widgets={WIDGETS}' > /tmp/stream.out\"")

with subtest("the stream has the station's watts, the timeline and the outlets"):
	for _ in range(60):
		data = [l for l in stream_lines(machine.succeed("cat /tmp/stream.out 2>/dev/null || true")) if l["type"] == "data"]
		if data and (data[-1]["widgets"].get(output) or {}).get("v", 0) > 0:
			break
		machine.sleep(1)
	else:
		raise AssertionError(f"no watts in the stream: {data[-1:]}")
	started = now_ms()
	assert data[-1]["dashboard"] == "ups.eco", data[-1]
	w = data[-1]["widgets"]
	print(json.dumps(w, indent=1))
	assert w["ups.eco/status"]["v"] in ("On line", "On battery"), w["ups.eco/status"]
	assert all(isinstance(x, (int, float)) for x in w["ups.eco/watts"]["v"]), w["ups.eco/watts"]
	segments = w["ups.eco/power"]["segments"]
	assert segments and segments[-1]["state"] in ("line", "battery"), segments
	rows = {r["key"]: r["f"] for r in w["ups.eco/outlets"]["rows"]}
	assert rows["eco.2"]["status"] == "On" and rows["eco.2"]["desc"] == "AC outlets, group 2", rows
	assert w["ups.eco/switches/outlet.eco.2"] == {"v": "outlet.2.load.on", "enabled": True}, w
	assert w["ups.eco/station/beeper"]["v"] == "beeper.enable", w["ups.eco/station/beeper"]

with subtest("outlet 2 off and on again through the API, as the station says"):
	assert act(auth, "nut.command.eco", {"cmd": "outlet.2.load.off"}) == (200, {"ok": True, "output": "done"})
	station_says("outlet.2.status", "off")
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} 'https://localhost:47823/api/v1/snapshot?dashboard=ups.eco' | jq -e '.widgets[\"ups.eco/switches/outlet.eco.2\"].v == \"outlet.2.load.off\"'", timeout=30)
	assert act(auth, "nut.command.eco", {"cmd": "outlet.2.load.on"}) == (200, {"ok": True, "output": "done"})
	station_says("outlet.2.status", "on")
	machine.succeed("journalctl _UID=1000 -o cat | grep -c 'nut eco: outlet.2.load.off: done'")

with subtest("the charge limit: 90 the station takes, 40 is off its range and never reaches it"):
	assert act(auth, "nut.set.eco.experimental.battery.charge.max", {"value": 90}) == (200, {"ok": True, "output": "done"})
	station_says("experimental.battery.charge.max", "90")
	assert act(auth, "nut.set.eco.experimental.battery.charge.max", {"value": 40}) == (400, {"error": "bad_param", "message": "value"})
	assert upsc("experimental.battery.charge.max") == "90"

with subtest("what the source doesn't list isn't run, nor the critical outlet off unless typed in"):
	assert act(auth, "nut.command.eco", {"cmd": "outlet.3.load.off"}) == (403, {"error": "not_allowed", "message": "cmd"})
	assert act(auth, "nut.command.eco", {"cmd": "load.off"}) == (403, {"error": "not_allowed", "message": "cmd"})
	assert act(auth, "nut.set.eco.experimental.output.xboost", {"value": "enabled"}) == (404, {"error": "no_such_action"})
	assert act(auth, "nut.command.eco", {"cmd": "outlet.1.load.off"}) == (403, {"error": "confirm_required", "message": "outlet.1.load.off"})
	assert act(auth, "nut.command.eco", {"cmd": "outlet.1.load.off"}, "outlet.2.load.off") == (403, {"error": "confirm_required", "message": "outlet.1.load.off"})
	machine.sleep(2)
	assert upsc("outlet.1.status") == upsc("outlet.3.status") == "on"
	# Typed in, it runs; turning it on again isn't.
	assert act(auth, "nut.command.eco", {"cmd": "outlet.1.load.off"}, "outlet.1.load.off") == (200, {"ok": True, "output": "done"})
	station_says("outlet.1.status", "off")
	assert act(auth, "nut.command.eco", {"cmd": "outlet.1.load.on"}) == (200, {"ok": True, "output": "done"})
	station_says("outlet.1.status", "on")

with subtest("a command the source lists and nutflow's user may not run: the station's refusal, in its words"):
	status, body = act(auth, "nut.command.eco", {"cmd": "beeper.disable"})
	assert status == 502 and body["error"] == "failed" and "access denied" in body["message"], (status, body)
	machine.sleep(2)
	assert upsc("ups.beeper.status") == "enabled"

with subtest("elamond control off stops the station's actions too"):
	machine.succeed("su - alice -c 'elamond control off'")
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} 'https://localhost:47823/api/v1/ui?widgets={WIDGETS}' | jq -e '.control.enabled == false'", timeout=30)
	assert act(auth, "nut.command.eco", {"cmd": "outlet.2.load.off"}) == (403, {"error": "control_disabled"})
	machine.sleep(2)
	assert upsc("outlet.2.status") == "on"
	machine.succeed("su - alice -c 'elamond control on'")
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} 'https://localhost:47823/api/v1/ui?widgets={WIDGETS}' | jq -e '.control.enabled'", timeout=30)

with subtest("the activity log has each, by phone, with how it ended"):
	activity(auth, [
		("Test phone", "nut eco outlet.2.load.off", "ok"),
		("Test phone", "nut eco experimental.battery.charge.max 90", "ok"),
		("Test phone", "nut eco experimental.battery.charge.max 40", "bad_param"),
		("Test phone", "nut eco outlet.3.load.off", "not_allowed"),
		("Test phone", "nut eco outlet.1.load.off", "confirm_required"),
		("Test phone", "nut eco outlet.1.load.off", "ok"),
		("Test phone", "nut eco beeper.disable", "failed"),
		("Test phone", "nut eco outlet.2.load.off", "control_disabled"),
	])

with subtest("elamond values asks the station, and check reads the source"):
	values = machine.succeed("su - alice -c 'elamond values eco' 2>&1")
	print(values)
	seen.append(values)
	for word in ["eco.ups.realpower", "eco.outlet.2.status", "outlet.2.load.off", "experimental.battery.charge.max"]:
		assert word in values, word
	rc, check = machine.execute("su - alice -c 'elamond check' 2>&1")
	print(check)
	seen.append(check)
	assert rc == 0, check

with subtest("restarted, the service has the history from before"):
	# Twenty seconds of readings, two 10-second points at least, before the restart.
	machine.sleep(max(0, 25 - (now_ms() - started) // 1000))
	restart = now_ms()
	machine.succeed("systemctl --user -M alice@ restart elamond")
	machine.wait_until_succeeds("curl -sk https://localhost:47823/api/v1/hello", timeout=60)
	# The graph is in ui once the source has had its values again (its `when`).
	for _ in range(30):
		board = next(d for d in ui(auth)["dashboards"] if d["id"] == "ups.eco")
		if widget(board, "ups.eco/watts"):
			break
		machine.sleep(1)
	else:
		raise AssertionError(f"no graph after the restart: {json.dumps(board)}")
	status, hist = call("GET", "/api/v1/history?widget=ups.eco/watts&range=1h", auth)
	assert status == 200 and hist["step_ms"] == 10000, (status, hist)
	before = [t for t, out in zip(hist["t"], hist["v"][1]) if out is not None and t + 10000 <= restart]
	assert len(before) >= 2, (restart, hist)
	status, line = call("GET", "/api/v1/history?widget=ups.eco/power&range=1h", auth)
	assert status == 200, (status, line)
	known = [s for s in line["segments"] if s["state"] in ("line", "battery") and s["from"] < restart - 10000]
	assert known, (restart, line)
	# The graph a new stream starts with reaches back past the restart too.
	out = machine.succeed(f"curl -sNk --max-time 4 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard=ups.eco&widgets={WIDGETS}' || true")
	seen.append(out)
	first = next(l for l in stream_lines(out) if l["type"] == "data")
	assert any(t < restart for t in first["history"]["ups.eco/watts"]["t"]), first["history"]

with subtest("the NUT password is nowhere: answers, streams, the journal, elamond's files, a backup"):
	machine.succeed("systemctl stop stream-watch || true")
	seen.append(machine.succeed("cat /tmp/stream.out"))
	seen.append(json.dumps(ui(auth)))
	for text in seen:
		assert PASSWORD not in text, text
	assert machine.succeed(f"journalctl -o cat --no-pager | grep -cF {PASSWORD} || true").strip() == "0"
	# Only the file it is in, of all alice's.
	assert machine.succeed(f"grep -rlF {PASSWORD} /home/alice || true").split() == ["/home/alice/.config/elamon/nut.password"]
	out = machine.succeed("su - alice -c 'elamond backup ~/b.elamond' 2>&1")
	assert PASSWORD not in out, out
	magic, header, payload = machine.succeed("cat /home/alice/b.elamond").split("\n", 2)
	assert magic == "elamond-backup", magic
	payload = json.loads(payload)
	for name, text in payload["settings"].items():
		assert PASSWORD not in text, name
	for name, b64 in payload["state"].items():
		assert PASSWORD.encode() not in base64.b64decode(b64), name
