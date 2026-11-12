import json
import re

machine.wait_for_unit("multi-user.target")

# Every request says which app it is, as the app does.
APP = "-H 'Elamon-Client: android/0.6.0'"

# Journal checks count with grep -c, which reads all journalctl writes: grep -q would stop at a
# match and journalctl, cut off mid-write, fail the pipe.

# The settings the module writes, which login shells name for the commands; systemd-run's don't.
ELAMON_CONFIG = machine.succeed("grep -oh '/nix/store/[a-z0-9]*-elamond.toml' /etc/set-environment /etc/systemd/user/elamond.service | head -1").strip()
assert ELAMON_CONFIG.startswith("/nix/store/"), ELAMON_CONFIG
AS_ALICE = f"--uid=alice -p Environment=HOME=/home/alice -p Environment=ELAMON_CONFIG={ELAMON_CONFIG}"

def call(method, path, auth="", body=None):
	"""The status and the JSON body of a request to the service."""
	data = f"-H 'Content-Type: application/json' -d '{json.dumps(body)}'" if body is not None else ""
	out = machine.succeed(f"curl -sk -X {method} {APP} {auth} {data} -w '\\n%{{http_code}}' https://localhost:47823{path}")
	text, status = out.rsplit("\n", 1)
	return int(status), json.loads(text) if text.strip() else None

def ui(auth):
	status, doc = call("GET", "/api/v1/ui", auth)
	assert status == 200, (status, doc)
	return doc

def activity(auth, entry):
	"""The activity log's rows as the role of `auth` reads them, once `entry` (phone, what, result) is
	among them: a reading is up to a tick old."""
	for _ in range(30):
		_, snap = call("GET", "/api/v1/snapshot?dashboard=activity", auth)
		rows = [r["f"] for r in snap["widgets"]["activity/log"]["rows"]]
		if entry in {(r.get("phone"), r["what"], r["result"]) for r in rows}:
			return rows
		machine.sleep(1)
	now = machine.succeed("date +%s%3N").strip()
	raise AssertionError(f"{entry} not in the activity log; the reading is of {snap['t']}, now {now}: {rows}")

def pair_code(unit, *args):
	"""Runs `elamond pair --yes` as alice in a unit of its own and returns the code it prints."""
	machine.succeed(f"systemd-run {AS_ALICE} --unit={unit} /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair --yes {' '.join(args)}")
	pattern = "'^ +[0-9]{3} [0-9]{3} *$'"
	machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -Ec {pattern}", timeout=30)
	return machine.succeed(f"journalctl -u {unit} -o cat | grep -E {pattern} | sed -n 1p | tr -d ' \\n'")

with subtest("the greeter and users not listed run no service"):
	machine.succeed("systemctl start user@990.service user@1001.service")
	machine.sleep(5)
	machine.fail("ss -ltn | grep -q :47823")

with subtest("the listed user's service takes the port, and says it speaks API 2"):
	machine.succeed("systemctl start user@1000.service")
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	hello = json.loads(machine.succeed("curl -sk https://localhost:47823/api/v1/hello"))
	assert hello["app"] == "elamon" and hello["api"] == 2 and hello["widgets"] == 1 and hello["min_app"] == "0.6.0", hello
	# What the computer can do is no stranger's business: only a paired phone reads it, in ui.
	assert "features" not in hello, hello

with subtest("the module's settings are the defaults with those in Nix over them, and the user's own files are beside"):
	settings = machine.succeed(f"cat {ELAMON_CONFIG}")
	assert settings.startswith("schema = 3\n") or "\nschema = 3\n" in settings, settings
	# Under ELAMON_CONFIG the service writes no elamond.toml of the user's, but defaults.toml beside.
	machine.fail("test -e /home/alice/.config/elamon/elamond.toml")
	machine.succeed("test $(stat -c %a /home/alice/.config/elamon/defaults.toml) = 444")
	machine.succeed("head -1 /home/alice/.config/elamon/defaults.toml | grep -q '^# elamond [0-9.]*: the settings it starts with'")

