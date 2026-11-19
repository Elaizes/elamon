import json
import re

# Every request says which app it is, and which widget types it draws, as the 0.9 app does.
APP = "-H 'Elamon-Client: android/0.9.0'"
WIDGETS = "value,gauge,meter,status,graph,list,bars,table,button,toggle,select,slider,text,group,media,timeline,log"
DONE = (200, {"ok": True, "output": "done"})

machine.wait_for_unit("multi-user.target")

# The settings the module writes, which login shells name for the commands; systemd-run's don't.
ELAMON_CONFIG = machine.succeed("grep -oh '/nix/store/[a-z0-9]*-elamond.toml' /etc/set-environment /etc/systemd/user/elamond.service | head -1").strip()
assert ELAMON_CONFIG.startswith("/nix/store/"), ELAMON_CONFIG
AS_ALICE = f"--uid=alice -p Environment=HOME=/home/alice -p Environment=ELAMON_CONFIG={ELAMON_CONFIG}"
# Hyprland's own hyprctl as alice, beside her Hyprland's sockets.
HYPRCTL = "runuser -u alice -- env XDG_RUNTIME_DIR=/run/user/1000 /run/current-system/sw/bin/hyprctl"

def call(method, path, auth="", body=None):
	"""The status and the JSON body of a request to the service."""
	data = f"-H 'Content-Type: application/json' -d '{json.dumps(body)}'" if body is not None else ""
	out = machine.succeed(f"curl -sk -X {method} {APP} {auth} {data} -w '\\n%{{http_code}}' 'https://localhost:47823{path}'")
	text, status = out.rsplit("\n", 1)
	return int(status), json.loads(text) if text.strip() else None

def ui(auth):
	status, doc = call("GET", f"/api/v1/ui?widgets={WIDGETS}", auth)
	assert status == 200, (status, doc)
	return doc

def act(auth, id, params, confirm_text=None):
	body = {"params": params}
	if confirm_text is not None:
		body["confirm_text"] = confirm_text
	return call("POST", f"/api/v1/actions/{id}", auth, body)

def actions(doc):
	"""The monitors' actions a ui offers, by id, in its order."""
	return {a["id"]: a for a in doc["actions"] if a["kind"] == "display"}

def typed(action):
	"""What turning it off has typed in, if anything: the choice `off`'s confirm_text."""
	return next(c for c in action["params"][0]["choices"] if c["value"] == "off").get("confirm_text")

def ui_until(auth, ok, what):
	"""The monitors' actions a ui offers once `ok` holds of them: a reading is up to a tick old."""
	for _ in range(20):
		ids = actions(ui(auth))
		if ok(ids):
			return ids
		machine.sleep(1)
	raise AssertionError(f"never {what}: {list(ids)}")

def toggle_says(auth, wid, value):
	"""Waits for the Control dashboard's toggle `wid` to show `value`: a reading is up to a tick old."""
	for _ in range(20):
		status, snap = call("GET", "/api/v1/snapshot?dashboard=control", auth)
		assert status == 200, (status, snap)
		w = snap["widgets"].get(wid)
		if w and w.get("v") == value:
			return
		machine.sleep(1)
	raise AssertionError(f"{wid} never showed {value}: {w}")

def pair(unit, device, *args):
	"""Pairs a phone through `elamond pair --yes` run as alice in a unit of its own: its answer and
	its Authorization header."""
	machine.succeed(f"systemd-run {AS_ALICE} --unit={unit} /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair --yes {' '.join(args)}")
	pattern = "'^ +[0-9]{3} [0-9]{3} *$'"
	machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -Ec {pattern}", timeout=30)
	code = machine.succeed(f"journalctl -u {unit} -o cat | grep -E {pattern} | sed -n 1p | tr -d ' \\n'")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": device})
	assert status == 200, (status, paired)
	return paired, f"-H 'Authorization: Bearer {paired['token']}'"

def as_alice(command):
	"""A command in alice's login shell, what it printed (the journal's lines too)."""
	return machine.succeed(f"su - alice -c '{command}' 2>&1")

def lines(text):
	return [l.rstrip() for l in text.splitlines()]

# ---- a real Hyprland ---------------------------------------------------------------------------

def outputs():
	"""Hyprland's own word on its outputs, by name."""
	return {o["name"]: o for o in json.loads(machine.succeed(f"{HYPRCTL} --instance {SIG} -j monitors all"))}

