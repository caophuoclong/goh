#!/bin/sh
# Install goh and its shell integration in one command:
#
#   curl -fsSL https://raw.githubusercontent.com/caophuoclong/goh/main/install.sh | sh
#   ./install.sh                       the same, from a checkout
#
#   ./install.sh --version 1.2.3       a given release instead of the latest
#   ./install.sh --from-source         build with cargo instead of downloading (this is also the fallback)
#   ./install.sh --no-rc               install (or uninstall) the binaries only; do not touch any shell startup file
#   ./install.sh --uninstall           remove the binaries and the block this script added to your shell file
#
# It downloads the release package for your system (macOS or Linux, Intel or ARM), checks its SHA-256, and puts goh,
# gohd and (on macOS/Linux) goh-popup in $GOH_BIN (default ~/.local/bin); on macOS goh-popup links to goh-popup.app
# in $GOH_LIB (default ~/.local/lib/goh). Then it adds one marked block to ~/.zshrc,
# ~/.bashrc or fish's config.fish ($GOH_RC names another file); --uninstall removes exactly that block.
# Re-running updates the binaries and leaves the block as it is. Building from source needs Rust (https://rustup.rs).

set -eu

SLUG=${GOH_SLUG:-caophuoclong/goh}
REPO=${GOH_REPO:-https://github.com/$SLUG.git}
RELEASES=${GOH_RELEASES:-https://github.com/$SLUG/releases}
BIN=${GOH_BIN:-$HOME/.local/bin}
LIB=${GOH_LIB:-$HOME/.local/lib/goh}
BEGIN='# >>> goh >>>'
END='# <<< goh <<<'

say() { printf 'goh: %s\n' "$*"; }
die() { printf 'goh: %s\n' "$*" >&2; exit 1; }

action=install
touch_rc=1
from_source=0
version=${GOH_VERSION:-}
while [ $# -gt 0 ]; do
  case $1 in
    --uninstall) action=uninstall ;;
    --no-rc) touch_rc=0 ;;
    --from-source) from_source=1 ;;
    --version) [ $# -ge 2 ] || die "--version needs a number, e.g. --version 1.2.3"; version=$2; shift ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1 (see --help)" ;;
  esac
  shift
done
version=${version#v}

# The shell to integrate with, and the file that starts it.
shell_name=$(basename "${GOH_SHELL:-${SHELL:-sh}}")
case $shell_name in
  zsh)  rc=${ZDOTDIR:-$HOME}/.zshrc ;;
  bash) rc=$HOME/.bashrc; [ "$(uname)" = Darwin ] && [ ! -f "$rc" ] && rc=$HOME/.bash_profile ;;
  fish) rc=${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish ;;
  *)    rc= ;;
esac
rc=${GOH_RC:-$rc}

# Remove the block this script added, and nothing else.
remove_block() {
  [ -n "$rc" ] && [ -f "$rc" ] && grep -qF "$BEGIN" "$rc" || return 0
  tmp=$(mktemp)
  awk -v b="$BEGIN" -v e="$END" '$0 == b { skip = 1 } !skip { print } $0 == e { skip = 0 }' "$rc" > "$tmp"
  cat "$tmp" > "$rc"
  rm -f "$tmp"
  say "removed the goh block from $rc"
}

if [ "$action" = uninstall ]; then
  if command -v goh >/dev/null 2>&1; then goh stop >/dev/null 2>&1 || true; fi
  pkill -U "$(id -u)" -x goh-popup 2>/dev/null || true
  for f in goh gohd goh-popup; do rm -f "$BIN/$f"; done
  rm -rf "$LIB/goh-popup.app"
  rmdir "$LIB" 2>/dev/null || true
  [ "$touch_rc" = 0 ] || remove_block
  say "uninstalled. Open shells keep the adapter until you run: exec \$SHELL"
  say "your specs, settings and learned data stay (~/.config/goh, ~/.local/share/goh); \`rm -r\` them if you want them gone"
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$BIN"

# ---- a release package -----------------------------------------------------------------------------------------------

target_triple() {
  case $(uname -s) in Darwin) os=apple-darwin ;; Linux) os=unknown-linux-gnu ;; *) return 1 ;; esac
  case $(uname -m) in arm64|aarch64) arch=aarch64 ;; x86_64|amd64) arch=x86_64 ;; *) return 1 ;; esac
  echo "$arch-$os"
}

latest_version() {
  url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$RELEASES/latest") || return 1
  case $url in */tag/v*) echo "${url##*/tag/v}" ;; *) return 1 ;; esac
}

sha256_check() { # run in the directory of the files; the .sha256 file names the archive
  if command -v sha256sum >/dev/null 2>&1; then sha256sum -c "$1" >/dev/null; else shasum -a 256 -c "$1" >/dev/null; fi
}