with subtest("a phone pairs with a profile of its own that uses the one in Nix, and gets its dashboard, the standard ones and the action"):
	code = pair_code("pair", "--profile", "vm")
	machine.wait_until_succeeds("journalctl -u pair -o cat | grep -Ec 'Profile: +its own, made in ~/.config/elamon/elamond.d/phones.toml, using vm — VM'", timeout=30)
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Test phone"})
	assert status == 200 and paired["role"] == "admin" and paired["profile"] == "test-phone", paired
	# Its table, in a drop-in of alice's alone.
	phones = "/home/alice/.config/elamon/elamond.d/phones.toml"
	machine.succeed(f"test $(stat -c %a:%U {phones}) = 600:alice")
	table = machine.succeed(f"cat {phones}")
	assert re.search(r'^# Test phone, paired \d{4}-\d\d-\d\d \([0-9a-f]+\)\n\[profiles\.test-phone\]\nname = "Test phone"\nuse = "vm"\n', table, re.M), table
	machine.wait_until_succeeds("journalctl -u pair -o cat | grep -c 'Its profile: *test-phone, in ~/.config/elamon/elamond.d/phones.toml, using vm'", timeout=30)
	auth = f"-H 'Authorization: Bearer {paired['token']}'"
	test_id = paired["client_id"]
	doc = ui(auth)
	assert doc["service"]["interval_ms"] > 0 and doc["service"]["history_seconds"] > 0, doc["service"]
	assert doc["service"]["version"] == hello["version"], doc["service"]
	features = doc["service"]["features"]
	assert "processes" in features and "control" not in features and features == sorted(features), doc["service"]
	assert doc["profile"]["id"] == "test-phone" and doc["profile"]["name"] == "Test phone" and doc["tabs"] == ["vm", "overview"], (doc["profile"], doc["tabs"])
	assert doc["profile"]["source"] == {"file": "elamond.d/phones.toml", "line": 5}, doc["profile"]
	# The dashboard from Nix, and the standard one the module's defaults give, with what its tiles open.
	ids = {d["id"] for d in doc["dashboards"]}
	assert {"vm", "overview", "header", "cpu", "memory"} <= ids, ids
	vm = next(d for d in doc["dashboards"] if d["id"] == "vm")
	kinds = {w["id"]: w["type"] for w in vm["widgets"]}
	assert kinds == {"vm/0": "value", "vm/1": "select", "vm/2": "table"}, kinds
	greet = next(a for a in doc["actions"] if a["id"] == "greet")
	assert greet["kind"] == "actions" and greet["params"][0]["id"] == "who", greet
	assert [c["value"] for c in greet["params"][0]["choices"]] == ["world", "nix"], greet
	# It said which app it is.
	machine.wait_until_succeeds("su - alice -c 'elamond clients' | grep -q 'android 0.6.0'", timeout=30)

# alice's user manager was started by hand, so logind stops it, and the service with it, ten
# seconds after her last session ends; each `su - alice` below ends one.
machine.succeed("loginctl enable-linger alice")

with subtest("the stream sends open, then the dashboard on screen"):
	out = machine.succeed(f"curl -sNk --max-time 4 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard=vm' || true")
	# The last line may be cut where the time ran out.
	lines = [json.loads(l) for l in out.splitlines()[:-1]]
	assert lines[0]["type"] == "open" and lines[0]["ui"] == doc["version"], lines[0]
	data = [l for l in lines if l["type"] == "data"]
	assert data and data[0]["dashboard"] == "vm" and "history" in data[0], lines
	assert isinstance(data[0]["widgets"]["vm/0"]["v"], (int, float)), data[0]
	assert data[0]["widgets"]["vm/1"]["enabled"] is False, data[0]

with subtest("a drop-in turns control on without a restart"):
	assert not ui(auth)["control"]["enabled"]
	machine.succeed("su - alice -c 'mkdir -p ~/.config/elamon/elamond.d && echo control = true > ~/.config/elamon/elamond.d/on.toml'")
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} https://localhost:47823/api/v1/ui | jq -e .control.enabled", timeout=30)

with subtest("the action from Nix runs with its parameter, and a value it doesn't take is refused"):
	status, result = call("POST", "/api/v1/actions/greet", auth, {"params": {"who": "nix"}})
	assert (status, result) == (200, {"ok": True, "exit_code": 0, "timed_out": False, "output": "hello nix\n"}), (status, result)
	assert call("POST", "/api/v1/actions/greet", auth, {"params": {"who": "mars"}}) == (400, {"error": "bad_param", "message": "who"})
	assert call("POST", "/api/v1/actions/greet", auth, {"params": {"who": "world", "then": "x"}}) == (400, {"error": "bad_param", "message": "then"})
	assert call("POST", "/api/v1/actions/nothing", auth, {}) == (404, {"error": "no_such_action"})
	machine.succeed("journalctl _UID=1000 -o cat | grep -c 'control: Test phone ran \"Greet\"'")

with subtest("systemctl reload rereads the settings"):
	# Nothing changes on disk here, so only the reload can add a reread to the log.
	rereads = "journalctl _UID=1000 -o cat | grep -c 'settings: reread' || true"
	machine.sleep(6)
	before = int(machine.succeed(rereads).strip() or 0)
	machine.succeed("systemctl --user -M alice@ reload elamond")
	machine.wait_until_succeeds(f"test $({rereads}) -gt {before}", timeout=30)