def places():
	"""Each output's mode and place, by name."""
	return {n: {k: o[k] for k in ("width", "height", "refreshRate", "x", "y", "scale", "transform")} for n, o in outputs().items()}

with subtest("Hyprland on Lua settings runs as alice, on the virtio GPU's screen, with two outputs without one"):
	# alice's session on tty1 is the seat's: Hyprland, from her user manager, takes the screen through it.
	machine.wait_until_succeeds("loginctl list-sessions --no-legend | grep -q 'alice.*seat0'", timeout=60)
	machine.succeed("systemd-run --user -M alice@ --unit=hyprland /run/current-system/sw/bin/Hyprland --config /etc/hypr/elamon-test.lua")
	try:
		machine.wait_until_succeeds(f"{HYPRCTL} -j instances 2>/dev/null | jq -e 'length == 1'", timeout=60)
	except Exception:
		print(machine.execute("journalctl _SYSTEMD_USER_UNIT=hyprland.service -o cat --no-pager | tail -n 100")[1])
		raise
	SIG = json.loads(machine.succeed(f"{HYPRCTL} -j instances"))[0]["instance"]
	assert machine.succeed(f"{HYPRCTL} --instance {SIG} eval return").strip() == "ok"
	machine.succeed(f"{HYPRCTL} --instance {SIG} output create headless A")
	machine.succeed(f"{HYPRCTL} --instance {SIG} output create headless B")
	machine.wait_until_succeeds(f"{HYPRCTL} --instance {SIG} -j monitors all | jq -e 'length == 3'", timeout=30)
	o = outputs()
	assert [(n, x["hardwareDetails"]["backend"], x["x"], x["disabled"], x["dpmsStatus"]) for n, x in o.items()] == [
		("Virtual-1", "drm", 0, False, True), ("A", "headless", 1280, False, True), ("B", "headless", 3200, False, True)
	], o

with subtest("a phone pairs; the service finds the Hyprland started after it by itself, while the phone shows the monitors"):
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	paired, auth = pair("pair", "Pixel 9")
	assert paired["role"] == "admin", paired
	# The Control dashboard on screen, as the app shows it: the monitors are read every second, and
	# Hyprland looked for again every 30 seconds while there is none.
	machine.succeed(f"systemd-run --unit=stream-watch /bin/sh -c \"/run/current-system/sw/bin/curl -sNk --max-time 900 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard=control&widgets={WIDGETS}' > /tmp/stream.out\"")
	for _ in range(60):
		doc = ui(auth)
		if "display_control" in doc["service"]["features"]:
			break
		machine.sleep(1)
	else:
		raise AssertionError(f"no display_control: {doc['service']}")
	machine.succeed(f"journalctl _UID=1000 -o cat | grep -c 'displays: Hyprland {SIG} found'")
	ids = actions(doc)
	assert list(ids) == [f"display.{w}.{o}" for o in ("Virtual-1", "A", "B") for w in ("output", "dpms")], list(ids)
	assert all(a["confirm"] and [c["value"] for c in a["params"][0]["choices"]] == ["on", "off"] for a in ids.values()), ids
	# Virtual-1 is the one screen: its off is typed in. A and B, without one, never count as one
	# showing a picture; with the screen on theirs isn't.
	assert typed(ids["display.output.Virtual-1"]) == "Virtual-1" and typed(ids["display.dpms.Virtual-1"]) == "Virtual-1", ids["display.output.Virtual-1"]
	assert [typed(ids[f"display.{w}.{o}"]) for w in ("output", "dpms") for o in ("A", "B")] == [None] * 4, ids
	# The Control dashboard's toggles, two for each output.
	board = next(d for d in doc["dashboards"] if d["id"] == "control")
	group = next(w for w in board["widgets"] if w["id"] == "control/display_controls")
	TOGGLE = {w["action"]: w["id"] for w in group["widgets"] if w["type"] == "toggle"}
	assert set(TOGGLE) == set(ids), TOGGLE
	toggle_says(auth, TOGGLE["display.output.A"], "on")

with subtest("output A off and on again through the API, as Hyprland then says, back with the mode and place it had"):
	before = places()
	assert act(auth, "display.output.A", {"state": "off"}) == DONE
	assert outputs()["A"]["disabled"] is True
	toggle_says(auth, TOGGLE["display.output.A"], "off")
	# Off, it has no picture to put to sleep.
	ui_until(auth, lambda ids: "display.output.A" in ids and "display.dpms.A" not in ids, "A without DPMS")
	assert act(auth, "display.output.A", {"state": "on"}) == DONE
	assert not outputs()["A"]["disabled"] and outputs()["A"]["dpmsStatus"]
	assert places() == before, (before, places())
	toggle_says(auth, TOGGLE["display.output.A"], "on")
	# Done already: nothing to do, and done.
	assert act(auth, "display.output.A", {"state": "on"}) == DONE

