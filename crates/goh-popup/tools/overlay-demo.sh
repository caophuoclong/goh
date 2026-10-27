#!/usr/bin/env bash
# Live check of the caret anchor + overlay gate. Run it at a shell prompt in iTerm2 or Terminal.app
# (not tmux/ssh), with that app enabled under System Settings > Privacy & Security > Accessibility.
#
#   cargo build -p goh-popup && crates/goh-popup/tools/overlay-demo.sh
#
# 1. prints what the gate decides for this terminal (`goh-popup --resolve`);
# 2. types "git c" without a newline, so the caret sits right after it, and shows the popup for ~6 s:
#    it should appear just below that line (above it if you are on the bottom rows).
# Tell me: the printed decisions, and whether the popup was where the caret is.
set -euo pipefail
cd "$(dirname "$0")/../../.."
POPUP=${1:-target/debug/goh-popup}
FRAME=crates/goh-popup/tools/sample-frame-auto.json
OUT=$(mktemp)
trap 'rm -f "$OUT"' EXIT

echo "--- gate for this terminal:"
"$POPUP" --resolve || true

echo "--- showing the popup at the caret for about 6 seconds... (do not press keys)"
printf '$ git c'
# Ask the terminal itself where its cursor is (ESC[6n answers ESC[row;colR), without moving it, so it
# can be compared with the caret the Accessibility API reports. Printed after the popup is gone.
reply=
IFS= read -r -s -d R -t 1 -p $'\e[6n' reply || true
( cat "$FRAME"; sleep 6; echo '{"type":"quit"}' ) | "$POPUP" >"$OUT" 2>/dev/null || true
printf '\n--- terminal says the cursor was at (row;col): %s\n' "${reply#*[}"
printf -- '--- popup decision:\n'
cat "$OUT"
echo "--- the popup's y is the caret's y + its height + 12; flipped above it when there is no room below"