with subtest("settings that stop parsing hold control off until they parse again"):
	machine.succeed("su - alice -c 'echo \"control = \" > ~/.config/elamon/elamond.d/half.toml'")
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} https://localhost:47823/api/v1/ui | jq -e '.control.enabled == false and .control.off_reason == \"settings_unreadable\"'", timeout=30)
	machine.succeed("journalctl _UID=1000 -o cat | grep -c \"settings: they don't parse\"")
	assert "don't parse" in machine.succeed("su - alice -c 'elamond control status'")
	assert call("POST", "/api/v1/actions/greet", auth, {"params": {"who": "nix"}}) == (403, {"error": "control_disabled"})
	machine.succeed("su - alice -c 'rm ~/.config/elamon/elamond.d/half.toml'")
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} https://localhost:47823/api/v1/ui | jq -e '.control.enabled and .control.off_reason == null'", timeout=30)

with subtest("the unit gives the service room for its connections, and a PATH with the wrappers and the profiles"):
	pid = machine.succeed("systemctl --user -M alice@ show elamond -p MainPID --value").strip()
	limits = machine.succeed(f"grep 'Max open files' /proc/{pid}/limits")
	assert limits.split()[3] == "8192", limits
	path = machine.succeed(f"tr '\\0' '\\n' < /proc/{pid}/environ | grep ^PATH=")
	for d in ["/run/wrappers/bin", "/etc/profiles/per-user/alice/bin", "/run/current-system/sw/bin", "/home/alice/.nix-profile/bin"]:
		assert d in path.split("=", 1)[1].strip().split(":"), path

with subtest("a link from a drop-in shows in the links table"):
	machine.succeed("""printf '%s\\n' '[links.web]' 'name = "Web"' 'url = "http://{host}:8080"' > /home/alice/.config/elamon/elamond.d/links.toml""")
	machine.succeed("chown alice: /home/alice/.config/elamon/elamond.d/links.toml")
	machine.wait_until_succeeds(f"""curl -sk {APP} {auth} 'https://localhost:47823/api/v1/snapshot?dashboard=vm' | jq -e '[.widgets["vm/2"].rows[].f.name] == ["Web"]'""", timeout=30)
	# Without a unit, no unit actions; a unit no link may have makes settings that don't parse.
	assert not any(a["id"].startswith("unit.") for a in ui(auth)["actions"])

with subtest("elamond clients profile changes what ui says, and an open stream hears of it at once"):
	machine.succeed(f"systemd-run --unit=stream-watch /bin/sh -c \"/run/current-system/sw/bin/curl -sNk --max-time 60 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard=vm' > /tmp/stream.out\"")
	machine.wait_until_succeeds("grep -q '\"type\":\"data\"' /tmp/stream.out", timeout=30)
	out = machine.succeed(f"su - alice -c 'elamond clients profile {test_id} default'")
	assert "Now with default" in out, out
	machine.wait_until_succeeds("grep -q '\"type\":\"ui\"' /tmp/stream.out", timeout=30)
	doc = ui(auth)
	assert doc["profile"]["id"] == "default" and "vm" not in doc["tabs"], (doc["profile"], doc["tabs"])
	# The dashboard it had is gone for it: the stream goes on with the first tab.
	machine.wait_until_succeeds(f"grep '\"type\":\"data\"' /tmp/stream.out | tail -1 | jq -e '.dashboard == \"{doc['tabs'][0]}\"'", timeout=30)
	# Not its own table: one others may have too; its own stays, for it to have back.
	assert re.search(rf"^{test_id}\s+admin\s+default \(shared\)\s+Test phone\s+android 0\.6\.0\s+\(paired ", machine.succeed("su - alice -c 'elamond clients'"), re.M)
	machine.succeed("grep -qx '\\[profiles.test-phone\\]' /home/alice/.config/elamon/elamond.d/phones.toml")
	assert "no profile \"nope\"" in machine.fail(f"su - alice -c 'elamond clients profile {test_id} nope' 2>&1")
	machine.succeed("systemctl stop stream-watch || true")

