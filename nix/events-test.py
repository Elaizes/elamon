import base64
import json
import re
import shlex

# The service's events, each step asserting on three things: what `elamond events` shows and
# events.jsonl keeps, what reached the mock ntfy (one request per target and event), and what the
# service's journal says. Two targets send to the mock: `mock` as a target starts (names = false,
# level warning), `named` with names and a token. Both post to the server's root, as ntfy takes
# JSON; their topics tell them apart.

machine.wait_for_unit("multi-user.target")
machine.wait_for_unit("ntfy-mock.service")
machine.wait_for_open_port(8090)
machine.wait_for_unit("upsd.service")

# alice's commands run in a login shell, which has the module's settings in ELAMON_CONFIG.
HOME = "/home/alice"
CONF = f"{HOME}/.config/elamon"
DATA = f"{HOME}/.local/share/elamon"
DROPIN = f"{CONF}/elamond.d/zz-events-test.toml"
MOCK_LOG = "/var/lib/ntfy-mock/requests.jsonl"
UPS_FILE = "/var/lib/cellar-ups/cellarups.dev"
# The computer's name, the scratch disk's key and mount point, the link and its unit, the UPS
# source: what a target without names never sends.
HOST = "evbench"
DISK = "srv_scratch"
NAMES = [HOST, "scratch", "srv/", "ledger", "cellar", "bench 1500"]

# The topics and the token, made at boot: looked for below everywhere the service writes or says.
TOPIC = machine.succeed(f"cat {CONF}/ntfy.topic").strip()
NAMED_TOPIC = machine.succeed(f"cat {CONF}/ntfy-named.topic").strip()
TOKEN = machine.succeed(f"cat {CONF}/ntfy.token").strip()
assert TOPIC.startswith("topic-marker-") and len(TOPIC) == 29, TOPIC
assert NAMED_TOPIC.startswith("topic-named-") and len(NAMED_TOPIC) == 28, NAMED_TOPIC
assert TOKEN.startswith("tk_") and len(TOKEN) == 27, TOKEN
SECRETS = {"topic": TOPIC, "named topic": NAMED_TOPIC, "token": TOKEN}
TARGET_OF = {TOPIC: "mock", NAMED_TOPIC: "named"}
# Everything the service's commands printed, looked through for the secrets at the end.
seen = []


def as_alice(command, check=True):
	"""What `command` prints (stderr too) run as alice in a login shell; with `check`, it must
	succeed; without, (exit status, output)."""
	rc, out = machine.execute(f"su - alice -c {shlex.quote(command)} 2>&1")
	seen.append(out)
	if check:
		assert rc == 0, (command, rc, out)
		return out
	return rc, out


def now_ms():
	return int(machine.succeed("date +%s%3N").strip())


def kept():
	"""The events events.jsonl keeps, oldest first, each with how sending it went by target."""
	out = machine.succeed(f"cat {DATA}/events.jsonl 2>/dev/null || true")
	seen.append(out)
	events: dict = {}
	for line in out.splitlines():
		k = json.loads(line)
		if "n" in k:
			events[k["n"]] = dict(k, sent={})
		else:
			events[k["sent"]]["sent"][k["target"]] = k["result"]
	return list(events.values())


def requests():
	"""What the mock ntfy got, oldest first: each request with its body as JSON and its target."""
	out = machine.succeed(f"cat {MOCK_LOG} 2>/dev/null || true")
	reqs = []
	for line in out.splitlines():
		r = json.loads(line)
		r["json"] = json.loads(r["body"])
		r["target"] = TARGET_OF.get(r["json"].get("topic"))
		assert r["target"], r
		reqs.append(r)
	return reqs


def journal():
	"""The service's journal, as root reads it."""
	out = machine.succeed("journalctl _SYSTEMD_USER_UNIT=elamond.service -o cat --no-pager")
	seen.append(out)
	return out


def journal_events():
	"""The events the service's journal tells of, as (level, code, text)."""
	return re.findall(r"^event \d+: (\w+) (\S+): (.*)$", journal(), re.M)


EVENT_LINE = re.compile(r"^\d{4}-\d\d-\d\d \d\d:\d\d  (\w+) +(\S+)  (\S+?)( \(end\))?: (.*)$")


