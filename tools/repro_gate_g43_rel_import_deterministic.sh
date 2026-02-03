set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G43_REL_IMPORT_DETERMINISTIC MISSING_fardrun"; exit 1; }

P="tests/fixtures/imports/app_rel/main.fard"
test -f "$P" || { echo "FAIL G43_REL_IMPORT_DETERMINISTIC MISSING_FIXTURE $P"; exit 1; }

O1="/tmp/fard_g43_o1.$$.$(date +%s)"
O2="/tmp/fard_g43_o2.$$.$(date +%s)"
rm -rf "$O1" "$O2"
mkdir -p "$O1" "$O2"

"$FARDRUN" run --program "$P" --out "$O1" >/dev/null 2>&1
"$FARDRUN" run --program "$P" --out "$O2" >/dev/null 2>&1

T1="$(shasum -a 256 "$O1/trace.ndjson" | awk '{print $1}')"
T2="$(shasum -a 256 "$O2/trace.ndjson" | awk '{print $1}')"
test "$T1" = "$T2" || { echo "FAIL G43_REL_IMPORT_DETERMINISTIC TRACE_HASH_MISMATCH $T1 $T2"; exit 1; }

R1="$(shasum -a 256 "$O1/result.json" | awk '{print $1}')"
R2="$(shasum -a 256 "$O2/result.json" | awk '{print $1}')"
test "$R1" = "$R2" || { echo "FAIL G43_REL_IMPORT_DETERMINISTIC RESULT_HASH_MISMATCH $R1 $R2"; exit 1; }

rm -rf "$O1" "$O2"

echo "PASS G43_REL_IMPORT_DETERMINISTIC"
