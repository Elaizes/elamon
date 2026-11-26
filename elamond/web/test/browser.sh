#!/usr/bin/env bash
# The page in a real browser: a headless Chromium driven by Playwright (browser.py) against a
# throwaway elamond on 127.0.0.1, with settings, data and a port of its own. Pairing by code, the
# dashboards live from the stream, the theme and the layout picked in the page, the frame (the side
# bar, its keys, a phone's width), an action with its question, the admin panel (clients, pairing
# from the page, the files editor, control) and the dashboard editor; see browser.py.
#
#   nix develop .#browser -c elamond/web/test/browser.sh
#
#   ELAMOND=path/to/elamond   another build of the service than the one `cargo build` makes here.
#   BROWSER_SHOTS=dir         keeps the screenshots there; else they go with the work folder.
#   browser.sh NAME…          runs only the scenarios named (pair runs first whatever is named).
#
# The service never sees this computer's Hyprland (a stand-in hyprctl that knows no monitors comes
# first on its PATH), its monitors (a sysfs and /dev of its own) nor its buses (none): what it can
# do here is run the `echo` of the test's own actions. The main settings file is a link to a file
# out of the settings' folder, read-only, as Nix makes it. The service is stopped by its own PID.
set -euo pipefail
cd "$(dirname "$0")/../../.."

if [[ -z "${ELAMOND:-}" ]]; then
	(cd elamond && cargo build --locked --quiet)
	ELAMOND=elamond/target/debug/elamond
fi
ELAMOND=$(realpath "$ELAMOND")
[[ -x "$ELAMOND" ]] || { echo "browser.sh: no elamond at $ELAMOND" >&2; exit 2; }
python3 -I -c 'import playwright' 2>/dev/null || { echo "browser.sh: no Playwright for Python here; run it in nix develop .#browser" >&2; exit 2; }

# A port nothing listens on: never the real service's (47823), nor Sunshine's (47984–48010).
port=
for _ in $(seq 50); do
	p=$((40000 + RANDOM % 7000))
	if [[ -z "$(ss -Htln "sport = :$p")" ]]; then
		port=$p
		break
	fi
done
[[ -n "$port" ]] || { echo "browser.sh: found no free port" >&2; exit 2; }

work=$(mktemp -d)
service=
# Whether $1 is still the service this run started, and not another process given its PID since.
ours() { [[ -r /proc/$1/cmdline ]] && tr '\0' ' ' < "/proc/$1/cmdline" | grep -qF "$ELAMOND serve"; }
cleanup() {
	# The service writes its history as it stops: waited for, or it writes $work/data again once gone.
	if [[ -n "$service" ]] && ours "$service"; then
		kill "$service" 2>/dev/null || true
		wait "$service" 2>/dev/null || true
	fi
	if [[ -n "${BROWSER_SHOTS:-}" && -d "$work/shots" ]]; then
		mkdir -p "$BROWSER_SHOTS"
		cp -r "$work/shots/." "$BROWSER_SHOTS/"
	fi
	rm -rf "$work"
}
# On a failure on the way, as at the end.
trap cleanup EXIT

mkdir -p "$work/bin" "$work/cfg/elamond.d" "$work/data" "$work/home" "$work/nix" "$work/sys" "$work/dev" "$work/shots"
# A Hyprland without monitors, first on the service's PATH.
printf '#!/bin/sh\necho "[]"\n' > "$work/bin/hyprctl"
chmod 755 "$work/bin/hyprctl"

# The main file, as Nix would make it: the starter with the test's own keys, out of the settings'
# folder, read-only, and elamond.toml a link to it. The standard dashboards and rules beside it, as
# the starter writes them.
main=$work/nix/elamond.toml
sed -e 's/^control = false$/control = true/' \
	-e 's/^listen = \["::"\]$/listen = ["127.0.0.1"]/' \
	-e "s/^port = 47823\$/port = $port/" \
	-e 's/^mdns = true$/mdns = false/' \
	-e 's/^# name = "Desk"$/name = "Test computer"/' \
	elamond/src/starter.toml > "$main"
for line in 'control = true' 'listen = ["127.0.0.1"]' "port = $port" 'mdns = false' 'name = "Test computer"'; do
	grep -qxF "$line" "$main" || { echo "browser.sh: the starter has no line to make into: $line" >&2; exit 2; }
done
cat >> "$main" <<'TOML'

# The test's own actions: one that asks before it runs, one that doesn't.
[actions.greet]
name = "Greet"
command = ["echo", "hello from the browser test"]
confirm = true

[actions.quiet]
name = "Quiet"
command = ["echo", "quietly"]
confirm = false
TOML
chmod 444 "$main"
ln -s "$main" "$work/cfg/elamond.toml"
for f in dashboards events; do
	{ echo '#:schema ../elamond-schema.json'; cat "elamond/src/$f.toml"; } > "$work/cfg/elamond.d/$f.toml"
	chmod 600 "$work/cfg/elamond.d/$f.toml"
done

# elamond with the test's settings and data, and nothing of this session's: no buses, no Hyprland.
run=$work/elamond
{
	echo '#!/bin/sh'
	printf 'exec env -i PATH=%q HOME=%q ELAMON_CONFIG_DIR=%q ELAMON_DATA_DIR=%q ELAMON_SYSFS=%q ELAMON_DEV=%q ' \
		"$work/bin:$PATH" "$work/home" "$work/cfg" "$work/data" "$work/sys" "$work/dev"
	printf 'DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent '
	[[ -z "${ELAMON_PCI_IDS:-}" ]] || printf 'ELAMON_PCI_IDS=%q ' "$ELAMON_PCI_IDS"
	printf '%q "$@"\n' "$ELAMOND"
} > "$run"
chmod 755 "$run"
"$run" check > "$work/check.log" 2>&1 || { cat "$work/check.log" >&2; echo "browser.sh: the test's settings don't check" >&2; exit 1; }

# exec all the way down: $! is elamond itself.
"$run" serve > "$work/elamond.log" 2>&1 &
service=$!
for _ in $(seq 100); do
	[[ -z "$(ss -Htln "sport = :$port")" ]] || break
	ours "$service" || { cat "$work/elamond.log" >&2; echo "browser.sh: elamond stopped as it started" >&2; exit 1; }
	sleep 0.2
done
[[ -n "$(ss -Htln "sport = :$port")" ]] || { cat "$work/elamond.log" >&2; echo "browser.sh: elamond isn't listening on $port" >&2; exit 1; }

start=$(date +%s)
status=0
TMPDIR="$work" BROWSER_BASE="https://127.0.0.1:$port" BROWSER_WORK="$work" BROWSER_ELAMOND="$run" BROWSER_SHOT_DIR="$work/shots" \
	python3 -I elamond/web/test/browser.py "$@" || status=$?
echo "browser.sh: $(($(date +%s) - start)) s in the browser"
if ! ours "$service"; then
	echo "browser.sh: elamond is gone before the end; its log:" >&2
	cat "$work/elamond.log" >&2
	status=1
elif (( status != 0 )); then
	echo "browser.sh: the service's log, its last lines:" >&2
	tail -n 40 "$work/elamond.log" >&2
fi
cleanup
trap - EXIT
[[ -z "${BROWSER_SHOTS:-}" ]] || echo "browser.sh: screenshots in $BROWSER_SHOTS"
exit "$status"