def listed():
	"""The events as `elamond events` lists them, oldest first."""
	out = as_alice("elamond events")
	events: list[dict] = []
	for line in out.splitlines():
		m = EVENT_LINE.match(line)
		if m:
			events.append({"level": m[1], "rule": m[2], "code": m[3], "end": bool(m[4]), "text": m[5], "sent": {}})
		elif line.startswith("    ") and events:
			target, result = line.strip().split(": ", 1)
			events[-1]["sent"][target] = result
	assert events or "No events are kept." in out, out
	return events


def mark():
	"""Where the events and the requests are now, for a step to count what it made."""
	return len(kept()), len(requests())


def since(at):
	"""The events and the requests made since `at`."""
	return kept()[at[0]:], requests()[at[1]:]


def group(pattern, text):
	"""The first group of `pattern`, which the whole of `text` must match."""
	m = re.fullmatch(pattern, text)
	assert m, (pattern, text)
	return m[1]


def wait_for(what, check, timeout=60):
	"""`check()`'s first true answer, asked every second for `timeout` seconds."""
	for _ in range(timeout):
		got = check()
		if got:
			return got
		machine.sleep(1)
	raise AssertionError(f"{what}: not within {timeout} s; events {kept()}; requests {requests()}")


def wait_event(at, rule, phase, count=1, sent=("mock", "named")):
	"""The `count` events of `rule` in `phase` since `at`, once there are as many and each has gone
	to every target of `sent` (or been told why not)."""
	def ready():
		events = [e for e in since(at)[0] if e["rule"] == rule and e["phase"] == phase]
		return events if len(events) >= count and all(set(sent) <= set(e["sent"]) for e in events) else None
	return wait_for(f"{count} {phase} event(s) of {rule}", ready)


def one_each(reqs):
	"""Requests that are one for each target, by target."""
	by = {r["target"]: r for r in reqs}
	assert len(reqs) == 2 and set(by) == {"mock", "named"}, reqs
	return by


def without_names(r):
	"""A request of the target without names: no name of the computer's in it, and no token."""
	assert r["target"] == "mock", r
	b = r["json"]
	shown = json.dumps({k: v for k, v in b.items() if k != "topic"}).lower()
	assert not any(n.lower() in shown for n in NAMES), b
	assert "Authorization" not in r["headers"], r["headers"]
	return b


def with_names(r):
	"""A request of the target with names, which comes with its token."""
	assert r["target"] == "named", r
	assert r["headers"].get("Authorization") == f"Bearer {TOKEN}", r["headers"]
	return r["json"]


def posted(r, title, message, priority, tags):
	b = r["json"]
	assert r["method"] == "POST" and r["path"] == "/", r
	assert r["headers"].get("Content-Type") == "application/json", r["headers"]
	want: dict = {"topic": {"mock": TOPIC, "named": NAMED_TOPIC}[r["target"]], "title": title, "message": message, "priority": priority}
	if tags:
		want["tags"] = tags
	assert b == want, (b, want)


def quiet(at, seconds, what):
	"""Nothing more made or sent for `seconds` after `at`."""
	machine.sleep(seconds)
	events, reqs = since(at)
	assert not events and not reqs, (what, events, reqs)


def disk_fill(percent=95):
	"""Fills the scratch disk to `percent`, as the service counts it: used of the whole."""
	size, avail = map(int, machine.succeed("df -B1 --output=size,avail /srv/scratch | tail -1").split())
	want = int(size * percent / 100) - (size - avail)
	assert want > 0, (size, avail)
	machine.succeed(f"fallocate -l {want} /srv/scratch/fill && sync")
	pct = int(machine.succeed("df --output=pcent /srv/scratch | tail -1").strip().rstrip("%"))
	assert pct >= 93, pct


def disk_free():
	machine.succeed("rm -f /srv/scratch/fill && sync")
	pct = int(machine.succeed("df --output=pcent /srv/scratch | tail -1").strip().rstrip("%"))
	assert pct < 50, pct


def write_dropin(text):
	"""Alice's drop-in, written whole, and the service made to read it: the settings reread."""
	before = journal().count("settings: reread")
	machine.succeed(f"install -d -o alice -g users -m 700 {CONF}/elamond.d")
	if text is None:
		machine.succeed(f"rm -f {DROPIN}")
	else:
		machine.succeed(f"printf %s {shlex.quote(text)} > {DROPIN}.new && chown alice:users {DROPIN}.new && chmod 600 {DROPIN}.new && mv {DROPIN}.new {DROPIN}")
	machine.succeed("systemctl --user -M alice@ reload elamond")
	wait_for("the settings reread", lambda: journal().count("settings: reread") > before, timeout=20)