with subtest("elamond pair asks before a phone is let in: no turns it away, yes lets it in, and nothing to ask on stops it"):
	# Under systemd-run its input is /dev/null: nobody can answer, so it doesn't pair at all.
	machine.succeed(f"systemd-run {AS_ALICE} --unit=pair-unasked /run/current-system/sw/bin/elamond pair")
	machine.wait_until_succeeds("journalctl -u pair-unasked -o cat | grep -c 'elamond pair --yes'", timeout=30)
	machine.fail("journalctl -u pair-unasked -o cat | grep -Ec '^ +[0-9]{3} [0-9]{3} *$'")
	# A person who answers once asked, through a pipe.
	machine.succeed("""cat > /tmp/answer.sh <<'EOF'
#!/bin/sh
# Answers elamond pair's question with $1 once it is asked; what it prints goes to $2.
(until grep -q 'Allow?' "$2" 2>/dev/null; do sleep 0.2; done; echo "$1") | timeout 120 elamond pair 2>&1 | tee "$2"
EOF
chmod 755 /tmp/answer.sh""")
	codes = {}
	for answer, device in [("n", "Stranger"), ("y", "Asked phone")]:
		out = f"/tmp/pair-{answer}.out"
		machine.succeed(f"systemd-run {AS_ALICE} -p Environment=PATH=/run/current-system/sw/bin --unit=pair-ask-{answer} /tmp/answer.sh {answer} {out}")
		machine.wait_until_succeeds(f"grep -Eq '^ +[0-9]{{3}} [0-9]{{3}} *$' {out}", timeout=30)
		code = codes[answer] = machine.succeed(f"grep -E '^ +[0-9]{{3}} [0-9]{{3}} *$' {out} | head -1 | tr -d ' \\n'")
		status, body = call("POST", "/api/v1/pair", body={"code": code, "device": device})
		asked = machine.succeed(f"cat {out}")
		assert re.search(rf'Phone "{device}" from (127\.0\.0\.1|::1) asks to pair as admin\. Allow\? \[y/N\] {answer}$', asked, re.M), asked
		clients = machine.succeed("su - alice -c 'elamond clients'")
		if answer == "n":
			assert (status, body) == (403, {"error": "pair_declined"}), (status, body)
			assert device not in clients, clients
			machine.wait_until_succeeds(f"grep -q 'turned away' {out}", timeout=30)
			machine.succeed(f"journalctl _UID=1000 -o cat | grep -c 'pairing: \"{device}\" from .* not added: turned away at elamond pair'")
		else:
			assert status == 200 and body["role"] == "admin" and body["profile"] == "asked-phone", (status, body)
			assert device in clients, clients
			machine.wait_until_succeeds(f"grep -q 'Paired with \"{device}\"' {out}", timeout=30)
	# The code a phone was turned away with doesn't work again.
	assert call("POST", "/api/v1/pair", body={"code": codes["n"], "device": "Again"}) == (403, {"error": "no_pairing"})

with subtest("a phone paired as glance sees the basics and may run nothing"):
	code = pair_code("pair-glance", "--role", "glance")
	machine.wait_until_succeeds("journalctl -u pair-glance -o cat | grep -c 'Role: *glance'", timeout=30)
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Glance phone"})
	assert status == 200 and paired["role"] == "glance", paired
	glance, glance_id = f"-H 'Authorization: Bearer {paired['token']}'", paired["client_id"]
	doc = ui(glance)
	assert doc["role"] == "glance" and doc["actions"] == [] and "processes" not in doc["tabs"], doc
	assert call("GET", "/api/v1/snapshot?dashboard=processes", glance) == (404, {"error": "no_such_dashboard"})
	_, snap = call("GET", "/api/v1/snapshot", glance)
	assert snap["dashboard"] == doc["tabs"][0] and snap["widgets"], snap
	# The admin phone, asking the same, gets the processes; they take a reading or two to come.
	machine.wait_until_succeeds(f"curl -sk {APP} {auth} 'https://localhost:47823/api/v1/snapshot?dashboard=processes' | jq -e '.widgets[\"processes/table\"].total > 0'", timeout=30)
	assert call("POST", "/api/v1/actions/greet", glance, {"params": {"who": "nix"}}) == (403, {"error": "not_allowed"})

with subtest("elamond control off stops every phone at once, until elamond control on"):
	greet = {"params": {"who": "world"}}
	assert call("POST", "/api/v1/actions/greet", auth, greet)[0] == 200
	machine.succeed("su - alice -c 'elamond control off'")
	assert call("POST", "/api/v1/actions/greet", auth, greet) == (403, {"error": "control_disabled"})
	assert ui(auth)["control"] == {"enabled": False, "off_reason": "held_off"}
	machine.succeed("su - alice -c 'elamond control status' | grep 'held off'")
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -c 'control: held off'", timeout=30)
	machine.succeed("su - alice -c 'elamond control on'")
	assert call("POST", "/api/v1/actions/greet", auth, greet)[0] == 200

