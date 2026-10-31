#!/bin/sh
# Run inside dbus-run-session and xvfb-run: starts Openbox, waits until it manages the display (before that the
# test terminal cannot take focus), then runs the X11 test with one terminal pane and with two.
set -eu
binary=$1
here=$(dirname "$0")
openbox >/tmp/openbox.log 2>&1 &
i=0
until xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q 'window id'; do
  i=$((i + 1))
  [ "$i" -le 50 ] || { echo "Openbox did not start" >&2; exit 1; }
  sleep 0.1
done
python3 "$here/linux-x11-test.py" "$binary"
GOH_TEST_SPLIT=1 python3 "$here/linux-x11-test.py" "$binary"
