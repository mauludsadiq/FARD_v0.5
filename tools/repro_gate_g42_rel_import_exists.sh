set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G42_REL_IMPORT_EXISTS MISSING_fardrun"; exit 1; }

P="tests/fixtures/imports/app_rel/main.fard"
test -f "$P" || { echo "FAIL G42_REL_IMPORT_EXISTS MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g42.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

set +e
"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>"$OUT/stderr.txt"
rc=$?
set -e

test "$rc" -eq 0 || { echo "FAIL G42_REL_IMPORT_EXISTS RUN_FAILED"; test -f "$OUT/stderr.txt" && tail -n 30 "$OUT/stderr.txt" || true; exit 1; }

test -f "$OUT/trace.ndjson" || { echo "FAIL G42_REL_IMPORT_EXISTS MISSING_trace"; exit 1; }
test -f "$OUT/result.json" || { echo "FAIL G42_REL_IMPORT_EXISTS MISSING_result"; exit 1; }

rg -n '"result"' "$OUT/result.json" >/dev/null || true

echo "PASS G42_REL_IMPORT_EXISTS"
