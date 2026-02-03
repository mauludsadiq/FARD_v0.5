set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G44_PKG_IMPORT_REQUIRES_LOCK MISSING_fardrun"; exit 1; }

P="tests/fixtures/imports/app_pkg/main.fard"
test -f "$P" || { echo "FAIL G44_PKG_IMPORT_REQUIRES_LOCK MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g44.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

set +e
"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>"$OUT/stderr.txt"
rc=$?
set -e

test "$rc" -ne 0 || { echo "FAIL G44_PKG_IMPORT_REQUIRES_LOCK EXPECTED_FAIL_NO_LOCK"; exit 1; }

test -f "$OUT/error.json" || { echo "FAIL G44_PKG_IMPORT_REQUIRES_LOCK MISSING_error.json"; exit 1; }

rg -n 'LOCK_|PACKAGE_|REGISTRY_|IMPORT_' "$OUT/stderr.txt" >/dev/null || { echo "FAIL G44_PKG_IMPORT_REQUIRES_LOCK STDERR_MISSING_TAG"; exit 1; }

echo "PASS G44_PKG_IMPORT_REQUIRES_LOCK"