def ups(status, charge=100):
	"""dummy-ups's UPS with `status`, once upsd says so."""
	lines = ["device.mfr: Example", "device.model: Bench 1500", "ups.mfr: Example", "ups.model: Bench 1500", f"ups.status: {status}", f"battery.charge: {charge}", "battery.runtime: 3600", "ups.load: 20"]
	machine.succeed(f"printf '%s\\n' {' '.join(shlex.quote(l) for l in lines)} > {UPS_FILE}")
	machine.wait_until_succeeds(f"upsc cellarups@127.0.0.1 ups.status 2>/dev/null | grep -qx '{status}'", timeout=30)


def service_up():
	machine.wait_until_succeeds("ss -ltnpe | grep :47823 | grep -q uid:1000", timeout=60)


def state():
	"""The rules' states as events-state.json keeps them."""
	out = machine.succeed(f"cat {DATA}/events-state.json")
	seen.append(out)
	return json.loads(out)["states"]


with subtest("the service starts with the targets, nothing made or sent; elamond check names them on its Events line"):
	service_up()
	machine.wait_until_succeeds("upsc cellarups@127.0.0.1 ups.status 2>/dev/null | grep -qx OL", timeout=60)
	# The scratch disk is one of the service's disks (tmpfs it leaves out).
	values = as_alice("elamond values")
	assert re.search(rf"^disk\.{DISK}\.percent\s", values, re.M), values
	check = as_alice("elamond check")
	print(check)
	# The standard rules that are on, and boot, which the settings turn on.
	assert [l for l in check.splitlines() if l.startswith("Events:")] == ["Events: 8 rules on; sent on to mock, named."], check
	# Their topic and token files are alice's alone: no word of them.
	assert "topic_file" not in check and "token_file" not in check, check
	machine.sleep(5)
	assert kept() == [] and requests() == [], (kept(), requests())
	assert listed() == []

with subtest("the disk rule: past 90% for its 3 s, one event and one request per target, the one without names naming nothing; the end, one more"):
	at = mark()
	filled = now_ms()
	disk_fill()
	[start] = wait_event(at, "disk", "start")
	# Not before the trouble had lasted `for`.
	assert start["t"] - filled >= 3000, (start, filled)
	pct = group(r"A disk is (\d+%) full", start["plain"])
	assert int(pct[:-1]) >= 93, start
	assert (start["level"], start["of"], start["code"], start["text"]) == ("warning", "warning", "disk.full", f"Disk {DISK} is {pct} full"), start
	assert start["sent"] == {"mock": "sent", "named": "sent"}, start
	by = one_each(since(at)[1])
	without_names(by["mock"])
	posted(by["mock"], "Elamon", f"A disk is {pct} full", 4, ["warning"])
	with_names(by["named"])
	posted(by["named"], HOST, f"Disk {DISK} is {pct} full", 4, ["warning"])
	last = listed()[-1]
	assert last == {"level": "warning", "rule": "disk", "code": "disk.full", "end": False, "text": f"Disk {DISK} is {pct} full", "sent": {"mock": "sent", "named": "sent"}}, last
	assert journal_events().count(("warning", "disk.full", f"Disk {DISK} is {pct} full")) == 1
	# Several ticks and readings of the disk more, still full: nothing more.
	quiet(mark(), 12, "the disk still full")
	# Below 88%, calm again: the end, sent with the priority and tags of `recovered`.
	at = mark()
	disk_free()
	[end] = wait_event(at, "disk", "end")
	assert (end["level"], end["of"], end["code"], end["plain"]) == ("info", "warning", "disk.full", "The disk has room again"), end
	assert re.fullmatch(rf"Disk {DISK} has room again: \d+% full", end["text"]), end
	by = one_each(since(at)[1])
	without_names(by["mock"])
	posted(by["mock"], "Elamon", "The disk has room again", 2, ["white_check_mark"])
	posted(by["named"], HOST, end["text"], 2, ["white_check_mark"])
	last = listed()[-1]
	assert (last["code"], last["end"], last["text"], last["sent"]) == ("disk.full", True, end["text"], {"mock": "sent", "named": "sent"}), last
	quiet(mark(), 8, "the disk calm")
	# Every disk event is of the scratch disk: no other filesystem of the VM is near full.
	assert all(DISK in e["text"] for e in kept() if e["rule"] == "disk"), kept()