with subtest("elamond clients role takes effect at once, and the log shows what was refused"):
	clients = machine.succeed("su - alice -c 'elamond clients'")
	assert re.search(rf"^{glance_id}\s+glance\s+glance-phone\s+Glance phone\s", clients, re.M), clients
	machine.succeed(f"su - alice -c 'elamond clients role {glance_id} monitor'")
	# Processes take a reading or two to come.
	machine.wait_until_succeeds(f"curl -sk {APP} {glance} 'https://localhost:47823/api/v1/snapshot?dashboard=processes' | jq -e '.widgets[\"processes/table\"].total > 0'", timeout=30)
	doc = ui(glance)
	assert doc["role"] == "monitor" and doc["actions"] == [] and "processes" in doc["tabs"], doc
	assert call("POST", "/api/v1/actions/greet", glance, {"params": {"who": "nix"}}) == (403, {"error": "not_allowed"})
	rows = activity(glance, ("Glance phone", "action Greet nix", "not_allowed"))
	seen = {(r.get("phone"), r["what"], r["result"]) for r in rows}
	assert ("Test phone", "action Greet nix", "control_disabled") in seen, rows
	assert ("Test phone", "action Greet nix", "ok") in seen, rows
	assert ("Test phone", "action Greet world", "control_disabled") in seen, rows
	assert rows[0]["t"] >= rows[-1]["t"], rows
	machine.succeed("journalctl _UID=1000 -o cat | grep -c '\"Glance phone\" (.*) is now monitor'")

with subtest("streams are in the journal: which phone, from where, for what, for how long"):
	# One stream of a phone is logged in ten seconds; those in between are only counted.
	opened = "journalctl _UID=1000 -o cat | grep -Ec 'stream: \"Test phone\" \\(admin\\) opened from [0-9a-f:.]+: dashboard processes, every [0-9]+ ms, processes'"
	machine.wait_until_succeeds(f"curl -sk --max-time 3 {APP} {auth} 'https://localhost:47823/api/v1/stream?dashboard=processes' >/dev/null || true; {opened}", timeout=60)
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -c 'stream: \"Test phone\" closed after [0-9]* s: connection closed'", timeout=30)

with subtest("five wrong codes from one address shut it out of that code"):
	code = pair_code("pair-guess")
	wrong = f"{(int(code) + 1) % 1_000_000:06d}"
	for _ in range(5):
		assert call("POST", "/api/v1/pair", body={"code": wrong}) == (403, {"error": "wrong_code"})
	assert call("POST", "/api/v1/pair", body={"code": code}) == (429, {"error": "too_many_requests"})
	machine.succeed("systemctl stop pair-guess")

with subtest("openFirewall with firewallInterfaces opens the port on those alone, mDNS's not over a VPN"):
	rules = machine.succeed("iptables-save 2>/dev/null || nft list ruleset")
	def opened(iface, proto, port):
		return re.search(rf'(-i {iface} -p {proto} -m {proto} --dport {port} |iifname "{iface}" {proto} dport (\{{ )?{port}\b)', rules)
	assert opened("eth1", "tcp", 47823) and opened("eth1", "udp", 5353), rules
	assert opened("wg0", "tcp", 47823) and not opened("wg0", "udp", 5353), rules
	assert not re.search(r'(-A nixos-fw -p tcp -m tcp --dport 47823 |^\s*tcp dport (\{ )?47823\b)', rules, re.M), rules

def second(*args, unit, path=None):
	"""elamond as alice with the settings and state in /tmp/second, `path` before the system's PATH."""
	env = ["HOME=/home/alice", "ELAMON_CONFIG_DIR=/tmp/second/cfg", "ELAMON_DATA_DIR=/tmp/second/data", "ELAMON_CONFIG=/tmp/second/cfg/elamond.toml"]
	if path:
		env.append(f"PATH={path}:/run/current-system/sw/bin")
	env = " ".join(f"-p Environment={e}" for e in env)
	machine.succeed(f"systemd-run --uid=alice {env} --unit={unit} /run/current-system/sw/bin/timeout 300 /run/current-system/sw/bin/elamond {' '.join(args)}")

