#!/usr/bin/env bash
# End-to-end run on an emulator that adb sees: builds nothing, starts a throwaway elamond from
# elamond/target/release on 127.0.0.1 (the emulator reaches it as 10.0.2.2), installs the debug APK
# (io.github.elaizes.elamon.debug, beside the real app and never in its place), pairs, walks every
# screen and the settings' own dashboard with an action's choice, makes the phone a glance one and an
# admin again with elamond clients role, switches its profile and back, changes the phone's own
# profile in elamond.d/phones.toml with its screen open, pairs it again (it keeps that profile), and
# forgets the computer in the app (the computer unpairs it, keeping its table) and pairs once more,
# given that table by name, and leaves the screenshots in $OUT. The test's own settings are a
# drop-in, elamond.d/test.toml, over the
# starter elamond.toml and elamond.d/dashboards.toml the service writes at its first start (read
# after them, its name coming after dashboards.toml).
#   cargo build --release (in elamond/) and ./gradlew :app:assembleDebug first.
#   ELAMOND=path/to/elamond   another build of the service than elamond/target/release/elamond.
#   FONT_SCALE=2.0 maestro/run.sh   repeats the screens with a large font; the phone's own comes back after.
#   maestro/run.sh --yes-real-phone  runs on a phone that isn't an emulator: it wipes the debug app's
#                                    data there and changes the font scale while it runs.
# With more than one device attached, ANDROID_SERIAL says which; adb and maestro both use it.
set -euo pipefail
cd "$(dirname "$0")/.."
APP_ID=io.github.elaizes.elamon.debug
ELAMOND=$(realpath "${ELAMOND:-elamond/target/release/elamond}")
PORT=${PORT:-47911}
OUT=${OUT:-$(mktemp -d)}
FONT_SCALE=${FONT_SCALE:-1.0}
NAME="Test computer"

real_phone=false
for arg in "$@"; do
	case "$arg" in
		--yes-real-phone) real_phone=true ;;
		*) echo "unknown argument: $arg" >&2; exit 2 ;;
	esac
done

