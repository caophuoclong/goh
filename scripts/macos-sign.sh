#!/bin/sh
# Sign goh-popup (an app bundle or a bare binary) with goh's certificate, so macOS keeps its Accessibility permission
# from one build to the next.
#
#   scripts/macos-sign.sh PATH...
#   cargo build && scripts/macos-sign.sh target/debug/goh-popup
#
# The identity is, in order: GOH_MACOS_SIGN_IDENTITY (a certificate's SHA-1 hash or name, from GOH_MACOS_KEYCHAIN if
# set; the release workflow sets both), then the certificate scripts/macos-signing-cert.sh made, at GOH_MACOS_SIGN_P12
# (default ../macos-sign/goh-signing.p12 beside this checkout, password in the same path + .password), imported into
# a keychain that lives only while this runs. Without either it signs ad hoc, and says so.
set -eu

[ $# -ge 1 ] || { sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }

identity=${GOH_MACOS_SIGN_IDENTITY-}
keychain=${GOH_MACOS_KEYCHAIN-}
work=
trap '[ -z "$work" ] || { security delete-keychain "$work/sign.keychain-db" 2>/dev/null; rm -rf "$work"; }' EXIT

if [ -z "$identity" ]; then
  p12=${GOH_MACOS_SIGN_P12:-$(cd "$(dirname "$0")/../.." && pwd)/macos-sign/goh-signing.p12}
  if [ -f "$p12" ] && [ -f "$p12.password" ]; then
    work=$(mktemp -d)
    keychain=$work/sign.keychain-db
    password=$(openssl rand -hex 24)
    security create-keychain -p "$password" "$keychain"
    security unlock-keychain -p "$password" "$keychain"
    security import "$p12" -k "$keychain" -P "$(cat "$p12.password")" -T /usr/bin/codesign >/dev/null
    security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" >/dev/null
    identity=$(security find-identity -p codesigning "$keychain" | awk '$1 == "1)" { print $2; exit }')
    [ -n "$identity" ] || { echo "$p12 has no code-signing identity" >&2; exit 1; }
  fi
fi

for path in "$@"; do
  if [ -n "$identity" ]; then
    codesign --force --sign "$identity" ${keychain:+--keychain "$keychain"} "$path"
  else
    echo "warning: no signing certificate (GOH_MACOS_SIGN_IDENTITY or GOH_MACOS_SIGN_P12): $path is signed ad hoc" >&2
    codesign --force --sign - "$path"
  fi
  codesign --verify --strict "$path"
done