with subtest("listen names interfaces: eth1 at once, dummy0 once it has an address and until it loses it; mDNS on eth1 alone"):
	machine.succeed("mkdir -p /tmp/second/cfg /tmp/second/data")
	machine.succeed("""printf '%s\\n' 'port = 47900' 'listen = ["eth1", "dummy0"]' 'name = "Second"' > /tmp/second/cfg/elamond.toml""")
	machine.succeed("chown -R alice: /tmp/second")
	second("serve", unit="second")
	eth1 = machine.succeed("ip -4 -o addr show eth1 | awk '{print $4}' | cut -d/ -f1").strip()
	eth0 = machine.succeed("ip -4 -o addr show eth0 | awk '{print $4}' | cut -d/ -f1").strip()
	machine.wait_until_succeeds(f"curl -sk https://{eth1}:47900/api/v1/hello | jq -e '.name == \"Second\"'", timeout=60)
	# Not on loopback or the other interface, which it wasn't told to listen on.
	machine.fail("curl -sk --max-time 3 https://127.0.0.1:47900/api/v1/hello")
	if eth0:
		machine.fail(f"curl -sk --max-time 3 https://{eth0}:47900/api/v1/hello")
	journal = lambda: machine.succeed("journalctl -u second -o cat")
	assert "dummy0 has no address yet" in journal(), journal()
	machine.wait_until_succeeds("journalctl -u second -o cat | grep -cx 'mdns: announced on eth1'", timeout=30)
	# An interface that comes up after the service: taken within seconds.
	machine.succeed("ip link add dummy0 type dummy && ip addr add 10.99.0.1/24 dev dummy0 && ip link set dummy0 up")
	machine.wait_until_succeeds("curl -sk https://10.99.0.1:47900/api/v1/hello | jq -e '.name == \"Second\"'", timeout=30)
	assert "listen: on 10.99.0.1:47900 (dummy0)" in journal(), journal()
	# A dummy interface is virtual: mDNS stays on eth1.
	assert journal().count("mdns: announced on") == 1, journal()
	# An address that goes away is let go, and the service goes on.
	machine.succeed("ip addr del 10.99.0.1/24 dev dummy0")
	machine.wait_until_succeeds("journalctl -u second -o cat | grep -c 'listen: no longer on 10.99.0.1:47900'", timeout=30)
	machine.fail("ss -ltn | grep -q '10.99.0.1:47900'")
	machine.succeed(f"curl -sk https://{eth1}:47900/api/v1/hello")

with subtest("elamond pair offers the addresses listened on, and finds the service there"):
	second("pair", "--yes", unit="second-pair")
	machine.wait_until_succeeds("journalctl -u second-pair -o cat | grep -c 'Addresses:'", timeout=30)
	out = machine.succeed("journalctl -u second-pair -o cat")
	assert re.search(rf"Addresses: +{re.escape(eth1)}  port 47900$", out, re.M), out
	assert "dummy0 once it has an address" in out and "isn't answering" not in out, out
	machine.succeed("systemctl stop second-pair second")

with subtest("elamond pair offers names after the addresses: those in the settings, and Tailscale's where it listens"):
	# A stand-in for Tailscale: its interface with an address of Tailscale's, and a tailscale that
	# answers `status` as the real one does, or hangs.
	machine.succeed("ip link add tailscale0 type dummy && ip addr add 100.100.7.1/32 dev tailscale0 && ip link set tailscale0 up")
	machine.succeed("""mkdir -p /tmp/fakebin && cat > /tmp/fakebin/tailscale <<'EOF'
#!/bin/sh
[ -e /tmp/fakebin/hang ] && exec sleep 60
[ "$*" = "status --json --peers=false" ] || exit 2
echo '{"BackendState": "Running", "Self": {"DNSName": "second.tail-test.ts.net.", "TailscaleIPs": ["100.100.7.1"]}, "CurrentTailnet": {"MagicDNSEnabled": true}}'
EOF
chmod 755 /tmp/fakebin/tailscale""")
	def offered(listen, unit):
		machine.succeed(f"""printf '%s\\n' 'port = 47900' 'listen = {listen}' 'names = ["second.example.net"]' > /tmp/second/cfg/elamond.toml""")
		second("pair", "--yes", unit=unit, path="/tmp/fakebin")
		machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -c 'Waiting for the phone'", timeout=30)
		out = machine.succeed(f"journalctl -u {unit} -o cat")
		machine.succeed(f"systemctl stop {unit}")
		return out
	out = offered('["eth1", "tailscale0"]', "second-names")
	assert re.search(rf"Addresses: +{re.escape(eth1)}, 100\.100\.7\.1  port 47900$", out, re.M), out
	assert re.search(r"Names: +second\.example\.net, second\.tail-test\.ts\.net$", out, re.M), out
	# Not listening on Tailscale: its name isn't offered.
	out = offered('["eth1"]', "second-names-lan")
	assert re.search(r"Names: +second\.example\.net$", out, re.M), out
	# A Tailscale that doesn't answer holds pairing up for a moment, then is done without.
	machine.succeed("touch /tmp/fakebin/hang")
	out = offered('["eth1", "tailscale0"]', "second-names-hang")
	assert re.search(r"Names: +second\.example\.net$", out, re.M), out
	machine.succeed("ip link del tailscale0")