with subtest("a restart in the middle of a trouble tells nothing twice, and its end comes after it"):
	at = mark()
	disk_fill()
	wait_event(at, "disk", "start")
	one_each(since(at)[1])
	assert state()[f"disk/{DISK}"]["fired"] is True, state()
	at = mark()
	machine.succeed("systemctl --user -M alice@ restart elamond")
	service_up()
	# Readings of the disk, still full, by the service started again: no second start, no request.
	quiet(at, 15, "restarted with the disk full")
	starts = [e for e in journal_events() if e[1] == "disk.full" and e[2].startswith(f"Disk {DISK} is ")]
	assert len(starts) == 2, journal_events()
	at = mark()
	disk_free()
	[end] = wait_event(at, "disk", "end")
	assert end["plain"] == "The disk has room again", end
	one_each(since(at)[1])
	quiet(mark(), 8, "the disk calm after the restart")

with subtest("a link's unit that fails: one event, no unit or link named without names; its end once reset"):
	at = mark()
	machine.succeed("systemctl start ledger-sync.service || true")
	machine.wait_until_succeeds("systemctl is-failed ledger-sync.service", timeout=15)
	failed = now_ms()
	# Links' units are read every `background` (5 s) while no phone looks.
	[start] = wait_event(at, "unit", "start")
	assert start["t"] - failed <= 12000, (start, failed)
	assert (start["level"], start["code"], start["text"], start["plain"]) == ("warning", "unit.failed", "The unit of ledger failed", "A unit failed"), start
	by = one_each(since(at)[1])
	without_names(by["mock"])
	posted(by["mock"], "Elamon", "A unit failed", 4, ["warning"])
	with_names(by["named"])
	posted(by["named"], HOST, "The unit of ledger failed", 4, ["warning"])
	assert journal_events().count(("warning", "unit.failed", "The unit of ledger failed")) == 1
	last = listed()[-1]
	assert (last["rule"], last["code"], last["text"], last["sent"]) == ("unit", "unit.failed", "The unit of ledger failed", {"mock": "sent", "named": "sent"}), last
	quiet(mark(), 11, "the unit still failed")
	at = mark()
	machine.succeed("systemctl reset-failed ledger-sync.service")
	[end] = wait_event(at, "unit", "end")
	assert (end["text"], end["plain"]) == ("The unit of ledger is inactive again", "The unit runs again"), end
	by = one_each(since(at)[1])
	posted(by["mock"], "Elamon", "The unit runs again", 2, ["white_check_mark"])

with subtest("elamond notify: a warning sent with its own title, as it is; info kept and not sent; too long refused; no service, no event"):
	at = mark()
	as_alice("elamond notify --level warning --title Backup 'The backup failed'")
	[e] = wait_event(at, "notify", "once")
	assert (e["level"], e["code"], e["title"], e["text"], e["plain"]) == ("warning", "notify", "Backup", "The backup failed", "The backup failed"), e
	assert e["sent"] == {"mock": "sent", "named": "sent"}, e
	by = one_each(since(at)[1])
	posted(by["mock"], "Backup", "The backup failed", 4, ["warning"])
	posted(by["named"], "Backup", "The backup failed", 4, ["warning"])
	assert journal_events().count(("warning", "notify", "The backup failed")) == 1
	# Quotes and a backslash reach ntfy as they were written: the body goes to curl on its stdin.
	at = mark()
	title, text = 'Night "job"', 'Copy of C:\\temp says "done", 100%'
	as_alice(f"elamond notify --level warning --title {shlex.quote(title)} -- {shlex.quote(text)}")
	[e] = wait_event(at, "notify", "once")
	assert (e["title"], e["text"]) == (title, text), e
	by = one_each(since(at)[1])
	posted(by["mock"], title, text, 4, ["warning"])
	posted(by["named"], title, text, 4, ["warning"])
	# Info: kept and listed, below the targets' level, so never sent.
	at = mark()
	as_alice("elamond notify 'just info'")
	[e] = wait_event(at, "notify", "once")
	assert (e["level"], e["text"], e["sent"]) == ("info", "just info", {"mock": "not sent: below warning", "named": "not sent: below warning"}), e
	last = listed()[-1]
	assert last == {"level": "info", "rule": "notify", "code": "notify", "end": False, "text": "just info", "sent": {"mock": "not sent: below warning", "named": "not sent: below warning"}}, last
	quiet(mark(), 4, "an info event")
	# [events.notify] length is 60: 61 characters are refused, 60 taken.
	rc, out = as_alice(f"elamond notify {'x' * 61}", check=False)
	assert rc != 0 and "61 characters, more than the 60" in out, (rc, out)
	at = mark()
	as_alice(f"elamond notify {'y' * 60}")
	[e] = wait_event(at, "notify", "once")
	assert e["text"] == "y" * 60, e
	assert not [e for e in kept() if e["text"].startswith("x")], kept()
	# Without the service, nothing takes it: refused, saying so, and nothing left for its start.
	at = mark()
	machine.succeed("systemctl --user -M alice@ stop elamond")
	rc, out = as_alice("elamond notify --level warning stopped", check=False)
	assert rc != 0 and "isn't running" in out, (rc, out)
	machine.succeed("systemctl --user -M alice@ start elamond")
	service_up()
	quiet(at, 5, "a notify refused while the service was stopped")

