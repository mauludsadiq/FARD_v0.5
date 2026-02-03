set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G45_PKG_IMPORT_WITH_LOCK_REGISTRY MISSING_fardrun"; exit 1; }

APP="tests/fixtures/imports/app_pkg"
P="$APP/main.fard"
LOCK="$APP/fard.lock.json"
REG="$APP/registry"

test -f "$P" || { echo "FAIL G45_PKG_IMPORT_WITH_LOCK_REGISTRY MISSING_PROGRAM $P"; exit 1; }
test -f "$LOCK" || { echo "FAIL G45_PKG_IMPORT_WITH_LOCK_REGISTRY MISSING_LOCK $LOCK"; exit 1; }
test -d "$REG" || { echo "FAIL G45_PKG_IMPORT_WITH_LOCK_REGISTRY MISSING_REGISTRY $REG"; exit 1; }

OUT="/tmp/fard_g45.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

"$FARDRUN" run --program "$P" --out "$OUT" --lock "$LOCK" --registry "$REG" >/dev/null 2>&1 || {
  echo "FAIL G45_PKG_IMPORT_WITH_LOCK_REGISTRY RUN_FAILED"
  exit 1
}

test -f "$OUT/result.json" || { echo "FAIL G45_PKG_IMPORT_WITH_LOCK_REGISTRY MISSING_result.json"; exit 1; }
test -f "$OUT/trace.ndjson" || { echo "FAIL G45_PKG_IMPORT_WITH_LOCK_REGISTRY MISSING_trace.ndjson"; exit 1; }

echo "PASS G45_PKG_IMPORT_WITH_LOCK_REGISTRY"
