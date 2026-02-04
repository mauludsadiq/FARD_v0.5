set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G39_ERROR_SPAN_CORRECT MISSING_fardrun"; exit 1; }

P="tests/fixtures/diag/g39_span_exact.fard"
test -f "$P" || { echo "FAIL G39_ERROR_SPAN_CORRECT MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g39.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

set +e
"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>"$OUT/stderr.txt"
rc=$?
set -e

test "$rc" -ne 0 || { echo "FAIL G39_ERROR_SPAN_CORRECT EXPECTED_FAIL"; exit 1; }
test -f "$OUT/error.json" || { echo "FAIL G39_ERROR_SPAN_CORRECT MISSING_error.json"; exit 1; }

BS="$(jq -r '.span.byte_start' "$OUT/error.json" 2>/dev/null || true)"
BE="$(jq -r '.span.byte_end'   "$OUT/error.json" 2>/dev/null || true)"

test -n "${BS:-}" || { echo "FAIL G39_ERROR_SPAN_CORRECT NO_byte_start"; exit 1; }
test -n "${BE:-}" || { echo "FAIL G39_ERROR_SPAN_CORRECT NO_byte_end"; exit 1; }

test "$BE" -gt "$BS" || { echo "FAIL G39_ERROR_SPAN_CORRECT NON_POSITIVE_SPAN $BS $BE"; exit 1; }

BYTES="$(wc -c < "$P" | tr -d ' ')"
test "$BS" -ge 0 || { echo "FAIL G39_ERROR_SPAN_CORRECT NEG_byte_start $BS"; exit 1; }
test "$BE" -le "$BYTES" || { echo "FAIL G39_ERROR_SPAN_CORRECT OUT_OF_RANGE $BS $BE FILE_BYTES $BYTES"; exit 1; }

OFF_EXPECT="$(rg -n '^#EXPECTED_SPAN_BYTE_START=' "$P" | head -n 1 | cut -d= -f2- | tr -d ' \t\r\n' || true)"
test -n "${OFF_EXPECT:-}" || { echo "FAIL G39_ERROR_SPAN_CORRECT MISSING_EXPECTED_MARKER"; exit 1; }

test "$BS" = "$OFF_EXPECT" || { echo "FAIL G39_ERROR_SPAN_CORRECT START_MISMATCH got=$BS exp=$OFF_EXPECT"; exit 1; }

echo "PASS G39_ERROR_SPAN_CORRECT"
