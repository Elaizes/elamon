#!/usr/bin/env bash
# End-to-end run on an emulator or phone that adb sees: builds nothing, starts a throwaway elamond
# from elamond/target/release on 127.0.0.1 (the emulator reaches it as 10.0.2.2), installs the
# debug APK, pairs, walks every screen and leaves the screenshots in $OUT.
#   cargo build --release (in elamond/) and ./gradlew :app:assembleDebug first.
#   FONT_SCALE=2.0 maestro/run.sh   repeats the screens with a large font.
set -euo pipefail
cd "$(dirname "$0")/.."
PORT=${PORT:-47911}
OUT=${OUT:-$(mktemp -d)}
FONT_SCALE=${FONT_SCALE:-1.0}
NAME="Test computer"
work=$(mktemp -d)
trap 'kill "$service" "${pairing:-}" 2>/dev/null; adb shell settings put system font_scale 1.0 >/dev/null; rm -rf "$work"' EXIT

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
# Always started in the background: exec makes $! elamond itself, so the trap can stop it.
run_elamond() {
	exec env -i PATH="$PATH" HOME="$work" ELAMON_CONFIG_DIR="$work/cfg" ELAMON_DATA_DIR="$work/data" \
		ELAMON_CONFIG="$work/cfg/elamond.toml" ${ELAMON_PCI_IDS:+ELAMON_PCI_IDS="$ELAMON_PCI_IDS"} \
		elamond/target/release/elamond "$@"
}
run_elamond serve > "$OUT/elamond.log" 2>&1 &
service=$!
sleep 2

adb uninstall io.github.elaizes.elamon > /dev/null 2>&1 || true
adb install -r "${APK:-app/build/outputs/apk/debug/app-debug.apk}" > /dev/null
adb shell settings put system font_scale "$FONT_SCALE"

run_elamond pair > "$work/pair.log" 2>&1 &
pairing=$!
sleep 3
code=$(grep -a -E '^\s+[0-9]{3} [0-9]{3}\s*$' "$work/pair.log" | tr -d ' ')
maestro test --test-output-dir "$OUT" -e ADDRESS="10.0.2.2:$PORT" -e CODE="$code" maestro/pair.yaml
maestro test --test-output-dir "$OUT" -e NAME="$NAME" maestro/screens.yaml
echo "screenshots: $OUT"