with subtest("while a request is made, neither topic nor the token is in the process list"):
	# The mock takes 6 s to answer: curl waits on it meanwhile.
	machine.succeed("echo 6 > /var/lib/ntfy-mock/slow")
	at = mark()
	as_alice("elamond notify --level warning 'A slow one'")
	wait_for("a request in the making", lambda: len(requests()) > at[1], timeout=30)
	ps = machine.succeed("ps -efww")
	print(ps)
	assert re.search(r"curl .*--url http://127\.0\.0\.1:8090", ps), ps
	for name, secret in SECRETS.items():
		assert secret not in ps, name
	pids = machine.succeed("pgrep -x curl").split()
	assert pids, ps
	for pid in pids:
		raw = machine.succeed(f"cat /proc/{pid}/cmdline /proc/{pid}/environ 2>/dev/null | tr '\\0' '\\n' || true")
		for name, secret in SECRETS.items():
			assert secret not in raw, (name, pid)
	machine.succeed("rm /var/lib/ntfy-mock/slow")
	wait_event(at, "notify", "once")
	one_each(since(at)[1])

with subtest("a UPS on battery: one event and one request per target; polls meanwhile nothing; the power back, one end with how long"):
	at = mark()
	ups("OB DISCHRG", 97)
	[start] = wait_event(at, "ups-battery", "start")
	assert (start["level"], start["code"], start["text"], start["plain"]) == ("warning", "power.lost", "cellar is on battery", "The UPS is on battery"), start
	by = one_each(since(at)[1])
	without_names(by["mock"])
	posted(by["mock"], "Elamon", "The UPS is on battery", 4, ["warning"])
	with_names(by["named"])
	posted(by["named"], HOST, "cellar is on battery", 4, ["warning"])
	j = journal()
	assert j.count("nut cellar: power.lost: ") == 1, j
	assert journal_events().count(("warning", "power.lost", "cellar is on battery")) == 1
	# The source is asked every second: ten polls on battery, nothing more.
	quiet(mark(), 10, "the UPS still on battery")
	at = mark()
	ups("OL")
	[end] = wait_event(at, "ups-battery", "end")
	assert (end["level"], end["of"], end["code"]) == ("info", "warning", "power.restored"), end
	assert re.fullmatch(r"cellar: the power is back after \d+ min", end["text"]), end
	assert re.fullmatch(r"The power is back after \d+ min", end["plain"]), end
	by = one_each(since(at)[1])
	without_names(by["mock"])
	posted(by["mock"], "Elamon", end["plain"], 2, ["white_check_mark"])
	posted(by["named"], HOST, end["text"], 2, ["white_check_mark"])
	last = listed()[-1]
	assert (last["rule"], last["code"], last["end"], last["text"]) == ("ups-battery", "power.restored", True, end["text"]), last
	assert journal().count("nut cellar: power.restored: ") == 1
	quiet(mark(), 6, "the UPS on line")
	# No other UPS rule spoke: the battery wasn't low, upsd always answered.
	assert not [e for e in kept() if e["rule"] in ("ups-low", "ups-link")], kept()

