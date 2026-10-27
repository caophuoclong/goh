#!/bin/sh
# Print the Homebrew formula for a release.
#
#   scripts/render-formula.sh VERSION SHA256SUMS > goh.rb
#
# SHA256SUMS is the release's checksum file (lines of "<sha256>  goh-VERSION-TARGET.tar.gz").
set -eu

[ $# -eq 2 ] || { echo "usage: $0 VERSION SHA256SUMS" >&2; exit 2; }
version=${1#v}
sums=$2

sum_of() {
  line=$(grep " goh-$version-$1.tar.gz\$" "$sums" || true)
  [ -n "$line" ] || { echo "no checksum for $1 in $sums" >&2; exit 1; }
  echo "${line%% *}"
}

# A missing checksum must stop the release, not put an empty one in the formula.
mac_arm=$(sum_of aarch64-apple-darwin) || exit 1
mac_x64=$(sum_of x86_64-apple-darwin) || exit 1
linux_arm=$(sum_of aarch64-unknown-linux-gnu) || exit 1
linux_x64=$(sum_of x86_64-unknown-linux-gnu) || exit 1

sed \
  -e "s/@VERSION@/$version/g" \
  -e "s/@SHA_AARCH64_APPLE_DARWIN@/$mac_arm/" \
  -e "s/@SHA_X86_64_APPLE_DARWIN@/$mac_x64/" \
  -e "s/@SHA_AARCH64_LINUX@/$linux_arm/" \
  -e "s/@SHA_X86_64_LINUX@/$linux_x64/" \
  "$(dirname "$0")/../packaging/homebrew/goh.rb.in"
