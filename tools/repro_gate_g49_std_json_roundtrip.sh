#!/bin/sh
set -eu

BIN="./target/debug/fardrun"
test -x "$BIN" || { echo "FAIL G49_STD_JSON_ROUNDTRIP NO_BIN"; exit 1; }

OUT="$(mktemp -d "/tmp/fard_g49.XXXXXX")"
P="tests/fixtures/stdlib/json_roundtrip.fard"

"$BIN" run --program "$P" --out "$OUT" >/dev/null 2>/dev/null || true

test -f "$OUT/result.json" || { echo "FAIL G49_STD_JSON_ROUNDTRIP NO_RESULT"; exit 1; }

# result is a record; compare canonical JSON of .result
jq -cS '.result' "$OUT/result.json" >/tmp/fard_g49.result.json 2>/dev/null || true
test -s /tmp/fard_g49.result.json || { echo "FAIL G49_STD_JSON_ROUNDTRIP BAD_JSON"; exit 1; }

EXP='{"a":1,"b":[1,2,3],"c":"x"}'
GOT="$(cat /tmp/fard_g49.result.json)"
rm -f /tmp/fard_g49.result.json

test "$GOT" = "$EXP" || { echo "FAIL G49_STD_JSON_ROUNDTRIP MISMATCH got=$GOT"; exit 1; }

echo "PASS G49_STD_JSON_ROUNDTRIP"
