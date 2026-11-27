#!/usr/bin/env bash
# Desktop smoke test: runs paperlike in a throwaway X session (Xvfb, i3 with
# i3bar, picom) and checks the things unit tests can't reach: the overlay,
# the tray icon, fullscreen hiding, `paperlike hide`, live config edits.
#
#   tests/desktop.sh [path/to/paperlike]
#
# Needs Xvfb, i3, picom, xterm, xdotool, xwininfo and xrandr. Nothing outside
# the temporary session is touched.

set -euo pipefail

PAPERLIKE=$(realpath "${1:-target/debug/paperlike}")
export PAPERLIKE
WORK=$(mktemp -d)
pids=()
passed=0

cleanup() {
    local status=$?
    if [ -n "${DISPLAY:-}" ]; then
        timeout 5 "$PAPERLIKE" quit >/dev/null 2>&1 || true
    fi
    for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    if [ "$status" -ne 0 ]; then
        echo "--- paperlike log"
        cat "$WORK/paperlike.log" 2>/dev/null || true
    fi
    rm -rf "$WORK"
}
trap cleanup EXIT

ok() {
    passed=$((passed + 1))
    echo "ok $passed - $1"
}

fail() {
    echo "not ok - $1" >&2
    echo "--- status: $(timeout 5 "$PAPERLIKE" status 2>&1 || true)" >&2
    echo "--- windows:" >&2
    xwininfo -root -tree 2>/dev/null | grep -iE "paperlike|i3bar" >&2 || true
    exit 1
}

# wait_for <description> <seconds> <command...>: retries until it succeeds.
wait_for() {
    local desc=$1 secs=$2
    shift 2
    local end=$((SECONDS + secs))
    until "$@" >/dev/null 2>&1; do
        [ "$SECONDS" -lt "$end" ] || fail "$desc"
        sleep 0.2
    done
    ok "$desc"
}

overlays() { xdotool search --name '^Paperlike overlay$' 2>/dev/null || true; }

overlay_count_is() { [ "$(overlays | wc -l)" -eq "$1" ]; }

overlay_is() {
    local w
    w=$(overlays | head -1)
    [ -n "$w" ] && xwininfo -id "$w" | grep -q "Map State: $1"
}

tray_docked() {
    local t
    t=$(xdotool search --classname '^paperlike-tray$' 2>/dev/null | head -1)
    [ -n "$t" ] && xwininfo -id "$t" -tree | grep -q 'Parent window.*i3bar'
}

status_has() { timeout 5 "$PAPERLIKE" status | grep -q "$1"; }

# The daemon is our child, so once it exits it's a zombie until reaped.
exited() { [ ! -d "/proc/$1" ] || grep -q ') Z ' "/proc/$1/stat"; }

i3c() { timeout 5 i3-msg -q "$@"; }

# ---- session ------------------------------------------------------------

exec 3>"$WORK/display"
Xvfb -displayfd 3 -screen 0 1280x800x24 +extension RANDR +extension Composite \
    >"$WORK/xvfb.log" 2>&1 &
pids+=($!)
exec 3>&-
end=$((SECONDS + 10))
until [ -s "$WORK/display" ]; do
    [ "$SECONDS" -lt "$end" ] || { echo "Xvfb didn't start" >&2; exit 1; }
    sleep 0.1
done
display=$(cat "$WORK/display")
export DISPLAY=":$display"
export XDG_CONFIG_HOME="$WORK/config" XDG_RUNTIME_DIR="$WORK/run" I3SOCK="$WORK/i3.sock"
mkdir -p "$XDG_CONFIG_HOME/paperlike" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
output=$(xrandr --query | awk '$2 == "connected" { print $1; exit }')

cat >"$WORK/i3.conf" <<EOF
font pango:DejaVu Sans 8
ipc-socket $I3SOCK
bar {
    tray_output $output
}
EOF
i3 -c "$WORK/i3.conf" >"$WORK/i3.log" 2>&1 &
pids+=($!)
wait_for "i3 is running" 10 i3c nop

