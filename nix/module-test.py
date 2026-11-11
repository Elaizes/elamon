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
	machine.succeed("systemd-run --uid=alice -p Environment=HOME=/home/alice --unit=pair /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair --yes")
	machine.wait_until_succeeds("journalctl -u pair -o cat | grep -Eq '^ +[0-9]{3} [0-9]{3} *$'", timeout=30)
	code = machine.succeed("journalctl -u pair -o cat | grep -E '^ +[0-9]{3} [0-9]{3} *$' | head -1 | tr -d ' \\n'")
	body = json.dumps({"code": code, "device": "Test phone"})
	token = json.loads(machine.succeed(f"curl -sk -X POST -H 'Content-Type: application/json' -d '{body}' https://localhost:47823/api/v1/pair"))["token"]
	auth = f"-H 'Authorization: Bearer {token}'"
	info = json.loads(machine.succeed(f"curl -sk {auth} https://localhost:47823/api/v1/info"))
	assert info["interval_ms"] > 0 and info["history_seconds"] > 0, info
	assert info["service_version"] == hello["version"], info

# alice's user manager was started by hand, so logind stops it, and the service with it, ten
# seconds after her last session ends; each `su - alice` below ends one.
machine.succeed("loginctl enable-linger alice")

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

with subtest("settings that stop parsing hold control off until they parse again"):
	machine.succeed("su - alice -c 'echo \"control = \" > ~/.config/elamon/elamond.d/half.toml'")
	machine.wait_until_succeeds(f"curl -sk {auth} https://localhost:47823/api/v1/control | jq -e '.enabled == false and .off_reason == \"settings_unreadable\"'", timeout=30)
	machine.succeed("journalctl _UID=1000 -o cat | grep -q \"settings: they don't parse\"")
	assert "don't parse" in machine.succeed("su - alice -c 'elamond control status'")
	machine.succeed("su - alice -c 'rm ~/.config/elamon/elamond.d/half.toml'")
	machine.wait_until_succeeds(f"curl -sk {auth} https://localhost:47823/api/v1/control | jq -e '.enabled and .off_reason == null'", timeout=30)

with subtest("the unit gives the service room for its connections, and a PATH with the wrappers and the profiles"):
	pid = machine.succeed("systemctl --user -M alice@ show elamond -p MainPID --value").strip()
	limits = machine.succeed(f"grep 'Max open files' /proc/{pid}/limits")
	assert limits.split()[3] == "8192", limits
	path = machine.succeed(f"tr '\\0' '\\n' < /proc/{pid}/environ | grep ^PATH=")
	for d in ["/run/wrappers/bin", "/etc/profiles/per-user/alice/bin", "/run/current-system/sw/bin", "/home/alice/.nix-profile/bin"]:
		assert d in path.split("=", 1)[1].strip().split(":"), path

with subtest("a link from the app opens only a web page, and carries only a unit the settings name"):
	def post_link(link):
		data = json.dumps(link).replace("'", "'\\''")
		out = machine.succeed(f"curl -sk -X POST {auth} -H 'Content-Type: application/json' -d '{data}' -w '\\n%{{http_code}}' https://localhost:47823/api/v1/links")
		text, status = out.rsplit("\n", 1)
		return int(status), json.loads(text)
	assert post_link({"name": "Script", "url": "javascript:alert(1)"}) == (400, {"error": "bad_url"})
	evil = {"name": "Evil", "url": "http://{host}:9", "unit": "podman-kube@https:--example.org-x.service"}
	assert post_link(evil) == (403, {"error": "not_allowed"})
	syncthing = {"name": "Syncthing", "url": "http://{host}:8384", "unit": "syncthing.service"}
	assert post_link(syncthing) == (403, {"error": "not_allowed"})
	machine.succeed("""su - alice -c 'echo "units = [\\"syncthing.service\\"]" > ~/.config/elamon/elamond.d/units.toml'""")
	machine.wait_until_succeeds(f"curl -sk -X POST {auth} -H 'Content-Type: application/json' -d '{json.dumps(syncthing)}' https://localhost:47823/api/v1/links | jq -e '.unit == \"syncthing.service\"'", timeout=30)
	links = json.loads(machine.succeed(f"curl -sk {auth} https://localhost:47823/api/v1/links"))
	for link in links:
		machine.succeed(f"curl -sk -X DELETE {auth} https://localhost:47823/api/v1/links/{link['id']}")
	machine.succeed("su - alice -c 'rm ~/.config/elamon/elamond.d/units.toml'")

