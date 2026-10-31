#!/bin/sh
# Make Goh.app around a built goh-popup and sign it (scripts/macos-sign.sh).
#
#   scripts/macos-popup-app.sh BINARY VERSION APP
#   scripts/macos-popup-app.sh target/release/goh-popup 1.2.3 dist/Goh.app
#
# macOS keeps the popup's own Accessibility permission under the bundle identifier (packaging/macos) and the
# signature's requirement, so both must stay the same from one build to the next. The icon is
# packaging/macos/Goh.icns, or made from packaging/macos/Goh.png (1024x1024) when only that is there; with neither
# the app has macOS's generic icon.
set -eu

[ $# -eq 3 ] || { sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
binary=$1
version=${2#v}
app=$3
here=$(cd "$(dirname "$0")" && pwd)
art=$here/../packaging/macos

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/goh-popup"
sed "s/@VERSION@/$version/g" "$art/Goh.Info.plist" > "$app/Contents/Info.plist"
if [ -f "$art/Goh.icns" ]; then
  cp "$art/Goh.icns" "$app/Contents/Resources/Goh.icns"
elif [ -f "$art/Goh.png" ]; then
  set_dir=$(mktemp -d)/Goh.iconset
  mkdir -p "$set_dir"
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$art/Goh.png" --out "$set_dir/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$art/Goh.png" --out "$set_dir/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$set_dir" -o "$app/Contents/Resources/Goh.icns"
  rm -rf "$(dirname "$set_dir")"
fi
"$here/macos-sign.sh" "$app"
