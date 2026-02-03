set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G41_ERROR_CODES_STABLE MISSING_fardrun"; exit 1; }

P="tests/fixtures/diag/g41_err_code.fard"
test -f "$P" || { echo "FAIL G41_ERROR_CODES_STABLE MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g41.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

set +e
"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>"$OUT/stderr.txt"
rc=$?
set -e

test "$rc" -ne 0 || { echo "FAIL G41_ERROR_CODES_STABLE EXPECTED_FAIL"; exit 1; }
test -f "$OUT/error.json" || { echo "FAIL G41_ERROR_CODES_STABLE MISSING_error.json"; exit 1; }
test -f "$OUT/trace.ndjson" || { echo "FAIL G41_ERROR_CODES_STABLE MISSING_trace.ndjson"; exit 1; }

CODE="$(rg -n -o '"code"[[:space:]]*:[[:space:]]*"[^"]+"' "$OUT/error.json" | head -n 1 | sed -E 's/.*"code"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)"
test -n "${CODE:-}" || { echo "FAIL G41_ERROR_CODES_STABLE NO_CODE"; exit 1; }

rg -n "$CODE" "$OUT/stderr.txt" >/dev/null || { echo "FAIL G41_ERROR_CODES_STABLE STDERR_MISSING_CODE $CODE"; exit 1; }
rg -n "$CODE" "$OUT/trace.ndjson" >/dev/null || { echo "FAIL G41_ERROR_CODES_STABLE TRACE_MISSING_CODE $CODE"; exit 1; }

echo "PASS G41_ERROR_CODES_STABLE"