with subtest("elamond rotate-cert: a new certificate signed by the old key, the phone's pairing kept"):
	served_fp = "openssl s_client -connect localhost:47823 </dev/null 2>/dev/null | openssl x509 -outform DER | sha256sum | cut -d' ' -f1"
	served_key = "openssl s_client -connect localhost:47823 </dev/null 2>/dev/null | openssl x509 -pubkey -noout | openssl pkey -pubin -outform DER | openssl dgst -sha256 -binary | base64"
	old_fp = machine.succeed(served_fp).strip()
	old_key = machine.succeed(served_key).strip()
	machine.succeed(f"curl -sk --pinnedpubkey 'sha256//{old_key}' https://localhost:47823/api/v1/hello")
	assert call("GET", "/api/v1/identity/rotations") == (200, [])
	machine.succeed("su - alice -c 'elamond rotate-cert' | grep 'New certificate'")
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -c 'certificate: now'", timeout=30)
	new_fp = machine.succeed(served_fp).strip()
	assert new_fp != old_fp
	# What the phone pinned no longer holds...
	machine.fail(f"curl -sk --pinnedpubkey 'sha256//{old_key}' https://localhost:47823/api/v1/hello")
	# ...but the service shows, with no token, how the new certificate follows from it.
	status, chain = call("GET", "/api/v1/identity/rotations")
	assert status == 200 and len(chain) == 1, chain
	step = chain[0]
	assert (step["format"], step["old_fp"], step["new_fp"]) == ("elamon-rotate-v1", old_fp, new_fp), step
	machine.succeed(f"echo {step['old_cert']} | base64 -d > /tmp/old.der && echo {step['signature']} | base64 -d > /tmp/sig")
	machine.succeed(f"printf '%s\\n%s\\n%s\\n%s' {step['format']} {old_fp} {new_fp} {step['t']} > /tmp/signed")
	assert machine.succeed("sha256sum /tmp/old.der").split()[0] == old_fp
	machine.succeed("openssl x509 -inform DER -in /tmp/old.der -pubkey -noout > /tmp/old.pem")
	machine.succeed("openssl dgst -sha256 -verify /tmp/old.pem -signature /tmp/sig /tmp/signed")
	# The same phone, its token unchanged, carries on.
	assert call("GET", "/api/v1/ui", auth)[0] == 200
	# And the chain outlives a restart.
	machine.succeed("systemctl --user -M alice@ restart elamond")
	machine.wait_until_succeeds("curl -sk https://localhost:47823/api/v1/identity/rotations | jq -e 'length == 1'", timeout=60)
	assert machine.succeed(served_fp).strip() == new_fp

with subtest("elamond backup and restore: a wiped service comes back as the same computer, its phones paired"):
	code = pair_code("pair-backup")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Backup phone"})
	assert status == 200, paired
	kept = f"-H 'Authorization: Bearer {paired['token']}'"
	fp = machine.succeed(served_fp).strip()
	machine.succeed("su - alice -c 'umask 077; echo \"two words or more\" > ~/pass; echo \"wrong words\" > ~/wrong'")
	out = machine.succeed("su - alice -c 'elamond backup ~/b.elamond --passphrase-file ~/pass'")
	assert "Backup phone" in out and "Argon2id" in out, out
	machine.succeed("test $(stat -c %a /home/alice/b.elamond) = 600 && head -1 /home/alice/b.elamond | grep -qx elamond-backup")
	# Nothing of the state shows in the file.
	machine.fail(f"grep -q {paired['client_id']} /home/alice/b.elamond")
	# While the service runs, a restore is refused.
	out = machine.fail("su - alice -c 'elamond restore ~/b.elamond --passphrase-file ~/pass' 2>&1")
	assert "service is running" in out and "systemctl --user stop elamond" in out, out
	# A reinstall: the data and the drop-in gone, the service makes a new identity and turns the phone away.
	machine.succeed("systemctl --user -M alice@ stop elamond")
	machine.succeed("rm -rf /home/alice/.local/share/elamon /home/alice/.config/elamon/elamond.d")
	machine.succeed("systemctl --user -M alice@ start elamond")
	machine.wait_until_succeeds("curl -sk https://localhost:47823/api/v1/hello", timeout=60)
	assert machine.succeed(served_fp).strip() != fp
	assert call("GET", "/api/v1/ui", kept) == (401, {"error": "unauthorized"})
	machine.succeed("systemctl --user -M alice@ stop elamond")
	# A wrong passphrase changes nothing.
	before = machine.succeed("ls /home/alice/.local/share/elamon")
	out = machine.fail("su - alice -c 'elamond restore ~/b.elamond --passphrase-file ~/wrong' 2>&1")
	assert "passphrase is wrong" in out, out
	assert machine.succeed("ls /home/alice/.local/share/elamon") == before
	# Restored, settings too: the same certificate, and the phones' tokens work again.
	out = machine.succeed("su - alice -c 'elamond restore ~/b.elamond --passphrase-file ~/pass --with-settings'")
	assert "Backup phone" in out and "Test phone" in out and "cert.der.before-restore-" in out, out
	# The settings from Nix stay where Nix puts them.
	assert "in the Nix store" in out, out
	machine.succeed("grep -qx 'control = true' /home/alice/.config/elamon/elamond.d/on.toml")
	# A phone unpaired since the backup doesn't come back unasked.
	backup_id = paired["client_id"]
	machine.succeed(f"su - alice -c 'elamond revoke {backup_id}'")
	out = machine.fail("su - alice -c 'elamond restore ~/b.elamond --passphrase-file ~/pass' 2>&1")
	assert "aren't paired here now" in out and "Backup phone" in out and "--with-old-phones" in out, out
	machine.succeed("su - alice -c 'elamond restore ~/b.elamond --passphrase-file ~/pass --with-old-phones'")
	machine.succeed("systemctl --user -M alice@ start elamond")
	machine.wait_until_succeeds("curl -sk https://localhost:47823/api/v1/hello", timeout=60)
	assert machine.succeed(served_fp).strip() == fp
	assert call("GET", "/api/v1/ui", kept)[0] == 200
	assert call("GET", "/api/v1/ui", auth)[0] == 200
	# With the rotation phones follow, and the drop-in that turned control on.
	assert len(call("GET", "/api/v1/identity/rotations")[1]) == 1
	assert ui(kept)["control"]["enabled"]

