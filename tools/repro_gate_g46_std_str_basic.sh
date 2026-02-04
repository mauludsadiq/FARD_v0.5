#!/bin/sh
set -eu

BIN="./target/debug/fardrun"
test -x "$BIN" || { echo "FAIL G46_STD_STR_BASIC NO_BIN"; exit 1; }

OUT="$(mktemp -d "/tmp/fard_g46.XXXXXX")"
P="tests/fixtures/stdlib/strings_basic.fard"

"$BIN" run --program "$P" --out "$OUT" >/dev/null 2>/dev/null || true

test -f "$OUT/result.json" || { echo "FAIL G46_STD_STR_BASIC NO_RESULT"; exit 1; }

R="$(jq -r '.result' "$OUT/result.json" 2>/dev/null || true)"
test "$R" = "abcd" || { echo "FAIL G46_STD_STR_BASIC BAD_RESULT got=$R"; exit 1; }

echo "PASS G46_STD_STR_BASIC"