with subtest("no rule is built in: one turned off in a drop-in makes nothing; its text and level changed there are the next event's"):
	write_dropin("[rules.disk]\nenabled = false\n")
	check = as_alice("elamond check")
	assert "Events: 7 rules on; sent on to mock, named." in check, check
	# Turned off, it is forgotten.
	wait_for("the disk rule's states forgotten", lambda: not [k for k in state() if k.startswith("disk/")], timeout=10)
	at = mark()
	disk_fill()
	quiet(at, 15, "the disk full, its rule off")
	write_dropin("[rules.disk]\nlevel = \"critical\"\ntext = \"Scratch volume {name} is at {value}\"\nplain = \"A volume is at {value}\"\n")
	assert "Events: 8 rules on; sent on to mock, named." in as_alice("elamond check")
	[start] = wait_event(at, "disk", "start")
	pct = group(r"A volume is at (\d+%)", start["plain"])
	assert (start["level"], start["text"]) == ("critical", f"Scratch volume {DISK} is at {pct}"), start
	by = one_each(since(at)[1])
	without_names(by["mock"])
	posted(by["mock"], "Elamon", f"A volume is at {pct}", 5, ["rotating_light"])
	posted(by["named"], HOST, f"Scratch volume {DISK} is at {pct}", 5, ["rotating_light"])
	assert journal_events().count(("critical", "disk.full", f"Scratch volume {DISK} is at {pct}")) == 1
	assert listed()[-1]["level"] == "critical"
	at = mark()
	disk_free()
	[end] = wait_event(at, "disk", "end")
	# The end goes by the level of its start: sent, as `recovered`.
	assert (end["level"], end["of"]) == ("info", "critical"), end
	by = one_each(since(at)[1])
	posted(by["mock"], "Elamon", "The disk has room again", 2, ["white_check_mark"])
	write_dropin(None)
	assert "Events: 8 rules on; sent on to mock, named." in as_alice("elamond check")

with subtest("a topic file others may read: elamond check says so, and that target's events aren't sent"):
	machine.succeed(f"chmod 644 {CONF}/ntfy.topic")
	check = as_alice("elamond check")
	assert "notify.mock.topic_file" in check and "may be read by others" in check, check
	at = mark()
	as_alice("elamond notify --level warning 'Not to the open topic'")
	[e] = wait_event(at, "notify", "once")
	assert e["sent"] == {"mock": "failed: topic_file may be read by others: chmod 600 it", "named": "sent"}, e
	reqs = since(at)[1]
	assert [r["target"] for r in reqs] == ["named"], reqs
	assert "notify mock: not sent: topic_file may be read by others" in journal()
	machine.succeed(f"chmod 600 {CONF}/ntfy.topic")

with subtest("the topics and the token are nowhere but their files and the requests: not the journal, the service's files, its commands or a backup"):
	# The requests: each topic in its target's bodies only, the token in the named one's header only.
	for r in requests():
		topic = {"mock": TOPIC, "named": NAMED_TOPIC}[r["target"]]
		assert r["json"]["topic"] == topic, r
		other = {TOPIC, NAMED_TOPIC} - {topic}
		assert not any(s in r["body"] for s in other | {TOKEN}), r
		assert not any(s in r["path"] or s in json.dumps(r["headers"]) for s in (TOPIC, NAMED_TOPIC)), r
		if r["target"] == "mock":
			assert TOKEN not in json.dumps(r["headers"]), r
	# The service's journal, the system's, everything.
	for name, secret in SECRETS.items():
		assert machine.succeed(f"journalctl -o cat --no-pager | grep -cF {secret} || true").strip() == "0", name
	# Of alice's files, only the one each is in: events.jsonl and events-state.json among the rest.
	machine.succeed(f"test -s {DATA}/events.jsonl && test -s {DATA}/events-state.json")
	for name, secret in SECRETS.items():
		files = machine.succeed(f"grep -rlsF -D skip {secret} {HOME} /run/user/1000 || true").split()
		assert len(files) == 1 and files[0].startswith(f"{CONF}/ntfy"), (name, files)
	# The commands that read the settings or the events.
	for command in ["elamond events", "elamond check", "elamond values", "elamond values cellar", "elamond defaults events"]:
		as_alice(command, check=False)
	out = as_alice(f"elamond backup {HOME}/b.elamond")
	magic, header, payload = machine.succeed(f"cat {HOME}/b.elamond").split("\n", 2)
	assert magic == "elamond-backup", magic
	payload = json.loads(payload)
	for name, secret in SECRETS.items():
		assert secret not in header and secret not in json.dumps(payload["settings"]), name
		for file, b64 in payload["state"].items():
			assert secret.encode() not in base64.b64decode(b64), (name, file)
	for text in seen:
		for name, secret in SECRETS.items():
			assert secret not in text, (name, text)

with subtest("a restart is no boot: the boot rule, on, made nothing"):
	assert not [e for e in kept() if e["rule"] == "boot"], kept()
	assert not [e for e in journal_events() if e[1] == "system.boot"], journal_events()
