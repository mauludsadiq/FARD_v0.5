#!/bin/sh
set -eu

BIN="./target/debug/fardrun"
test -x "$BIN" || { echo "FAIL G48_STD_MAP_OPS NO_BIN"; exit 1; }

OUT="$(mktemp -d "/tmp/fard_g48.XXXXXX")"
P="tests/fixtures/stdlib/map_ops.fard"

"$BIN" run --program "$P" --out "$OUT" >/dev/null 2>/dev/null || true

test -f "$OUT/result.json" || { echo "FAIL G48_STD_MAP_OPS NO_RESULT"; exit 1; }

R="$(jq -r '.result' "$OUT/result.json" 2>/dev/null || true)"
test "$R" = "3" || { echo "FAIL G48_STD_MAP_OPS BAD_RESULT got=$R"; exit 1; }

echo "PASS G48_STD_MAP_OPS"
