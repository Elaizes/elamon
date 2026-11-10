import json

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
	machine.succeed("systemctl --user -M alice@ reload elamond")
	machine.wait_until_succeeds("journalctl _UID=1000 -o cat | grep -c 'settings: reread' | grep -qv '^1$'", timeout=30)

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