with subtest("A's picture off and on (DPMS)"):
	assert act(auth, "display.dpms.A", {"state": "off"}) == DONE
	assert outputs()["A"]["dpmsStatus"] is False and outputs()["A"]["disabled"] is False
	toggle_says(auth, TOGGLE["display.dpms.A"], "off")
	assert act(auth, "display.dpms.A", {"state": "on"}) == DONE
	assert outputs()["A"]["dpmsStatus"] is True
	toggle_says(auth, TOGGLE["display.dpms.A"], "on")

with subtest("Virtual-1, the one screen with a picture, goes to sleep and off only with its name typed, and comes back"):
	for text in [None, "virtual-1", "Virtual-1 ", "A"]:
		assert act(auth, "display.output.Virtual-1", {"state": "off"}, text) == (403, {"error": "confirm_required", "message": "Virtual-1"}), text
		assert act(auth, "display.dpms.Virtual-1", {"state": "off"}, text) == (403, {"error": "confirm_required", "message": "Virtual-1"}), text
	screen = outputs()["Virtual-1"]
	assert not screen["disabled"] and screen["dpmsStatus"], screen
	before = places()
	assert act(auth, "display.dpms.Virtual-1", {"state": "off"}, "Virtual-1") == DONE
	assert outputs()["Virtual-1"]["dpmsStatus"] is False
	# Its picture back needs no name.
	assert act(auth, "display.dpms.Virtual-1", {"state": "on"}) == DONE
	assert outputs()["Virtual-1"]["dpmsStatus"] is True
	assert act(auth, "display.output.Virtual-1", {"state": "off"}, "Virtual-1") == DONE
	assert outputs()["Virtual-1"]["disabled"] is True
	toggle_says(auth, TOGGLE["display.output.Virtual-1"], "off")
	# No screen shows a picture now; an output without one still goes off and on without its name:
	# that leaves nobody in the dark.
	ids = ui_until(auth, lambda ids: "display.dpms.Virtual-1" not in ids, "Virtual-1 without DPMS")
	assert typed(ids["display.output.A"]) is None and typed(ids["display.dpms.A"]) is None, ids["display.output.A"]
	assert act(auth, "display.output.A", {"state": "off"}) == DONE
	assert outputs()["A"]["disabled"] is True
	assert act(auth, "display.output.A", {"state": "on"}) == DONE
	assert outputs()["A"]["disabled"] is False
	# Virtual-1 on again, as Hyprland applies it with the next picture A or B draws: back where it was.
	assert act(auth, "display.output.Virtual-1", {"state": "on"}) == DONE
	assert not outputs()["Virtual-1"]["disabled"]
	assert places() == before, (before, places())
	toggle_says(auth, TOGGLE["display.output.Virtual-1"], "on")

with subtest("elamond values displays and check tell of this Hyprland"):
	values = lines(as_alice("elamond values displays"))
	print("\n".join(values))
	assert values[0] == f"Hyprland {SIG}, its settings Lua.", values
	assert re.fullmatch(r"Virtual-1  on · DPMS on · \d+×\d+ @ [\d.]+ Hz at 0, 0, scale 1 · no brightness · no other screen shows a picture: its name is typed to turn it off", values[1]), values
	assert values[2:] == [
		"A          on · DPMS on · 1920×1080 @ 60.00 Hz at 1280, 0, scale 1 · no screen",
		"B          on · DPMS on · 1920×1080 @ 60.00 Hz at 3200, 0, scale 1 · no screen",
		"Actions: display.output.Virtual-1, display.dpms.Virtual-1, display.output.A, display.dpms.A, display.output.B, display.dpms.B",
	], values
	check = lines(as_alice("elamond check"))
	print("\n".join(check))
	assert "Monitors: Hyprland on Lua: Virtual-1 turned on and off from a phone." in check, check

# ---- a stand-in hyprctl ------------------------------------------------------------------------

# elamond's own tests' stand-in, in a folder of alice's that holds its state too: ~/.nix-profile/bin
# comes before /run/current-system/sw/bin, Hyprland's, on the service's PATH and in her shell.
STANDIN = "/home/alice/stand-in/bin"

