#!/usr/bin/env bash
# macOS: does showing the popup change which app is frontmost? (ADR-002 kill criterion: stealing focus.)
# Run from a real terminal window of the app you care about (Terminal, iTerm2), not from a sandboxed
# tool shell: those cannot see the real frontmost app. While it runs, keep typing in the terminal;
# the characters you type must keep arriving there.
#
#   cargo build -p goh-popup && crates/goh-popup/tools/focus-check.sh
set -euo pipefail
cd "$(dirname "$0")/../../.."
POPUP=${1:-target/debug/goh-popup}
FRAME=${2:-crates/goh-popup/tools/sample-frame.json}

front() { lsappinfo front | head -1; }
name() { lsappinfo list | grep -F "$1" | head -1 | sed 's/^[0-9]*) //;s/ ASN:.*//'; }

before=$(front)
echo "frontmost before: $before $(name "$before")"
# loginwindow (ASN 0x0-0x1001) in front means this shell cannot see the user's real session.
if [[ $before == *0x0-0x1001* ]]; then
  echo "INCONCLUSIVE: this shell sees 'loginwindow' as frontmost; run it from Terminal or iTerm2 itself" >&2
  exit 2
fi
# Feed the frame, keep stdin open while we watch, then ask the popup to quit.
( cat "$FRAME"; sleep 5; echo '{"type":"quit"}' ) | "$POPUP" 2>/dev/null &
POP_PID=$!
status=0
# Sample every 0.2 s for 4 s so a brief flash of focus is not missed.
for i in $(seq 1 20); do
  sleep 0.2
  now=$(front)
  if [[ $now != "$before" ]]; then
    status=1
    echo "t+$((i * 2))/10 s frontmost: $now $(name "$now") -> CHANGED"
  fi
done
[[ $status == 0 ]] && echo "frontmost stayed: $before $(name "$before")"
wait "$POP_PID" 2>/dev/null || true
[[ $status == 0 ]] && echo "PASS: the popup never became the frontmost app" || echo "FAIL: the popup took focus"
exit $status