with subtest("a phone that came by another's token gets a key of its own, and the old phone is revoked alone"):
	code = pair_code("pair-move")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Old phone"})
	assert status == 200 and paired["role"] == "admin", paired
	old_id = paired["client_id"]
	# Import pairings on a new phone: the old phone's token, from another client.
	shared = f"-H 'Authorization: Bearer {paired['token']}'"
	assert call("GET", "/api/v1/ui", shared)[0] == 200
	status, own = call("POST", "/api/v1/rekey", shared, {"device": "New phone"})
	assert status == 200 and own["role"] == "admin" and own["client_id"] != old_id and own["token"] != paired["token"], own
	mine = f"-H 'Authorization: Bearer {own['token']}'"
	# One key of its own per token: not again, and not from the key that came of it.
	assert call("POST", "/api/v1/rekey", shared, {"device": "Third phone"}) == (403, {"error": "rekey_refused"})
	assert call("POST", "/api/v1/rekey", mine, {"device": "Third phone"}) == (403, {"error": "rekey_refused"})
	clients = machine.succeed("su - alice -c 'elamond clients'")
	assert re.search(rf"^{old_id}\s+admin\s+old-phone\s+Old phone\s+android 0\.6\.0\s+\(paired ", clients, re.M), clients
	# A table of its own too, using the profile of the phone whose key it came by.
	assert own["profile"] == "new-phone", own
	assert re.search(rf"^{own['client_id']}\s+admin\s+new-phone\s+New phone\s+\S+(\s0\.6\.0)?\s+\(added \S+ with the key of {old_id}\)", clients, re.M), clients
	machine.succeed("grep -A2 -x '\\[profiles.new-phone\\]' /home/alice/.config/elamon/elamond.d/phones.toml | grep -qx 'use = \"old-phone\"'")
	# The old token works until it is revoked; then only the new one does.
	assert call("GET", "/api/v1/ui", shared)[0] == 200
	out = machine.succeed(f"su - alice -c 'elamond revoke {old_id}'")
	assert "Unpaired 1 phone(s): Old phone" in out and "STILL PAIRED" in out and own["client_id"] in out, out
	# Its table stays: the new phone's uses it.
	assert "its profile old-phone stays" in out and "new-phone uses it" in out, out
	assert call("GET", "/api/v1/ui", shared) == (401, {"error": "unauthorized"})
	assert ui(mine)["role"] == "admin"
	activity(mine, ("Old phone", "phone added: New phone", "ok"))
	machine.succeed("journalctl _UID=1000 -o cat | grep -c 'pairing: added \"New phone\" .* asked with the key of \"Old phone\"'")

with subtest("elamond rotate-cert --no-statement: a new certificate that nothing leads to, for a key that may have leaked"):
	before = machine.succeed(served_fp).strip()
	out = machine.succeed("su - alice -c 'elamond rotate-cert --no-statement'")
	assert "elamond revoke all" in out and "nothing signed with it" in out, out
	machine.wait_until_succeeds(f"test \"$({served_fp})\" != {before}", timeout=30)
	assert call("GET", "/api/v1/identity/rotations") == (200, [])
	assert call("GET", "/api/v1/ui", mine)[0] == 200
