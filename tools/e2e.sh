#!/usr/bin/env bash
# End-to-end check of the real daemon, bar module and CLI against fake MPRIS
# players on a private session bus. Needs dbus-run-session, jq and
# python-gobject (tools/fake-player.py).
#
#   tools/e2e.sh [path/to/tannoy]      (default: target/debug/tannoy)
#
# Every check polls for up to 5 seconds instead of sleeping a fixed time.
set -euo pipefail

if [[ -z ${TANNOY_E2E_INNER:-} ]]; then
  bin=$(realpath "${1:-target/debug/tannoy}")
  [[ -x $bin ]] || { echo "e2e: no tannoy at $bin (cargo build first)" >&2; exit 1; }
  # A private bus, and nothing from the desktop this may run on.
  exec env -u HYPRLAND_INSTANCE_SIGNATURE -u WAYLAND_DISPLAY \
    TANNOY_E2E_INNER=1 TANNOY_BIN="$bin" \
    dbus-run-session -- "$0"
fi

here=$(cd "$(dirname "$0")" && pwd)
tannoy=$TANNOY_BIN
python=${PYTHON:-python3}
# Socket paths must stay short (sun_path), so not a deep workspace path.
root=$(mktemp -d /tmp/tn-e2e.XXXXXX)
export TANNOY_RUNTIME_DIR=$root/run TANNOY_STATE_DIR=$root/state
export TANNOY_CACHE_DIR=$root/cache TANNOY_CONFIG_DIR=$root/config
mkdir -p "$TANNOY_CONFIG_DIR"
printf '[art]\nsignal = 0\n' > "$TANNOY_CONFIG_DIR/config.toml"

pids=()
cleanup() {
  kill "${pids[@]}" 2>/dev/null || true
  pkill -f "^$tannoy daemon" 2>/dev/null || true
  rm -rf "$root"
}
trap cleanup EXIT

passed=0
fail() {
  echo "e2e: FAIL: $1" >&2
  echo "--- bar" >&2; tail -5 "$root/bar.out" >&2 || true
  echo "--- status" >&2; "$tannoy" status >&2 || true
  echo "--- daemon.log" >&2; cat "$TANNOY_STATE_DIR/daemon.log" >&2 || true
  exit 1
}
# expect "what" 'shell condition'
expect() {
  for _ in $(seq 50); do
    if eval "$2" >/dev/null 2>&1; then
      passed=$((passed + 1))
      echo "ok   $1"
      return
    fi
    sleep 0.1
  done
  fail "$1"
}
status() { "$tannoy" status --json; }
# run tannoy <args>, failing the test with its output if it fails
run() { "$tannoy" "$@" > "$root/run.out" 2>&1 || fail "tannoy $* ($(cat "$root/run.out"))"; }
bar() { tail -1 "$root/bar.out"; }
fake() {
  "$python" "$here/fake-player.py" "$@" > "$root/fake-$2.out" 2>&1 &
  pids+=($!)
}

# A tiny PNG for file:// cover art.
"$python" - "$root/cover.png" <<'EOF'
import struct, sys, zlib
def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"\x00\x88\xc0\xd0")) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
EOF

fake --name daft --identity Daft --modes --title "Get Lucky" --title "Instant Crush" \
  --artist "Daft Punk" --art "file://$root/cover.png"
"$tannoy" bar > "$root/bar.out" 2>&1 &
pids+=($!)

expect "the bar starts the daemon and shows the track" \
  'bar | jq -e ".text == \"Daft Punk · Get Lucky\" and .class == [\"playing\", \"with-art\"]"'
expect "status sees the player" \
  'status | jq -e ".active == \"org.mpris.MediaPlayer2.daft\""'
expect "the cover is cached and tannoy art prints it" \
  '[[ $("$tannoy" art) == "$TANNOY_CACHE_DIR/art/"*.png && -s $("$tannoy" art) ]]'

run next
expect "next reaches the player" 'bar | jq -e ".text == \"Daft Punk · Instant Crush\""'
run play-pause
expect "play-pause pauses" 'bar | jq -e ".class[0] == \"paused\""'
run seek 90
expect "seek sets the position" \
  'status | jq -e "[.players[] | select(.key == \"daft\")][0].position >= 89000000"'
run seek -30
expect "a relative seek moves back" \
  'status | jq -e "[.players[] | select(.key == \"daft\")][0].position | . >= 59000000 and . < 61000000"'

run shuffle
expect "shuffle flips" 'status | jq -e "[.players[] | select(.key == \"daft\")][0].shuffle == true"'
run loop
run loop
expect "loop steps to one track" 'status | jq -e "[.players[] | select(.key == \"daft\")][0].loop_status == \"Track\""'
run loop none
expect "loop takes a setting" 'status | jq -e "[.players[] | select(.key == \"daft\")][0].loop_status == \"None\""'

fake --name other --identity Other --title "Somewhere Else" --artist "Someone"
expect "a player that starts playing takes the bar" \
  'bar | jq -e ".text == \"Someone · Somewhere Else\""'
run pick daft
expect "a pick wins over the latest" \
  'status | jq -e ".picked == \"daft\" and .active == \"org.mpris.MediaPlayer2.daft\""'
expect "a pick survives in state.json" \
  'jq -e ".picked == \"daft\"" "$TANNOY_STATE_DIR/state.json"'
if "$tannoy" raise 2>/dev/null; then fail "raise on a player that cannot raise"; fi
expect "raise explains a player that cannot" \
  '{ "$tannoy" raise 2>&1 || true; } | grep -q "cannot bring its window forward"'

printf '[art]\nsignal = 0\n[bar]\nmax_chars = 12\n' > "$TANNOY_CONFIG_DIR/config.toml"
expect "a config change applies without a restart" \
  'bar | jq -e ".text == \"Daft Punk…\""'
printf '[art]\nsignal = 0\n[bar]\nmax_char = 12\n' > "$TANNOY_CONFIG_DIR/config.toml"
expect "a broken config reports its error" 'status | jq -e ".config_error != null"'
printf '[art]\nsignal = 0\n' > "$TANNOY_CONFIG_DIR/config.toml"

old=$(pgrep -f "^$tannoy daemon")
run restart
expect "restart replaces the daemon" \
  '[[ $(pgrep -f "^$tannoy daemon") != "$old" ]] && status | jq -e ".players | length >= 2"'

run pick auto
kill "${pids[0]}" "${pids[2]}"
expect "with every player gone the bar empties" \
  'bar | jq -e ".text == \"\" and .class == [\"stopped\"]"'

echo "e2e: $passed checks passed"
