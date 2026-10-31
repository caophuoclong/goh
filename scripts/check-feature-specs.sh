#!/bin/sh
# Check the feature specs in docs/features (see its README).
#
#   scripts/check-feature-specs.sh            every spec has its front matter, and the index lists it
#   BASE=<sha> TITLE=<pr title> scripts/check-feature-specs.sh
#                                             also: a feat/fix/perf change to what goh does updates a spec
#
# NO_SPEC=true (the `no-spec` label in CI) skips the second check, for a change no user can notice.
set -eu

cd "$(dirname "$0")/.."
dir=docs/features
fail=0
err() { printf 'feature specs: %s\n' "$*" >&2; fail=1; }

field() { # FILE KEY -> the value in the front matter, or nothing
  awk -v k="$2" 'NR == 1 && $0 != "---" { exit } NR > 1 && $0 == "---" { exit } NR > 1 && index($0, k ": ") == 1 { print substr($0, length(k) + 3); exit }' "$1"
}

for f in "$dir"/*.md; do
  name=$(basename "$f" .md)
  [ "$name" = README ] && continue
  [ "$(head -n 1 "$f")" = --- ] || { err "$f: starts without front matter (---)"; continue; }
  [ "$(field "$f" id)" = "$name" ] || err "$f: id must be \`$name\`, the file's name"
  for k in title since updated; do
    [ -n "$(field "$f" "$k")" ] || err "$f: no \`$k\`"
  done
  case $(field "$f" status) in
    stable|experimental|planned|removed) ;;
    *) err "$f: status must be stable, experimental, planned or removed" ;;
  esac
  field "$f" updated | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' || err "$f: updated must be a date, YYYY-MM-DD"
  grep -q '^## Changes' "$f" || err "$f: no \`## Changes\` section"
  grep -q "($name.md)" "$dir/README.md" || err "$dir/README.md: the index does not list $name.md"
done

if [ -n "${BASE-}" ] && [ "${NO_SPEC-}" != true ]; then
  case ${TITLE-} in
    feat*|fix*|perf*)
      changed=$(git diff --name-only "$BASE"...HEAD)
      if printf '%s\n' "$changed" | grep -Eq '^(crates/[^/]+/src/|crates/goh-core/specs/|adapters/|install\.sh$|packaging/)' &&
         ! printf '%s\n' "$changed" | grep -q "^$dir/"; then
        err "this change alters what goh does but no spec in $dir changed: update the feature's spec (its text and a line under Changes), or label the PR no-spec if no user can notice"
      fi
      ;;
  esac
fi

exit "$fail"
