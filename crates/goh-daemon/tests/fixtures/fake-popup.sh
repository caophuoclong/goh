#!/usr/bin/env bash
# Stands in for goh-popup in adapter tests: logs every frame it receives and answers each `show`
# with the decision in $FAKE_DISPLAY (overlay|inline), like the real popup's decision line.
log=${FAKE_LOG:?FAKE_LOG must be set}
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case $line in
    *'"type":"show"'*)
      [[ -n ${FAKE_DELAY:-} ]] && sleep "$FAKE_DELAY"
      printf '{"type":"decision","display":"%s","reason":"fake"}\n' "${FAKE_DISPLAY:-overlay}" ;;
    *'"type":"quit"'*) exit 0 ;;
  esac
done
