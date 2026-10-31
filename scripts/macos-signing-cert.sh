#!/bin/sh
# Make the certificate that signs Goh.app (the popup) in releases, once.
#
#   scripts/macos-signing-cert.sh DIR
#
# Writes DIR/goh-signing.p12 (key and self-signed code-signing certificate, valid 20 years) and its password in
# DIR/goh-signing.p12.password, then prints how to store both as the repository secrets MACOS_SIGN_P12 and
# MACOS_SIGN_P12_PASSWORD. macOS keeps goh-popup's Accessibility permission for as long as releases are signed with
# this same certificate: keep the files somewhere safe, since a new certificate makes every user allow Goh again.
set -eu

[ $# -eq 1 ] || { sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
dir=$1
p12=$dir/goh-signing.p12
[ ! -e "$p12" ] || { echo "$p12 already exists; releases must keep using it" >&2; exit 1; }
mkdir -p "$dir"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cat > "$work/cert.cnf" <<'EOF'
[req]
distinguished_name = dn
prompt = no
x509_extensions = ext
[dn]
CN = goh code signing
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
EOF
openssl req -x509 -newkey rsa:3072 -nodes -days 7300 -config "$work/cert.cnf" -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
password=$(openssl rand -hex 24)
# macOS's `security import` reads only the older PKCS#12 encryption, which OpenSSL 3 writes with -legacy.
legacy=
openssl pkcs12 -help 2>&1 | grep -q -- -legacy && legacy=-legacy
umask 077
openssl pkcs12 -export $legacy -name "goh code signing" -inkey "$work/key.pem" -in "$work/cert.pem" -out "$p12" -passout "pass:$password"
printf '%s' "$password" > "$p12.password"

echo "wrote $p12 and $p12.password"
echo "store them as repository secrets:"
echo "  base64 < $p12 | gh secret set MACOS_SIGN_P12"
echo "  gh secret set MACOS_SIGN_P12_PASSWORD < $p12.password"
