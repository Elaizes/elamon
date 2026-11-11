import json
import re

machine.wait_for_unit("multi-user.target")

with subtest("the greeter and users not listed run no service"):
	machine.succeed("systemctl start user@990.service user@1001.service")
	machine.sleep(5)
	machine.fail("ss -ltn | grep -q :47823")

with subtest("the listed user's service takes the port"):
	machine.succeed("systemctl start user@1000.service")
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)
	hello = json.loads(machine.succeed("curl -sk https://localhost:47823/api/v1/hello"))
	assert hello["app"] == "elamon" and "min_app" in hello, hello

with subtest("a phone pairs"):
	machine.succeed("systemd-run --uid=alice -p Environment=HOME=/home/alice --unit=pair /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair")
	machine.wait_until_succeeds("journalctl -u pair -o cat | grep -Eq '^ +[0-9]{3} [0-9]{3} *$'", timeout=30)
	code = machine.succeed("journalctl -u pair -o cat | grep -E '^ +[0-9]{3} [0-9]{3} *$' | head -1 | tr -d ' \\n'")
	body = json.dumps({"code": code, "device": "Test phone"})
	token = json.loads(machine.succeed(f"curl -sk -X POST -H 'Content-Type: application/json' -d '{body}' https://localhost:47823/api/v1/pair"))["token"]
	auth = f"-H 'Authorization: Bearer {token}'"
	info = json.loads(machine.succeed(f"curl -sk {auth} https://localhost:47823/api/v1/info"))
	assert info["interval_ms"] > 0 and info["history_seconds"] > 0, info
	# service_version is the old name, kept for apps 0.4.
	assert info["service_version"] == info["service_version"] == hello["version"], info

with subtest("a drop-in turns control on without a restart"):
	assert not json.loads(machine.succeed(f"curl -sk {auth} https://localhost:47823/api/v1/control"))["enabled"]
	machine.succeed("su - alice -c 'mkdir -p ~/.config/elamon/elamond.d && echo control = true > ~/.config/elamon/elamond.d/on.toml'")
	machine.wait_until_succeeds(f"curl -sk {auth} https://localhost:47823/api/v1/control | jq -e .enabled", timeout=30)

with subtest("systemctl reload rereads the settings"):
	# Nothing changes on disk here, so only the reload can add a reread to the log.
	rereads = "journalctl _UID=1000 -o cat | grep -c 'settings: reread' || true"
	machine.sleep(6)
	before = int(machine.succeed(rereads).strip() or 0)
	machine.succeed("systemctl --user -M alice@ reload elamond")
	machine.wait_until_succeeds(f"test $({rereads}) -gt {before}", timeout=30)

with subtest("an elamond.toml from before 0.5, with [[services]], still works"):
	machine.succeed("""printf '%s\\n' '[[services]]' 'name = "Old style"' 'url = "http://{host}:8080"' > /home/alice/.config/elamon/elamond.toml""")
	machine.succeed("chown alice: /home/alice/.config/elamon/elamond.toml")
	# Its old routes and the old name in samples stay for apps 0.4.
	for path in ["links", "services"]:
		machine.wait_until_succeeds(f"""curl -sk {auth} https://localhost:47823/api/v1/{path} | jq -e '.[0].name == "Old style"'""", timeout=30)
	sample = json.loads(machine.succeed(f"curl -sk {auth} https://localhost:47823/api/v1/snapshot"))
	assert sample["links"][0]["name"] == "Old style" and sample["links"] == sample["services"], sample
	# The drop-in beside it still applies, and the old name is pointed out once.
	machine.succeed(f"curl -sk {auth} https://localhost:47823/api/v1/control | jq -e .enabled")
	machine.succeed("journalctl _UID=1000 -o cat | grep -c 'elamond.toml is read for now' | grep -qx 1")

def call(method, path, auth="", body=None):
	"""The status and the JSON body of a request to the service."""
	data = f"-H 'Content-Type: application/json' -d '{json.dumps(body)}'" if body is not None else ""
	out = machine.succeed(f"curl -sk -X {method} {auth} {data} -w '\\n%{{http_code}}' https://localhost:47823{path}")
	text, status = out.rsplit("\n", 1)
	return int(status), json.loads(text) if text.strip() else None

