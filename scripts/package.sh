#!/bin/sh
# Build the release binaries for one target and pack them: dist/goh-VERSION-TARGET.tar.gz (+ .sha256).
#
#   scripts/package.sh TARGET VERSION [--no-popup]
#   scripts/package.sh aarch64-apple-darwin v1.2.3
#
# The archive has one top-level folder with bin/{goh,gohd,goh-popup}, LICENSE and README.md. The popup is left
# out with --no-popup for minimal installations; inline fallback needs no popup.
# TARGET `host` builds for the machine this runs on.
#
# On macOS the popup is an app bundle, libexec/goh-popup.app, and bin/goh-popup links to its executable: macOS keeps
# the popup's own Accessibility permission under the bundle identifier and the signature's requirement. The bundle
# is signed by scripts/macos-sign.sh with goh's certificate (GOH_MACOS_SIGN_IDENTITY in the release workflow,
# ../macos-sign/goh-signing.p12 on a local build); that requirement then stays the same across releases and the
# permission survives updates. Without a certificate the bundle is signed ad hoc, and macOS asks again after every update.
set -eu

[ $# -ge 2 ] || { sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
target=$1
version=${2#v}
popup=1
[ "${3-}" = --no-popup ] && popup=0

cd "$(dirname "$0")/.."
if [ "$target" = host ]; then
  target=$(rustc -vV | sed -n 's/^host: //p')
fi

packages="-p goh-daemon"
bins="goh gohd"
if [ "$popup" = 1 ]; then
  packages="$packages -p goh-popup"
  bins="$bins goh-popup"
fi

# shellcheck disable=SC2086  # $packages is a list of flags
cargo build --release --locked --target "$target" $packages

name=goh-$version-$target
stage=dist/$name
rm -rf "$stage" "dist/$name.tar.gz" "dist/$name.tar.gz.sha256"
mkdir -p "$stage/bin"
for b in $bins; do
  cp "target/$target/release/$b" "$stage/bin/$b"
done
case $target in
  *-apple-darwin)
    if [ "$popup" = 1 ]; then
      scripts/macos-popup-app.sh "$stage/bin/goh-popup" "$version" "$stage/libexec/goh-popup.app"
      rm "$stage/bin/goh-popup"
      ln -s ../libexec/goh-popup.app/Contents/MacOS/goh-popup "$stage/bin/goh-popup"
    fi
    ;;
esac
cp LICENSE README.md "$stage/"

tar -C dist -czf "dist/$name.tar.gz" "$name"
rm -rf "$stage"

cd dist
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$name.tar.gz" > "$name.tar.gz.sha256"
else
  shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256"
fi
echo "dist/$name.tar.gz"
