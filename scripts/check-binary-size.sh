#!/bin/sh
# Fail when a release binary grew past its budget. Budgets are in scripts/binary-size-budget.txt, one
# "name bytes" per line; build first with `cargo build --release --locked -p goh-daemon -p goh-popup`.
set -eu
cd "$(dirname "$0")/.."
status=0
while read -r name budget; do
  case $name in '' | '#'*) continue ;; esac
  size=$(wc -c < "target/release/$name" | tr -d ' ')
  if [ "$size" -gt "$budget" ]; then
    echo "$name: $size bytes, over its budget of $budget"
    status=1
  else
    echo "$name: $size bytes (budget $budget)"
  fi
done < scripts/binary-size-budget.txt
exit "$status"