with subtest("a link in the settings shows in the list and in samples"):
	machine.succeed("""printf '%s\\n' '[[links]]' 'name = "Web"' 'url = "http://{host}:8080"' > /home/alice/.config/elamon/elamond.d/links.toml""")
	machine.succeed("chown alice: /home/alice/.config/elamon/elamond.d/links.toml")
	machine.wait_until_succeeds(f"""curl -sk {auth} https://localhost:47823/api/v1/links | jq -e '.[0].name == "Web"'""", timeout=30)
	machine.wait_until_succeeds(f"""curl -sk {auth} https://localhost:47823/api/v1/snapshot | jq -e '[.links[].name] == ["Web"]'""", timeout=30)

def call(method, path, auth="", body=None):
	"""The status and the JSON body of a request to the service."""
	data = f"-H 'Content-Type: application/json' -d '{json.dumps(body)}'" if body is not None else ""
	out = machine.succeed(f"curl -sk -X {method} {auth} {data} -w '\\n%{{http_code}}' https://localhost:47823{path}")
	text, status = out.rsplit("\n", 1)
	return int(status), json.loads(text) if text.strip() else None

def pair_code(unit, *args):
	"""Runs `elamond pair --yes` as alice in a unit of its own and returns the code it prints."""
	machine.succeed(f"systemd-run --uid=alice -p Environment=HOME=/home/alice --unit={unit} /run/current-system/sw/bin/timeout 120 /run/current-system/sw/bin/elamond pair --yes {' '.join(args)}")
	pattern = "'^ +[0-9]{3} [0-9]{3} *$'"
	machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -Eq {pattern}", timeout=30)
	return machine.succeed(f"journalctl -u {unit} -o cat | grep -E {pattern} | head -1 | tr -d ' \\n'")

# An action no computer has: let through, it fails as unknown, and nothing is done.
nothing = {"action": "nothing"}
unknown = (502, {"error": "failed", "message": "unknown_action"})

with subtest("elamond pair asks before a phone is let in: no turns it away, yes lets it in, and nothing to ask on stops it"):
	# Under systemd-run its input is /dev/null: nobody can answer, so it doesn't pair at all.
	machine.succeed("systemd-run --uid=alice -p Environment=HOME=/home/alice --unit=pair-unasked /run/current-system/sw/bin/elamond pair")
	machine.wait_until_succeeds("journalctl -u pair-unasked -o cat | grep -q 'elamond pair --yes'", timeout=30)
	machine.fail("journalctl -u pair-unasked -o cat | grep -Eq '^ +[0-9]{3} [0-9]{3} *$'")
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
		machine.succeed(f"systemd-run --uid=alice -p Environment=HOME=/home/alice -p Environment=PATH=/run/current-system/sw/bin --unit=pair-ask-{answer} /tmp/answer.sh {answer} {out}")
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
			machine.succeed(f"journalctl _UID=1000 -o cat | grep -q 'pairing: \"{device}\" from .* not added: turned away at elamond pair'")
		else:
			assert status == 200 and body["role"] == "admin", (status, body)
			assert device in clients, clients
			machine.wait_until_succeeds(f"grep -q 'Paired with \"{device}\"' {out}", timeout=30)
	# The code a phone was turned away with doesn't work again.
	assert call("POST", "/api/v1/pair", body={"code": codes["n"], "device": "Again"}) == (403, {"error": "no_pairing"})

