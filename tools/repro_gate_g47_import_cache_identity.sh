set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G47_IMPORT_CACHE_IDENTITY MISSING_fardrun"; exit 1; }

P="tests/fixtures/imports/app_rel/main_twice.fard"
test -f "$P" || { echo "FAIL G47_IMPORT_CACHE_IDENTITY MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g47.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>&1 || { echo "FAIL G47_IMPORT_CACHE_IDENTITY RUN_FAILED"; exit 1; }

test -f "$OUT/result.json" || { echo "FAIL G47_IMPORT_CACHE_IDENTITY MISSING_result.json"; exit 1; }

echo "PASS G47_IMPORT_CACHE_IDENTITY"