def pair_code(unit, *args):
	"""Runs `elamond pair` as alice in a unit of its own and returns the code it prints."""
	machine.succeed(f"systemd-run --uid=alice -p Environment=HOME=/home/alice --unit={unit} /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair {' '.join(args)}")
	pattern = "'^ +[0-9]{3} [0-9]{3} *$'"
	machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -Eq {pattern}", timeout=30)
	return machine.succeed(f"journalctl -u {unit} -o cat | grep -E {pattern} | head -1 | tr -d ' \\n'")

# alice's user manager was started by hand, so logind stops it, and the service with it, ten
# seconds after her last session ends; the `su - alice` below would end more than one.
machine.succeed("loginctl enable-linger alice")

# An action no computer has: let through, it fails as unknown, and nothing is done.
nothing = {"action": "nothing"}
unknown = (502, {"error": "failed", "message": "unknown_action"})

with subtest("a phone paired as glance sees the basics and may do nothing"):
	code = pair_code("pair-glance", "--role", "glance")
	machine.succeed("journalctl -u pair-glance -o cat | grep -q 'Role: *glance'")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Glance phone"})
	assert status == 200 and paired["role"] == "glance", paired
	glance, glance_id = f"-H 'Authorization: Bearer {paired['token']}'", paired["client_id"]
	_, sample = call("GET", "/api/v1/snapshot?procs=5&media=1", glance)
	assert not {"processes", "process_count", "media"} & sample.keys(), sample.keys()
	assert sample["displays"] == sample["links"] == sample["services"] == [], sample
	assert sample["cpu"]["cores"] and sample["memory"]["total"] > 0, sample
	# The admin phone, asking the same, gets it all; processes take a reading or two to come.
	machine.wait_until_succeeds(f"curl -sk {auth} 'https://localhost:47823/api/v1/snapshot?procs=5&media=1' | jq -e '(.links | length) > 0 and .process_count > 0 and has(\"media\")'", timeout=30)
	_, control = call("GET", "/api/v1/control", glance)
	assert control["enabled"] and control["role"] == "glance" and control["allowed"] == [], control
	assert call("POST", "/api/v1/control/power", glance, nothing) == (403, {"error": "not_allowed"})
	assert call("GET", "/api/v1/links", glance) == (403, {"error": "not_allowed"})

with subtest("elamond control off stops every phone at once, until elamond control on"):
	assert call("POST", "/api/v1/control/power", auth, nothing) == unknown
	machine.succeed("su - alice -c 'elamond control off'")
	assert call("POST", "/api/v1/control/power", auth, nothing) == (403, {"error": "control_disabled"})
	_, control = call("GET", "/api/v1/control", auth)
	assert not control["enabled"] and control["role"] == "admin" and "power" not in control["allowed"], control
	machine.succeed("su - alice -c 'elamond control status' | grep -q 'held off'")
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -q 'control: held off'", timeout=30)
	machine.succeed("su - alice -c 'elamond control on'")
	assert call("POST", "/api/v1/control/power", auth, nothing) == unknown

with subtest("elamond clients role takes effect at once, and the log shows what was refused"):
	clients = machine.succeed("su - alice -c 'elamond clients'")
	assert re.search(rf"^{glance_id}\s+glance\s+Glance phone", clients, re.M), clients
	machine.succeed(f"su - alice -c 'elamond clients role {glance_id} monitor'")
	# Processes take a reading or two to come.
	machine.wait_until_succeeds(f"curl -sk {glance} 'https://localhost:47823/api/v1/snapshot?procs=5' | jq -e '(.processes | length) > 0 and (.links | length) > 0'", timeout=30)
	_, control = call("GET", "/api/v1/control", glance)
	assert control["role"] == "monitor" and "processes" in control["allowed"] and "power" not in control["allowed"], control
	assert call("POST", "/api/v1/control/power", glance, nothing) == (403, {"error": "not_allowed"})
	_, log = call("GET", "/api/v1/control/log", glance)
	seen = {(e["client_id"], e["what"], e["result"]) for e in log}
	assert (glance_id, "power nothing", "not_allowed") in seen, log
	assert ("power nothing", "control_disabled") in {(e["what"], e["result"]) for e in log}, log
	assert log[0]["t"] >= log[-1]["t"], log
	machine.succeed("journalctl _UID=1000 -o cat | grep -q '\"Glance phone\" (.*) is now monitor'")