printf 'backend = "xrender";\nvsync = false;\nshadow = false;\nfading = false;\n' \
    >"$WORK/picom.conf"
picom --config "$WORK/picom.conf" >"$WORK/picom.log" 2>&1 &
pids+=($!)

config="$XDG_CONFIG_HOME/paperlike/paperlike.toml"
cat >"$config" <<EOF
theme_mode = "light"
light_texture = "matte"

[hotkeys]
enabled = false
EOF
"$PAPERLIKE" daemon >"$WORK/paperlike.log" 2>&1 &
daemon=$!
pids+=("$daemon")

# ---- checks -------------------------------------------------------------

wait_for "the daemon answers" 10 status_has "^on · Matte"
wait_for "the overlay covers the screen" 10 overlay_is IsViewable
overlay_count_is 1 || fail "exactly one overlay window"
ok "exactly one overlay window"
wait_for "the tray icon docks in i3bar" 10 tray_docked

# Changing monitors makes i3bar rebuild its bars, which takes the icon down.
xrandr --setmonitor SMOKE-LEFT 640/170x800/212+0+0 none >/dev/null
xrandr --setmonitor SMOKE-RIGHT 640/170x800/212+640+0 none >/dev/null
wait_for "one overlay despite overlapping monitors" 10 overlay_count_is 1
xrandr --delmonitor SMOKE-LEFT >/dev/null
xrandr --delmonitor SMOKE-RIGHT >/dev/null
wait_for "the tray icon re-docks after a monitor change" 10 tray_docked

i3c restart || true
sleep 1
wait_for "the tray icon re-docks after i3 restarts" 10 tray_docked

sed -i 's/^light_texture = .*/light_texture = "linen"/' "$config"
wait_for "a config edit applies on save" 5 status_has "· Linen ·"
echo 'enabled = ' >>"$config"
sleep 0.5
status_has "· Linen ·" || fail "a broken config edit is ignored"
ok "a broken config edit is ignored"
sed -i '/^enabled = $/d' "$config"

timeout 5 "$PAPERLIKE" texture parchment >/dev/null
wait_for "a screen-sized texture renders" 20 status_has "· Parchment ·"
wait_for "the overlay is back after rendering" 10 overlay_is IsViewable

xterm -T smoke-term -e sleep 600 2>/dev/null &
pids+=($!)
wait_for "a test window opens" 10 xdotool search --name '^smoke-term$'
i3c '[title="smoke-term"] fullscreen enable'
wait_for "a fullscreen window hides the texture" 5 status_has "fullscreen app"
wait_for "the overlay is unmapped while fullscreen" 5 overlay_is IsUnMapped
i3c '[title="smoke-term"] fullscreen disable'
wait_for "the texture returns after fullscreen" 5 overlay_is IsViewable

OVERLAY=$(overlays | head -1)
export OVERLAY
# shellcheck disable=SC2016 # expanded by the inner shell
timeout 10 "$PAPERLIKE" hide -- sh -c \
    '"$PAPERLIKE" status | grep -q "hidden for a command" &&
     xwininfo -id "$OVERLAY" | grep -q "Map State: IsUnMapped"' ||
    fail "paperlike hide hides the overlay while the command runs"
ok "paperlike hide hides the overlay while the command runs"
wait_for "the overlay returns after paperlike hide" 5 overlay_is IsViewable
set +e
timeout 10 "$PAPERLIKE" hide -- sh -c 'exit 7'
code=$?
set -e
[ "$code" -eq 7 ] || fail "paperlike hide passes the exit status through (got $code)"
ok "paperlike hide passes the exit status through"

timeout 5 "$PAPERLIKE" quit >/dev/null
wait_for "quit stops the daemon" 5 exited "$daemon"
[ ! -e "$XDG_RUNTIME_DIR/paperlike${DISPLAY//[^a-zA-Z0-9]/_}.sock" ] ||
    fail "quit removes the socket"
ok "quit removes the socket"

echo "all $passed checks passed"
