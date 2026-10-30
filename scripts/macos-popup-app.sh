#!/bin/sh
# Make goh-popup.app around a built goh-popup and sign it (scripts/macos-sign.sh).
#
#   scripts/macos-popup-app.sh BINARY VERSION APP
#   scripts/macos-popup-app.sh target/release/goh-popup 1.2.3 dist/goh-popup.app
#
# macOS keeps the popup's own Accessibility permission under the bundle identifier (packaging/macos) and the
# signature's requirement, so both must stay the same from one build to the next.
set -eu

[ $# -eq 3 ] || { sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
binary=$1
version=${2#v}
app=$3
here=$(cd "$(dirname "$0")" && pwd)

rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$binary" "$app/Contents/MacOS/goh-popup"
sed "s/@VERSION@/$version/g" "$here/../packaging/macos/goh-popup.Info.plist" > "$app/Contents/Info.plist"
"$here/macos-sign.sh" "$app"
