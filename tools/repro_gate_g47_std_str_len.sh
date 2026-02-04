#!/bin/sh
set -eu

BIN="./target/debug/fardrun"
test -x "$BIN" || { echo "FAIL G47_STD_STR_LEN NO_BIN"; exit 1; }

OUT="$(mktemp -d "/tmp/fard_g47.XXXXXX")"
P="tests/fixtures/stdlib/strings_len.fard"

"$BIN" run --program "$P" --out "$OUT" >/dev/null 2>/dev/null || true

test -f "$OUT/result.json" || { echo "FAIL G47_STD_STR_LEN NO_RESULT"; exit 1; }

R="$(jq -r '.result' "$OUT/result.json" 2>/dev/null || true)"
test "$R" = "5" || { echo "FAIL G47_STD_STR_LEN BAD_RESULT got=$R"; exit 1; }

echo "PASS G47_STD_STR_LEN"