with subtest("streams are in the journal: which phone, from where, for how long"):
	machine.succeed(f"curl -sk --max-time 3 {auth} 'https://localhost:47823/api/v1/stream?procs=5' >/dev/null || true")
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -Eq 'stream: \"Test phone\" \\(admin\\) opened from [0-9a-f:.]+: every [0-9]+ ms, 5 processes'", timeout=30)
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -q 'stream: \"Test phone\" closed after [0-9]* s: connection closed'", timeout=30)

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

def second(*args, unit):
	"""elamond as alice with the settings and state in /tmp/second."""
	env = " ".join(f"-p Environment={e}" for e in ["HOME=/home/alice", "ELAMON_CONFIG_DIR=/tmp/second/cfg", "ELAMON_DATA_DIR=/tmp/second/data", "ELAMON_CONFIG=/tmp/second/cfg/elamond.toml"])
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
	machine.wait_until_succeeds("journalctl -u second -o cat | grep -qx 'mdns: announced on eth1'", timeout=30)
	# An interface that comes up after the service: taken within seconds.
	machine.succeed("ip link add dummy0 type dummy && ip addr add 10.99.0.1/24 dev dummy0 && ip link set dummy0 up")
	machine.wait_until_succeeds("curl -sk https://10.99.0.1:47900/api/v1/hello | jq -e '.name == \"Second\"'", timeout=30)
	assert "listen: on 10.99.0.1:47900 (dummy0)" in journal(), journal()
	# A dummy interface is virtual: mDNS stays on eth1.
	assert journal().count("mdns: announced on") == 1, journal()
	# An address that goes away is let go, and the service goes on.
	machine.succeed("ip addr del 10.99.0.1/24 dev dummy0")
	machine.wait_until_succeeds("journalctl -u second -o cat | grep -q 'listen: no longer on 10.99.0.1:47900'", timeout=30)
	machine.fail("ss -ltn | grep -q '10.99.0.1:47900'")
	machine.succeed(f"curl -sk https://{eth1}:47900/api/v1/hello")

with subtest("elamond pair offers the addresses listened on, and finds the service there"):
	second("pair", unit="second-pair")
	machine.wait_until_succeeds("journalctl -u second-pair -o cat | grep -q 'Addresses:'", timeout=30)
	out = machine.succeed("journalctl -u second-pair -o cat")
	assert re.search(rf"Addresses: +{re.escape(eth1)}  port 47900$", out, re.M), out
	assert "dummy0 once it has an address" in out and "isn't answering" not in out, out
	machine.succeed("systemctl stop second-pair second")

with subtest("elamond rotate-cert: a new certificate signed by the old key, the phone's pairing kept"):
	served_fp = "openssl s_client -connect localhost:47823 </dev/null 2>/dev/null | openssl x509 -outform DER | sha256sum | cut -d' ' -f1"
	served_key = "openssl s_client -connect localhost:47823 </dev/null 2>/dev/null | openssl x509 -pubkey -noout | openssl pkey -pubin -outform DER | openssl dgst -sha256 -binary | base64"
	old_fp = machine.succeed(served_fp).strip()
	old_key = machine.succeed(served_key).strip()
	machine.succeed(f"curl -sk --pinnedpubkey 'sha256//{old_key}' https://localhost:47823/api/v1/hello")
	assert call("GET", "/api/v1/identity/rotations") == (200, [])
	machine.succeed("su - alice -c 'elamond rotate-cert' | grep -q 'New certificate'")
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -q 'certificate: now'", timeout=30)
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
	assert call("GET", "/api/v1/info", auth)[0] == 200
	# And the chain outlives a restart.
	machine.succeed("systemctl --user -M alice@ restart elamond")
	machine.wait_until_succeeds("curl -sk https://localhost:47823/api/v1/identity/rotations | jq -e 'length == 1'", timeout=60)
	assert machine.succeed(served_fp).strip() == new_fp
