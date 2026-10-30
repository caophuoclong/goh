#!/bin/sh
# Run inside dbus-run-session as a non-root user with Sway and XWayland installed.
set -eu
# Linux dash/bash support disabling core dumps; this fixture is Linux-only.
# shellcheck disable=SC3045
ulimit -c 0
[ -d /tmp/.X11-unix ] || mkdir -m 1777 /tmp/.X11-unix
runtime=$(mktemp -d)
export XDG_RUNTIME_DIR="$runtime"
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
chmod 700 "$runtime"
binary=$(realpath "$1")
cat > "$runtime/config" <<CONFIG
output * resolution 1280x1024
xwayland force
focus_follows_mouse no
for_window [title="Goh X11 test terminal"] floating enable
exec sh -c 'GDK_BACKEND=x11 python3 crates/goh-popup/tools/linux-x11-test.py "$binary" > "$runtime/result.log" 2>&1; echo \$? > "$runtime/status"'
CONFIG
sway -c "$runtime/config" > "$runtime/sway.log" 2>&1 &
sway_pid=$!
trap 'kill "$sway_pid" 2>/dev/null || true' EXIT INT TERM
attempt=0
while [ ! -f "$runtime/status" ] && [ "$attempt" -lt 400 ]; do
  kill -0 "$sway_pid" 2>/dev/null || { cat "$runtime/sway.log"; exit 1; }
  sleep 0.1
  attempt=$((attempt + 1))
done
cat "$runtime/result.log" 2>/dev/null || true
[ -f "$runtime/status" ] || { cat "$runtime/sway.log"; echo 'XWayland test timed out' >&2; exit 1; }
[ "$(cat "$runtime/status")" = 0 ]
