#!/bin/sh
# Rebuild the popup's icon font: the glyphs of crates/goh-theme/src/icons.rs cut out of Nerd Fonts' symbols font.
#
#   scripts/subset-nerd-font.sh SymbolsNerdFontMono-Regular.ttf
#
# The font is in NerdFontsSymbolsOnly.zip of a Nerd Fonts release; pyftsubset comes with fonttools (pip install
# fonttools). Run it after changing a glyph in icons.rs; the goh-popup tests fail until the font has it.
set -eu

[ $# -eq 1 ] || { echo "usage: $0 SymbolsNerdFontMono-Regular.ttf" >&2; exit 2; }
root=$(cd "$(dirname "$0")/.." && pwd)
codes=$(grep -o '\\u{[0-9a-f]*}' "$root/crates/goh-theme/src/icons.rs" | sed 's/\\u{\(.*\)}/U+\1/' | sort -u | paste -sd, -)
pyftsubset "$1" --unicodes="$codes" --no-hinting --desubroutinize --layout-features='' --name-IDs='*' \
  --output-file="$root/crates/goh-popup/assets/nerd-symbols.ttf"