def standin(name):
	"""The stand-in's state of output `name`."""
	return machine.succeed(f"cat {STANDIN}/outputs/{name}")

def standin_output(name, disabled, backend):
	machine.succeed(f"printf 'disabled={disabled}\\ndpms=true\\nbackend={backend}\\n' > {STANDIN}/outputs/{name}")

def evals():
	"""The Lua the stand-in was given."""
	return machine.succeed(f"cat {STANDIN}/evals 2>/dev/null || true").splitlines()

with subtest("a stand-in hyprctl ahead of Hyprland's: a laptop's panel off, a monitor on and a stream's output"):
	machine.succeed("systemctl stop stream-watch || true")
	machine.succeed("systemctl --user -M alice@ stop hyprland")
	machine.succeed(f"install -D -m 755 /etc/elamon-test/hyprctl-stand-in {STANDIN}/hyprctl && mkdir -p {STANDIN}/outputs")
	machine.succeed(f"echo '[{{\"instance\": \"standin_1_1\", \"time\": 100, \"pid\": 1, \"wl_socket\": \"wayland-1\"}}]' > {STANDIN}/instances")
	standin_output("eDP-1", "true", "drm")
	standin_output("DP-1", "false", "drm")
	standin_output("HEADLESS-2", "false", "headless")
	machine.succeed("chown -R alice:users /home/alice/stand-in")
	as_alice("ln -sfn ~/stand-in ~/.nix-profile")
	machine.succeed("systemctl --user -M alice@ restart elamond")
	machine.wait_until_succeeds("curl -sk https://localhost:47823/api/v1/hello", timeout=60)
	# The Control dashboard on screen again.
	machine.succeed(f"systemd-run --unit=stream-watch-2 /bin/sh -c \"/run/current-system/sw/bin/curl -sNk --max-time 900 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard=control&widgets={WIDGETS}' > /tmp/stream-2.out\"")
	# Read as the service starts, and in the ui with its first reading.
	for _ in range(20):
		doc = ui(auth)
		if "display_control" in doc["service"]["features"]:
			break
		machine.sleep(1)
	else:
		print(machine.succeed(f"journalctl _UID=1000 -o cat | grep 'displays:'; cat {STANDIN}/log"))
		raise AssertionError(f"no display_control: {doc['service']}")
	ids = actions(doc)
	# In Hyprland's order; the panel, off, has no picture to put to sleep; the stream's output is
	# there as [displays] headless says.
	assert list(ids) == ["display.output.DP-1", "display.dpms.DP-1", "display.output.HEADLESS-2", "display.dpms.HEADLESS-2", "display.output.eDP-1"], list(ids)
	assert all(a["confirm"] and a["params"][0]["id"] == "state" for a in ids.values()), ids
	# DP-1 is the one screen showing a picture: its off is typed in, the others' aren't.
	assert typed(ids["display.output.DP-1"]) == "DP-1" and typed(ids["display.dpms.DP-1"]) == "DP-1", ids["display.output.DP-1"]
	assert typed(ids["display.output.eDP-1"]) is None and typed(ids["display.output.HEADLESS-2"]) is None, ids