# One device, named, so that adb and maestro talk to the one checked here.
if [[ -z "${ANDROID_SERIAL:-}" ]]; then
	mapfile -t devices < <(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }')
	if [[ ${#devices[@]} -ne 1 ]]; then
		echo "${#devices[@]} devices attached; set ANDROID_SERIAL to the one to use" >&2
		exit 2
	fi
	ANDROID_SERIAL=${devices[0]}
fi
export ANDROID_SERIAL
# Emulators say so in ro.kernel.qemu, newer system images in ro.boot.qemu.
qemu=$(adb shell getprop ro.kernel.qemu | tr -d '\r')
[[ "$qemu" == 1 ]] || qemu=$(adb shell getprop ro.boot.qemu | tr -d '\r')
if [[ "$qemu" != 1 && "$real_phone" != true ]]; then
	echo "$ANDROID_SERIAL is not an emulator; run with --yes-real-phone to use it anyway" >&2
	exit 2
fi

# "null" when the phone never had it set: then it is deleted again rather than set.
font_scale_before=$(adb shell settings get system font_scale | tr -d '\r')
restore_font_scale() {
	if [[ -z "$font_scale_before" || "$font_scale_before" == null ]]; then
		adb shell settings delete system font_scale >/dev/null 2>&1 || true
	else
		adb shell settings put system font_scale "$font_scale_before" >/dev/null 2>&1 || true
	fi
}

work=$(mktemp -d)
trap 'kill "${service:-}" "${pairing:-}" "${upsd:-}" "${writer:-}" 2>/dev/null; restore_font_scale; rm -rf "$work"' EXIT

mkdir -p "$work/cfg/elamond.d" "$work/data"
# A simulated upsd (upst-core's demo, UPS "rack") on a loopback port of its own, for the UPS tab.
DEMO_UPSD=${DEMO_UPSD:-elamond/target/debug/examples/demo_upsd}
[[ -x "$DEMO_UPSD" ]] || (cd elamond && cargo build --example demo_upsd --locked)
"$DEMO_UPSD" > "$work/upsd.addr" 2> "$OUT/upsd.log" &
upsd=$!
for _ in $(seq 20); do
	grep -q ':[0-9][0-9]*$' "$work/upsd.addr" && break
	sleep 0.5
done
upsd_port=$(sed -n 's/^.*:\([0-9][0-9]*\)$/\1/p' "$work/upsd.addr" | head -n 1)
[[ -n "$upsd_port" ]] || { echo "maestro/run.sh: the simulated upsd printed no address" >&2; exit 1; }
# Any password does for the demo; the service reads it from a file, as it does anywhere.
echo "demo" > "$work/nut.password"
chmod 600 "$work/nut.password"
# The test's own settings, a drop-in: the service writes the starter files beside it.
cat > "$work/cfg/elamond.d/test.toml" <<TOML
port = $PORT
listen = "127.0.0.1"
mdns = false
name = "$NAME"
control = true

[actions.hello]
name = "Say hello"
command = ["echo", "hello from the test service"]
confirm = false

[actions.mode]
name = "Mode"
command = ["echo", "mode", "{mode}"]
params.mode = { choices = ["quiet", "fast"], labels = { quiet = "Quiet", fast = "Fast" }, default = "quiet" }

[links.web]
name = "Local web"
url = "http://{host}:$((PORT + 1))"

# A dashboard of the settings' own: a choice for the action above and a reading.
[dashboards.test]
title = "Test"
icon = "rocket"

[[dashboards.test.widgets]]
id = "mode"
type = "select"
action = "mode"
param = "mode"
label = "Mode"
icon = "leaf"
span = 4

[[dashboards.test.widgets]]
id = "load"
type = "value"
value = "cpu.usage"
label = "Test load"
icon = "chip"
spark = true

# The UPS below as a timeline whose legend tells shares too: "On battery · 100% · 14 min".
[[dashboards.test.widgets]]
id = "power"
type = "timeline"
source = "rack"
label = "Rack power"
legend = "both"

# The profile the phone pairs with, and a leaner one elamond clients profile switches it to.
[profiles.phone]
name = "Phone"
theme = "dark"
dashboards = ["overview", "processes", "sensors", "control", "ups", "test", "sources"]

# A UPS on the simulated upsd: its tab, the beeper's switch and the low-battery level it may set.
[sources.rack]
kind = "nut"
ups = "rack"
host = "127.0.0.1"
port = $upsd_port
user = "elamon"
password_file = "$work/nut.password"
commands = ["beeper.*"]
settings = ["battery.charge.low"]

# Sources of the settings' own: a file a loop below rewrites, a command's JSON, and this user's
# journal, which the same loop writes to.
[sources.counter]
kind = "file"
path = "$work/counter"
format = "text"
interval = 1

[sources.greet]
kind = "command"
command = ["printf", '{"word": "hello", "n": 7}']
interval = 5

[dashboards.sources]
title = "Sources"
icon = "terminal"

[[dashboards.sources.widgets]]
id = "counter"
type = "value"
value = "counter.text"
label = "Counter"
format = "integer"
span = 2

[[dashboards.sources.widgets]]
id = "greet"
type = "status"
value = "greet.word"
label = "Greeting"
span = 2

[[dashboards.sources.widgets]]
id = "journal"
type = "log"
scope = "user"
label = "My journal"
span = 4

[profiles.lean]
use = "phone"
name = "Lean"
dashboards = ["overview", "test"]
TOML
# Always started in the background: exec makes $! elamond itself, so the trap can stop it. No
# system bus: power, units and the rest of this computer stay out of the test's reach.
run_elamond() {
	exec env -i PATH="$PATH" HOME="$work" ELAMON_CONFIG_DIR="$work/cfg" ELAMON_DATA_DIR="$work/data" \
		DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent \
		${ELAMON_PCI_IDS:+ELAMON_PCI_IDS="$ELAMON_PCI_IDS"} \
		"$ELAMOND" "$@"
}
# What the Sources tab's file holds until a loop below takes it up.
echo 100 > "$work/counter"
run_elamond serve > "$OUT/elamond.log" 2>&1 &
service=$!
sleep 2
fail() { echo "maestro/run.sh: $*" >&2; exit 1; }
# The starter and the defaults are written beside the drop-in.
grep -qx 'schema = 3' "$work/cfg/elamond.toml" && [[ -f "$work/cfg/elamond.d/dashboards.toml" && -f "$work/cfg/defaults.toml" && -f "$work/cfg/elamond-schema.json" ]] || fail "elamond.toml, dashboards.toml, defaults.toml or elamond-schema.json wasn't written"

adb uninstall "$APP_ID" > /dev/null 2>&1 || true
adb install -r "${APK:-app/build/outputs/apk/debug/app-debug.apk}" > /dev/null
adb shell settings put system font_scale "$FONT_SCALE"

# --yes: the throwaway service allows the phone without asking at the computer.
run_elamond pair --yes --profile phone > "$work/pair.log" 2>&1 &
pairing=$!
sleep 3
code=$(grep -a -E '^\s+[0-9]{3} [0-9]{3}\s*$' "$work/pair.log" | tr -d ' ')
flow() { maestro --device "$ANDROID_SERIAL" test --test-output-dir "$OUT" "$@"; }
flow -e ADDRESS="10.0.2.2:$PORT" -e CODE="$code" maestro/pair.yaml
# The phone has a profile of its own in phones.toml, using the one --profile named.
phones=$work/cfg/elamond.d/phones.toml
grep -qx 'use = "phone"' "$phones" || fail "no profile of the phone's own in $phones"
own=$(sed -n 's/^\[profiles\.\(.*\)\]$/\1/p' "$phones" | tail -n 1)
flow -e NAME="$NAME" maestro/screens.yaml
# In a subshell: run_elamond execs. A glance phone sees the basics only.
(run_elamond clients role all glance)
flow -e NAME="$NAME" maestro/glance.yaml
# Admin again while the computer's screen stays open: the app notices without being reopened.
(run_elamond clients role all admin)
flow maestro/admin-again.yaml
# Another profile, and back: the phone's tabs follow with the screen open.
(run_elamond clients profile all lean)
flow maestro/profile.yaml
(run_elamond clients profile all phone)
flow maestro/profile-back.yaml
# The UPS tab of the simulated upsd: the status, the power, a long range, the beeper switched off
# (the station's own value says so afterwards). Here, as the phone has every tab again.
flow -e NAME="$NAME" maestro/ups.yaml
(run_elamond values rack) | grep -E 'ups\.beeper\.status' | grep -qi disabled || fail "the beeper wasn't switched off on the simulated upsd: $( (run_elamond values rack) )"
# The settings' own sources and the journal, live; a level chip leaves only the warnings. Meanwhile
# the counter goes up every 2 s from 100, each step a line in this user's journal, every fifth a
# warning.
(
	for i in $(seq 101 400); do
		sleep 2
		echo "$i" > "$work/counter"
		if (( i % 5 == 0 )); then
			logger -p user.warning -t elamon-maestro "maestro warning $i"
		else
			logger -t elamon-maestro "maestro line $i"
		fi
	done
) &
writer=$!
flow maestro/sources.yaml
kill "$writer" 2>/dev/null || true
# What is the phone's own: the app starts on the computer opened last, Back to the list; the scale
# and the app's own theme kept across a restart; the Test tab's timeline tells shares of the time.
flow -e NAME="$NAME" maestro/look.yaml
(run_elamond values greet) | grep -q 'greet\.word.*hello' || fail "elamond values greet doesn't read the command: $( (run_elamond values greet) )"
# The flow leaves the tab: the service's journalctl ends with the last stream that showed it.
for _ in $(seq 20); do pgrep -P "$service" -x journalctl > /dev/null || break; sleep 0.5; done
! pgrep -P "$service" -x journalctl > /dev/null || fail "the service's journalctl still runs with nobody looking at the journal"
# Its own profile again, then two keys added to its table with the screen open: the last table in
# the file is the phone's.
(run_elamond clients profile all "$own")
printf 'theme = "light"\ndashboards = ["overview", "test"]\n' >> "$phones"
flow maestro/own-profile.yaml
# Paired again by code: the app proves it is the same phone, so it keeps that table, and the entry
# before goes.
run_elamond pair --yes > "$work/pair2.log" 2>&1 &
pairing=$!
sleep 3
code=$(grep -a -E '^\s+[0-9]{3} [0-9]{3}\s*$' "$work/pair2.log" | tr -d ' ')
flow -e ADDRESS="10.0.2.2:$PORT" -e CODE="$code" maestro/pair-again.yaml
wait "$pairing" || true
grep -q "keeps its profile $own\\|It replaces" "$work/pair2.log" || fail "pairing again didn't replace the entry before: $(cat "$work/pair2.log")"
[[ $( (run_elamond clients) | grep -c '^[0-9a-f]\{8\} ') -eq 1 ]] || fail "more than one entry after pairing again"
[[ $(grep -c '^\[profiles\.' "$phones") -eq 1 ]] || fail "phones.toml has more than the phone's one table"
# Forget computer: the computer unpairs the phone too, and keeps its table, noted, for it to pair
# again. Paired again by code, without --yes: the person lets it in and gives it that table by name.
flow -e NAME="$NAME" maestro/forget.yaml
(run_elamond clients) | grep -q '^No phones are paired' || fail "the phone is still paired after Forget computer: $( (run_elamond clients) )"
if ! { grep -q '^# Unpaired ' "$phones" && grep -qx "\[profiles.$own\]" "$phones"; }; then fail "its table isn't kept with a note: $(cat "$phones")"; fi
(run_elamond check) | grep -q "\[profiles.$own\] is no paired phone's" || fail "elamond check doesn't name the table no phone has"
mkfifo "$work/answers"
run_elamond pair < "$work/answers" > "$work/pair3.log" 2>&1 &
pairing=$!
exec 3> "$work/answers"
# As the person at the computer: let the phone in, and give it the table of its name.
(
	for asked in 'Allow? \[y/N\]' 'Give it to this one? \[y/N\]'; do
		until grep -q "$asked" "$work/pair3.log"; do sleep 0.5; done
		echo y >&3
	done
) &
answering=$!
sleep 3
code=$(grep -a -E '^\s+[0-9]{3} [0-9]{3}\s*$' "$work/pair3.log" | tr -d ' ')
flow -e ADDRESS="10.0.2.2:$PORT" -e CODE="$code" maestro/pair-again.yaml
wait "$pairing" || true
kill "$answering" 2>/dev/null || true
exec 3>&-
grep -q "had the profile $own" "$work/pair3.log" || fail "pairing wasn't offered the table by name: $(cat "$work/pair3.log")"
[[ $( (run_elamond clients) | grep -c '^[0-9a-f]\{8\} ') -eq 1 ]] || fail "not one entry after pairing again"
if [[ $(grep -c '^\[profiles\.' "$phones") -ne 1 ]] || grep -q '^# Unpaired ' "$phones"; then fail "the table wasn't taken again: $(cat "$phones")"; fi
echo "screenshots: $OUT"
