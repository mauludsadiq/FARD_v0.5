set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_fardrun"; exit 1; }

P="tests/fixtures/diag/g38_parse_error_span.fard"
test -f "$P" || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g38.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

set +e
"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>"$OUT/stderr.txt"
rc=$?
set -e

test "$rc" -ne 0 || { echo "FAIL G38_ERROR_SPAN_PRESENT EXPECTED_FAIL"; exit 1; }

test -f "$OUT/error.json" || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_error.json"; exit 1; }

rg -n '"code"[[:space:]]*:' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_code"; exit 1; }
rg -n '"message"[[:space:]]*:' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_message"; exit 1; }

rg -n '"span"[[:space:]]*:' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_span"; exit 1; }
rg -n '"file"[[:space:]]*:[[:space:]]*".*"' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_span_file"; exit 1; }
rg -n '"byte_start"[[:space:]]*:[[:space:]]*[0-9]+' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_byte_start"; exit 1; }
rg -n '"byte_end"[[:space:]]*:[[:space:]]*[0-9]+' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_byte_end"; exit 1; }
rg -n '"line"[[:space:]]*:[[:space:]]*[0-9]+' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_line"; exit 1; }
rg -n '"col"[[:space:]]*:[[:space:]]*[0-9]+' "$OUT/error.json" >/dev/null || { echo "FAIL G38_ERROR_SPAN_PRESENT MISSING_col"; exit 1; }

echo "PASS G38_ERROR_SPAN_PRESENT"