with subtest("DP-1 off only with its name typed; the panel on; what isn't there or isn't the phone's to do, refused"):
	for text in [None, "dp-1", "DP-1 ", "eDP-1"]:
		assert act(auth, "display.output.DP-1", {"state": "off"}, text) == (403, {"error": "confirm_required", "message": "DP-1"}), text
		assert act(auth, "display.dpms.DP-1", {"state": "off"}, text) == (403, {"error": "confirm_required", "message": "DP-1"}), text
	assert evals() == [], evals()
	assert act(auth, "display.output.DP-1", {"state": "off"}, "DP-1") == DONE
	assert "disabled=true" in standin("DP-1")
	# Turned on with what Hyprland kept for it; no name asked.
	assert act(auth, "display.output.eDP-1", {"state": "on"}) == DONE
	assert "disabled=false" in standin("eDP-1")
	assert evals() == [
		"hl.monitor({ output = 'DP-1', disabled = true })",
		"hl.monitor({ output = 'eDP-1', mode = '1920x1080@144.149', position = '0x0', scale = 1, transform = 0, disabled = false })",
	], evals()
	assert act(auth, "display.output.HDMI-A-1", {"state": "on"}) == (404, {"error": "no_such_action"})
	paired, monitor = pair("pair-monitor", "Pixel 8", "--role", "monitor")
	assert paired["role"] == "monitor", paired
	assert actions(ui(monitor)) == {}
	assert act(monitor, "display.output.eDP-1", {"state": "off"}, "eDP-1") == (403, {"error": "not_allowed"})
	assert act(monitor, "display.output.HDMI-A-1", {"state": "off"}) == (404, {"error": "no_such_action"})
	assert len(evals()) == 2, evals()
	assert "disabled=false" in standin("eDP-1")
	# Now the panel is the one screen showing a picture.
	ids = ui_until(auth, lambda ids: "display.dpms.eDP-1" in ids and "display.dpms.DP-1" not in ids, "the panel on, DP-1 off")
	assert typed(ids["display.output.eDP-1"]) == "eDP-1" and typed(ids["display.dpms.eDP-1"]) == "eDP-1", ids
	assert act(auth, "display.dpms.eDP-1", {"state": "off"}) == (403, {"error": "confirm_required", "message": "eDP-1"})
	values = lines(as_alice("elamond values displays"))
	print("\n".join(values))
	assert values[0] == "Hyprland standin_1_1, its settings Lua.", values
	assert values[1] == "DP-1        off · 1920×1080 @ 144.15 Hz at 0, 0, scale 1 · no brightness", values
	assert values[3] == "eDP-1       on · DPMS on · 1920×1080 @ 144.15 Hz at 0, 0, scale 1 · no brightness · no other screen shows a picture: its name is typed to turn it off", values
	assert values[4] == "Actions: display.output.DP-1, display.output.HEADLESS-2, display.dpms.HEADLESS-2, display.output.eDP-1, display.dpms.eDP-1", values
	assert act(auth, "display.output.DP-1", {"state": "on"}) == DONE
	assert "disabled=false" in standin("DP-1")

with subtest("the activity log has them, by phone, with how each ended"):
	rows = [json.loads(l) for l in machine.succeed("cat /home/alice/.local/share/elamon/activity.jsonl").splitlines()]
	got = {(e["phone"], e["what"], e["result"]) for e in rows}
	print("\n".join(sorted(map(str, got))))
	for entry in [
		("Pixel 9", "display output A off", "ok"),
		("Pixel 9", "display dpms A on", "ok"),
		("Pixel 9", "display output Virtual-1 off", "confirm_required"),
		("Pixel 9", "display output Virtual-1 off", "ok"),
		("Pixel 9", "display dpms DP-1 off", "confirm_required"),
		("Pixel 9", "display output DP-1 off", "ok"),
		("Pixel 9", "display output eDP-1 on", "ok"),
		("Pixel 8", "display output eDP-1 off", "not_allowed"),
	]:
		assert entry in got, entry

with subtest("ddcutil where the module's option puts it, on the service's PATH; and what check says of DDC"):
	env = machine.succeed("systemctl --user -M alice@ show elamond -p Environment --value")
	found = re.search(r"\bPATH=(\S+)", env)
	assert found, env
	path = found[1]
	assert re.match(r"/nix/store/[a-z0-9]{32}-ddcutil-[^/:]+/bin:", path), path
	machine.succeed(f"test -x {path.split(':')[0]}/ddcutil")
	# DP-1 with an I²C bus, as DRM tells of a monitor's, in a sysfs and /dev of the test's own;
	# `elamond check` with the service's PATH.
	machine.succeed("mkdir -p /run/elamon-test/sys/class/drm/card1-DP-1 /run/elamon-test/dev && ln -s ../../i2c-14 /run/elamon-test/sys/class/drm/card1-DP-1/ddc")
	check = f"ELAMON_SYSFS=/run/elamon-test/sys ELAMON_DEV=/run/elamon-test/dev PATH={path} elamond check"
	out = lines(as_alice(check))
	print("\n".join(out))
	assert "  ! Monitors: DP-1's brightness needs access to /dev/i2c-14: add the user to the i2c group" in out, out
	# The bus there for alice: ddcutil asked, and no monitor answering it.
	machine.succeed("touch /run/elamon-test/dev/i2c-14 && chown alice /run/elamon-test/dev/i2c-14")
	out = lines(as_alice(check))
	print("\n".join(out))
	assert "  ! Monitors: DP-1 doesn't answer DDC: no brightness for it" in out, out
	assert not any("brightness" in a for a in actions(ui(auth))), actions(ui(auth))