with subtest("a phone paired as glance sees the basics and may do nothing"):
	code = pair_code("pair-glance", "--role", "glance")
	machine.succeed("journalctl -u pair-glance -o cat | grep -q 'Role: *glance'")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Glance phone"})
	assert status == 200 and paired["role"] == "glance", paired
	glance, glance_id = f"-H 'Authorization: Bearer {paired['token']}'", paired["client_id"]
	_, sample = call("GET", "/api/v1/snapshot?procs=5&media=1", glance)
	assert not {"processes", "process_count", "media"} & sample.keys(), sample.keys()
	assert sample["displays"] == sample["links"] == [], sample
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
	machine.succeed("su - alice -c 'elamond control status' | grep 'held off'")
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
	second("pair", "--yes", unit="second-pair")
	machine.wait_until_succeeds("journalctl -u second-pair -o cat | grep -q 'Addresses:'", timeout=30)
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
		machine.wait_until_succeeds(f"journalctl -u {unit} -o cat | grep -q 'Waiting for the phone'", timeout=30)
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
	assert call("GET", "/api/v1/info", kept) == (401, {"error": "unauthorized"})
	machine.succeed("systemctl --user -M alice@ stop elamond")
	# A wrong passphrase changes nothing.
	before = machine.succeed("ls /home/alice/.local/share/elamon")
	out = machine.fail("su - alice -c 'elamond restore ~/b.elamond --passphrase-file ~/wrong' 2>&1")
	assert "passphrase is wrong" in out, out
	assert machine.succeed("ls /home/alice/.local/share/elamon") == before
	# Restored, settings too: the same certificate, and the phones' tokens work again.
	out = machine.succeed("su - alice -c 'elamond restore ~/b.elamond --passphrase-file ~/pass --with-settings'")
	assert "Backup phone" in out and "Test phone" in out and "cert.der.before-restore-" in out, out
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
	assert call("GET", "/api/v1/info", kept)[0] == 200
	assert call("GET", "/api/v1/info", auth)[0] == 200
	# With the rotation phones follow, and the drop-in that turned control on.
	assert len(call("GET", "/api/v1/identity/rotations")[1]) == 1
	assert call("GET", "/api/v1/control", kept)[1]["enabled"]

with subtest("a phone that came by another's token gets a key of its own, and the old phone is revoked alone"):
	code = pair_code("pair-move")
	status, paired = call("POST", "/api/v1/pair", body={"code": code, "device": "Old phone"})
	assert status == 200 and paired["role"] == "admin", paired
	old_id = paired["client_id"]
	# Import pairings on a new phone: the old phone's token, from another client.
	shared = f"-H 'Authorization: Bearer {paired['token']}'"
	assert call("GET", "/api/v1/info", shared)[0] == 200
	status, own = call("POST", "/api/v1/rekey", shared, {"device": "New phone"})
	assert status == 200 and own["role"] == "admin" and own["client_id"] != old_id and own["token"] != paired["token"], own
	mine = f"-H 'Authorization: Bearer {own['token']}'"
	# One key of its own per token: not again, and not from the key that came of it.
	assert call("POST", "/api/v1/rekey", shared, {"device": "Third phone"}) == (403, {"error": "rekey_refused"})
	assert call("POST", "/api/v1/rekey", mine, {"device": "Third phone"}) == (403, {"error": "rekey_refused"})
	clients = machine.succeed("su - alice -c 'elamond clients'")
	assert re.search(rf"^{old_id}\s+admin\s+Old phone\s+\(paired ", clients, re.M), clients
	assert re.search(rf"^{own['client_id']}\s+admin\s+New phone\s+\(added \S+ with the key of {old_id}\)", clients, re.M), clients
	# The old token works until it is revoked; then only the new one does.
	assert call("GET", "/api/v1/info", shared)[0] == 200
	out = machine.succeed(f"su - alice -c 'elamond revoke {old_id}'")
	assert "Unpaired 1 phone(s): Old phone" in out and "STILL PAIRED" in out and own["client_id"] in out, out
	assert call("GET", "/api/v1/info", shared) == (401, {"error": "unauthorized"})
	assert call("GET", "/api/v1/info", mine)[0] == 200
	assert call("GET", "/api/v1/control", mine)[1]["role"] == "admin"
	_, log = call("GET", "/api/v1/control/log", mine)
	assert any(e["client_id"] == old_id and e["what"] == "phone added: New phone" and e["result"] == "ok" for e in log), log
	machine.succeed("journalctl _UID=1000 -o cat | grep -q 'pairing: added \"New phone\" .* asked with the key of \"Old phone\"'")

with subtest("elamond rotate-cert --no-statement: a new certificate that nothing leads to, for a key that may have leaked"):
	before = machine.succeed(served_fp).strip()
	out = machine.succeed("su - alice -c 'elamond rotate-cert --no-statement'")
	assert "elamond revoke all" in out and "nothing signed with it" in out, out
	machine.wait_until_succeeds(f"test \"$({served_fp})\" != {before}", timeout=30)
	assert call("GET", "/api/v1/identity/rotations") == (200, [])
	assert call("GET", "/api/v1/info", mine)[0] == 200
