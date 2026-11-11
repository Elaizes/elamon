#!/usr/bin/env bash
# End-to-end run on an emulator that adb sees: builds nothing, starts a throwaway elamond from
# elamond/target/release on 127.0.0.1 (the emulator reaches it as 10.0.2.2), installs the debug APK
# (io.github.elaizes.elamon.debug, beside the real app and never in its place), pairs, walks every
# screen, makes the phone a glance one and an admin again with elamond clients role, and leaves the
# screenshots in $OUT.
#   cargo build --release (in elamond/) and ./gradlew :app:assembleDebug first.
#   FONT_SCALE=2.0 maestro/run.sh   repeats the screens with a large font; the phone's own comes back after.
#   maestro/run.sh --yes-real-phone  runs on a phone that isn't an emulator: it wipes the debug app's
#                                    data there and changes the font scale while it runs.
# With more than one device attached, ANDROID_SERIAL says which; adb and maestro both use it.
set -euo pipefail
cd "$(dirname "$0")/.."
APP_ID=io.github.elaizes.elamon.debug
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
trap 'kill "${service:-}" "${pairing:-}" 2>/dev/null; restore_font_scale; rm -rf "$work"' EXIT

mkdir -p "$work/cfg" "$work/data"
cat > "$work/cfg/elamond.toml" <<TOML
port = $PORT
listen = "127.0.0.1"
mdns = false
name = "$NAME"
control = true

[[actions]]
name = "Say hello"
command = ["echo", "hello from the test service"]
confirm = false

[[links]]
name = "Local web"
url = "http://{host}:$((PORT + 1))"
TOML
# Always started in the background: exec makes $! elamond itself, so the trap can stop it. No
# system bus: power, units and the rest of this computer stay out of the test's reach.
run_elamond() {
	exec env -i PATH="$PATH" HOME="$work" ELAMON_CONFIG_DIR="$work/cfg" ELAMON_DATA_DIR="$work/data" \
		ELAMON_CONFIG="$work/cfg/elamond.toml" DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent \
		${ELAMON_PCI_IDS:+ELAMON_PCI_IDS="$ELAMON_PCI_IDS"} \
		elamond/target/release/elamond "$@"
}
run_elamond serve > "$OUT/elamond.log" 2>&1 &
service=$!
sleep 2

adb uninstall "$APP_ID" > /dev/null 2>&1 || true
adb install -r "${APK:-app/build/outputs/apk/debug/app-debug.apk}" > /dev/null
adb shell settings put system font_scale "$FONT_SCALE"

run_elamond pair > "$work/pair.log" 2>&1 &
pairing=$!
sleep 3
code=$(grep -a -E '^\s+[0-9]{3} [0-9]{3}\s*$' "$work/pair.log" | tr -d ' ')
flow() { maestro --device "$ANDROID_SERIAL" test --test-output-dir "$OUT" "$@"; }
flow -e ADDRESS="10.0.2.2:$PORT" -e CODE="$code" maestro/pair.yaml
flow -e NAME="$NAME" maestro/screens.yaml
# In a subshell: run_elamond execs. A glance phone sees the basics only.
(run_elamond clients role all glance)
flow -e NAME="$NAME" maestro/glance.yaml
# Admin again while the computer's screen stays open: the app notices without being reopened.
(run_elamond clients role all admin)
flow maestro/admin-again.yaml
echo "screenshots: $OUT"
