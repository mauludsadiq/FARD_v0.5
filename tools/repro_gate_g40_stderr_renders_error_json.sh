set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G40_STDERR_RENDERS_ERROR_JSON MISSING_fardrun"; exit 1; }

P="tests/fixtures/diag/g40_err_render.fard"
test -f "$P" || { echo "FAIL G40_STDERR_RENDERS_ERROR_JSON MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g40.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

set +e
"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>"$OUT/stderr.txt"
rc=$?
set -e

test "$rc" -ne 0 || { echo "FAIL G40_STDERR_RENDERS_ERROR_JSON EXPECTED_FAIL"; exit 1; }

test -f "$OUT/error.json" || { echo "FAIL G40_STDERR_RENDERS_ERROR_JSON MISSING_error.json"; exit 1; }
test -f "$OUT/stderr.txt" || { echo "FAIL G40_STDERR_RENDERS_ERROR_JSON MISSING_stderr.txt"; exit 1; }

CODE="$(rg -n -o '"code"[[:space:]]*:[[:space:]]*"[^"]+"' "$OUT/error.json" | head -n 1 | sed -E 's/.*"code"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)"
test -n "${CODE:-}" || { echo "FAIL G40_STDERR_RENDERS_ERROR_JSON NO_CODE_IN_error.json"; exit 1; }

rg -n "$CODE" "$OUT/stderr.txt" >/dev/null || { echo "FAIL G40_STDERR_RENDERS_ERROR_JSON STDERR_MISSING_CODE $CODE"; exit 1; }

echo "PASS G40_STDERR_RENDERS_ERROR_JSON"