install_release() {
  command -v curl >/dev/null 2>&1 || { say "curl not found"; return 1; }
  triple=$(target_triple) || { say "no package for $(uname -s) $(uname -m)"; return 1; }
  ver=$version
  [ -n "$ver" ] || ver=$(latest_version) || { say "could not find the latest release of $SLUG"; return 1; }
  name=goh-$ver-$triple
  say "downloading $name"
  ( cd "$work" &&
    curl -fsSL -o "$name.tar.gz" "$RELEASES/download/v$ver/$name.tar.gz" &&
    curl -fsSL -o SHA256SUMS "$RELEASES/download/v$ver/SHA256SUMS" &&
    grep " $name.tar.gz\$" SHA256SUMS > "$name.tar.gz.sha256" &&
    sha256_check "$name.tar.gz.sha256" &&
    tar -xzf "$name.tar.gz" ) || { say "download or checksum failed for $name"; return 1; }
  for f in "$work/$name"/bin/*; do
    b=$(basename "$f")
    [ -L "$f" ] && continue  # macOS goh-popup: a link into its app bundle, installed below
    # Replace through a rename: a running copy keeps working until it exits.
    cp "$f" "$BIN/.$b.new" && mv -f "$BIN/.$b.new" "$BIN/$b"
  done
  # The app bundle keeps macOS's Accessibility permission for goh-popup itself across updates.
  if [ -d "$work/$name/libexec/goh-popup.app" ]; then
    mkdir -p "$LIB"
    rm -rf "$LIB/.goh-popup.app.new"
    if ! { cp -R "$work/$name/libexec/goh-popup.app" "$LIB/.goh-popup.app.new" &&
           rm -rf "$LIB/goh-popup.app" && mv "$LIB/.goh-popup.app.new" "$LIB/goh-popup.app" &&
           ln -sf "$LIB/goh-popup.app/Contents/MacOS/goh-popup" "$BIN/.goh-popup.new" &&
           mv -f "$BIN/.goh-popup.new" "$BIN/goh-popup"; }; then
      say "could not install goh-popup.app in $LIB"
      return 1
    fi
  fi
  # A popup left from an earlier version (older/minimal packages may have none) would be stale.
  [ -x "$work/$name/bin/goh-popup" ] || rm -f "$BIN/goh-popup"
  say "installed goh $ver in $BIN"
}

# ---- from source -----------------------------------------------------------------------------------------------------

install_source() {
  command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust first (https://rustup.rs), or use a release package"
  here=$(cd "$(dirname "$0")" 2>/dev/null && pwd) || here=
  if [ -n "$here" ] && [ -f "$here/Cargo.toml" ] && grep -q 'crates/goh-daemon' "$here/Cargo.toml"; then
    src=$here
  else
    command -v git >/dev/null 2>&1 || die "git not found (needed to fetch the source)"
    src=$work/src
    say "fetching $REPO"
    git clone --depth 1 --quiet ${version:+--branch "v$version"} "$REPO" "$src"
  fi
  say "building (release); the first build takes a few minutes"
  log=$work/build.log
  # Build the popup on supported desktop platforms; unsupported sessions still use inline.
  packages="-p goh-daemon"
  bins="goh gohd"
  case "$(uname -s)" in
    Darwin|Linux) packages="$packages -p goh-popup"; bins="$bins goh-popup" ;;
  esac
  # shellcheck disable=SC2086  # $packages is a list of flags
  if ! (cd "$src" && cargo build --release --locked $packages) >"$log" 2>&1; then
    tail -n 25 "$log" >&2
    die "the build failed (output above)"
  fi
  for b in $bins; do
    [ -x "$src/target/release/$b" ] || die "the build did not produce $b"
    cp "$src/target/release/$b" "$BIN/.$b.new" && mv -f "$BIN/.$b.new" "$BIN/$b"
  done
  say "built and installed in $BIN"
}

if [ "$from_source" = 1 ]; then
  install_source
elif ! install_release; then
  [ -z "$version" ] || die "could not install goh $version from a release package"
  say "falling back to building from source"
  install_source
fi

# ---- shell integration -----------------------------------------------------------------------------------------------

# Make sure the shell finds the binaries, and load the adapter, through one marked block.
if [ "$touch_rc" = 1 ]; then
  if [ -z "$rc" ]; then
    say "unknown shell \`$shell_name\`: add \`eval \"\$(goh init <shell>)\"\` to its startup file (zsh, bash, fish, pwsh, nu, elvish, xonsh)"
  elif [ -f "$rc" ] && grep -qF "$BEGIN" "$rc"; then
    say "$rc already has the goh block"
  else
    mkdir -p "$(dirname "$rc")"
    backup=
    if [ -f "$rc" ] && [ ! -f "$rc.goh-backup" ]; then cp "$rc" "$rc.goh-backup"; backup=" (backup: $rc.goh-backup)"; fi
    # shellcheck disable=SC2016  # the $ in these lines is for the user's shell, not for this script
    {
      printf '\n%s\n' "$BEGIN"
      if [ "$shell_name" = fish ]; then
        printf 'fish_add_path %s\n' "$BIN"
        printf 'goh init fish | source\n'
      else
        printf 'case ":$PATH:" in *":%s:"*) ;; *) export PATH="%s:$PATH" ;; esac\n' "$BIN" "$BIN"
        printf 'eval "$(goh init %s)"\n' "$shell_name"
      fi
      printf '%s\n' "$END"
    } >> "$rc"
    say "added the goh block to $rc$backup"
  fi
fi

PATH=$BIN:$PATH
goh restart >/dev/null 2>&1 || true
goh doctor 2>&1 | sed 's/^/goh: /' || true
say "done. Open a new terminal (or run: exec \$SHELL) and type: git ch"
