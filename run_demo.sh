#!/usr/bin/env bash
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_RUN="$ROOT/target/debug/fardrun"
BIN_FARD="$ROOT/target/debug/fard"
PASS=0; FAIL=0

run_fardrun() {
  local name="$1" prog="$2"
  local out="/tmp/fard_demo_$$_${name//[^a-zA-Z0-9]/_}"
  mkdir -p "$out"
  "$BIN_RUN" run --program "$ROOT/$prog" --out "$out" 2>/dev/null
  if [ -f "$out/error.json" ]; then
    echo "FAIL  $name"; FAIL=$((FAIL+1))
  else
    echo "PASS  $name"; PASS=$((PASS+1))
  fi
  rm -rf "$out"
}

run_fard() {
  local name="$1" prog="$2"
  local result
  result=$("$BIN_FARD" run "$ROOT/$prog" 2>&1 | head -1)
  if echo "$result" | grep -qi error; then
    echo "FAIL  $name"; FAIL=$((FAIL+1))
  else
    echo "PASS  $name"; PASS=$((PASS+1))
  fi
}

echo "FARD v0.5 - Example Suite"
echo "=========================="
run_fard    "mathematical_proof_system   fardlang" examples/mathematical_proof_system/main.fard
run_fard    "collapse_chess_z            fardlang" examples/collapse_chess_z/main.fard
run_fard    "collapse_structural_numbers fardlang" examples/collapse_structural_numbers/main.fard
run_fardrun "qasim_safety                fardrun"  examples/qasim_safety/qasim_safety.fard
run_fardrun "collapse_periodic_table     fardrun"  examples/collapse_periodic_table/collapse_periodic_table.fard
run_fardrun "collapse_coin/canonicalize  fardrun"  examples/collapse_coin/canonicalize_tx.fard
run_fardrun "collapse_coin/rewards       fardrun"  examples/collapse_coin/compute_rewards.fard
run_fardrun "collapse_coin/settle        fardrun"  examples/collapse_coin/settle.fard
run_fardrun "collapse_coin/verify_jwt    fardrun"  examples/collapse_coin/verify_jwt.fard
run_fardrun "collapse_stack/apply_delta  fardrun"  examples/collapse_stack/apply_delta.fard
run_fardrun "collapse_stack/verify_z     fardrun"  examples/collapse_stack/verify_zstate.fard
run_fardrun "sembit/verify               fardrun"  examples/sembit/sembit_verify.fard
run_fardrun "kitchen_sink                fardrun"  examples/kitchen_sink_v0_5.fard
echo "=========================="
echo "  $PASS passed  $FAIL failed"
[ $FAIL -eq 0 ]
