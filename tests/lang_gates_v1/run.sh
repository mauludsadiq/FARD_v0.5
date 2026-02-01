FARD_BIN="${FARD_BIN:-target/debug/fardrun}"
LOCK="${LOCK:-fard.lock.json}"
OUT_ROOT="tests/lang_gates_v1/out"

pass_ct=0
fail_ct=0

ok()   { printf 'PASS %s\n' "$1"; pass_ct=$((pass_ct+1)); }
fail() { printf 'FAIL %s\n' "$1"; fail_ct=$((fail_ct+1)); }

has() { [ -f "$1" ]; }

run_prog() {
  prog="$1"
  out="$2"
  rm -rf "$out"
  mkdir -p "$out"
  if [ -f "$LOCK" ]; then
    "$FARD_BIN" run "$prog" --out "$out" --lock "$LOCK" >/dev/null 2>"$out/stderr.txt"
  else
    "$FARD_BIN" run "$prog" --out "$out" >/dev/null 2>"$out/stderr.txt"
  fi
}

run_prog_expect_fail() {
  prog="$1"
  out="$2"
  rm -rf "$out"
  mkdir -p "$out"
  if [ -f "$LOCK" ]; then
    "$FARD_BIN" run "$prog" --out "$out" --lock "$LOCK" >/dev/null 2>"$out/stderr.txt"
  else
    "$FARD_BIN" run "$prog" --out "$out" >/dev/null 2>"$out/stderr.txt"
  fi
  code=$?
  [ $code -ne 0 ]
}

ndjson_lint() {
  f="$1"
  has "$f" || return 1
  awk 'NF{print}' "$f" | while IFS= read -r line; do
    printf '%s\n' "$line" | jq -e . >/dev/null 2>/dev/null || exit 1
  done
}

grepq() { rg -n "$1" "$2" >/dev/null 2>/dev/null; }

gate_G0_distribution_identity() {
  v="$("$FARD_BIN" --version 2>/dev/null)"
  printf '%s\n' "$v" | rg -n 'fard_runtime_version=' >/dev/null 2>/dev/null || return 1
  printf '%s\n' "$v" | rg -n 'trace_format_version=' >/dev/null 2>/dev/null || return 1
  printf '%s\n' "$v" | rg -n 'stdlib_root_cid=' >/dev/null 2>/dev/null || return 1
}

gate_G1_determinism_trace_bytes() {
  a="$OUT_ROOT/g1a"
  b="$OUT_ROOT/g1b"
  run_prog "tests/gate/programs/g07_determinism_twice_same_trace.fard" "$a" || return 1
  run_prog "tests/gate/programs/g07_determinism_twice_same_trace.fard" "$b" || return 1
  has "$a/trace.ndjson" || return 1
  has "$b/trace.ndjson" || return 1
  cmp "$a/trace.ndjson" "$b/trace.ndjson" >/dev/null 2>/dev/null || return 1
  if has "$a/result.json" && has "$b/result.json"; then
    cmp "$a/result.json" "$b/result.json" >/dev/null 2>/dev/null || return 1
  fi
}

gate_G2_trace_parseable_ndjson() {
  out="$OUT_ROOT/g2"
  run_prog "tests/gate/programs/g03_trace_parseable.fard" "$out" || return 1
  has "$out/trace.ndjson" || return 1
  ndjson_lint "$out/trace.ndjson" || return 1
}

gate_G3_lock_mismatch_diagnostics() {
  out="$OUT_ROOT/g3b"
  rm -rf "$out"
  mkdir -p "$out"
  "$FARD_BIN" run "tests/gate/programs/g09_lock_mismatch/main.fard" \
    --out "$out" \
    --lock "tests/gate/programs/g09_lock_mismatch/fard.lock.json" \
    >/dev/null 2>"$out/stderr.txt"
  code=$?
  [ $code -ne 0 ] || return 1
  grepq 'LOCK_MISMATCH' "$out/stderr.txt" || return 1
}

gate_G4_import_cycle_diagnostics() {
  out="$OUT_ROOT/g4"
  run_prog_expect_fail "tests/gate/programs/g08_import_cycle/main.fard" "$out" || return 1
  grepq 'IMPORT_CYCLE' "$out/stderr.txt" || return 1
}

gate_G5_core_value_model_eq() {
  out="$OUT_ROOT/g5"
  run_prog "tests/lang_gates_v1/programs/g5_json_eq.fard" "$out" || return 1
  has "$out/result.json" || return 1
  jq -e '.result == true' "$out/result.json" >/dev/null 2>/dev/null || return 1
}

gate_G7_sort_stability_contract() {
  out="$OUT_ROOT/g7"
  run_prog "tests/lang_gates_v1/programs/g7_sort_stable.fard" "$out" || return 1
  jq -e '.result == [{"k":1,"id":"b"},{"k":1,"id":"c"},{"k":2,"id":"a"}]' "$out/result.json" >/dev/null 2>/dev/null || return 1
}

gate_G10_grow_unfold_order() {
  out="$OUT_ROOT/g10"
  run_prog "tests/lang_gates_v1/programs/g10_grow_order.fard" "$out" || return 1
  has "$out/trace.ndjson" || return 1
  rg -n '"t":"grow_node"' "$out/trace.ndjson" >/dev/null 2>/dev/null || return 1
  rg -n '"t":"grow_node"' "$out/trace.ndjson" > "$out/nodes.txt"
  cmp "$out/nodes.txt" "tests/lang_gates_v1/expected/g10_nodes.txt" >/dev/null 2>/dev/null || return 1
}

gate_G11_result_shortcircuit() {
  out="$OUT_ROOT/g11"
  run_prog "tests/lang_gates_v1/programs/g11_result_shortcircuit.fard" "$out" || return 1
  has "$out/trace.ndjson" || return 1
  rg -n 'NEVER' "$out/trace.ndjson" >/dev/null 2>/dev/null && return 1
  return 0
}

gate_G12_artifact_boundary_cid_commit() {
  out="$OUT_ROOT/g12"
  run_prog "tests/lang_gates_v1/programs/g12_artifacts.fard" "$out" || return 1
  has "$out/artifacts/output.bin" || return 1
  has "$out/artifacts/output.bin.cid" || return 1
  grepq '"t":"artifact_in"' "$out/trace.ndjson" || return 1
  grepq '"t":"artifact_out"' "$out/trace.ndjson" || return 1
}

gate_G13_error_model_structured() {
  out="$OUT_ROOT/g13"
  run_prog_expect_fail "tests/lang_gates_v1/programs/g13_error.fard" "$out" || return 1
  has "$out/error.json" || return 1
  has "$out/trace.ndjson" || return 1
  grepq '"t":"error"' "$out/trace.ndjson" || return 1
  grepq 'ERROR_' "$out/stderr.txt" || return 1
}

gate_G14_perf_floor() {
  out="$OUT_ROOT/g14"
  run_prog "tests/lang_gates_v1/programs/g14_perf_floor.fard" "$out" || return 1
  has "$out/trace.ndjson" || return 1
}


gate_G6_stdlib_min_surface() {
  out="$OUT_ROOT/g6"
  run_prog "tests/lang_gates_v1/programs/g06_std_min_surface.fard" "$out" || return 1
  has "$out/result.json" || return 1
  jq -e '.result == "OK"' "$out/result.json" >/dev/null 2>/dev/null || return 1
}

gate_G8_dedupe_contract() {
  out="$OUT_ROOT/g8"
  run_prog "tests/lang_gates_v1/programs/g08_dedupe_sorted.fard" "$out" || return 1
  has "$out/result.json" || return 1
  jq -e '.result == [1,2,3,4]' "$out/result.json" >/dev/null 2>/dev/null || return 1
}

gate_G9_hist_contract() {
  out="$OUT_ROOT/g9"
  run_prog "tests/lang_gates_v1/programs/g09_hist_int.fard" "$out" || return 1
  has "$out/result.json" || return 1
  jq -e '.result == [{"count":2,"v":1},{"count":3,"v":2},{"count":1,"v":3}]' "$out/result.json" >/dev/null 2>/dev/null || return 1
}






run_one() {
  name="$1"
  fn="$2"
  "$fn"
  if [ $? -eq 0 ]; then ok "$name"; else fail "$name"; fi
}

mkdir -p "$OUT_ROOT"

run_one "G0_distribution_identity" gate_G0_distribution_identity
run_one "G1_determinism_trace_bytes" gate_G1_determinism_trace_bytes
run_one "G2_trace_parseable_ndjson" gate_G2_trace_parseable_ndjson
run_one "G3_lock_mismatch_diagnostics" gate_G3_lock_mismatch_diagnostics
run_one "G4_import_cycle_diagnostics" gate_G4_import_cycle_diagnostics

run_one "G5_core_value_model_eq" gate_G5_core_value_model_eq
run_one "G6_stdlib_min_surface" gate_G6_stdlib_min_surface
run_one "G8_dedupe_contract" gate_G8_dedupe_contract
run_one "G9_hist_contract" gate_G9_hist_contract
run_one "G7_sort_stability_contract" gate_G7_sort_stability_contract
run_one "G10_grow_unfold_order" gate_G10_grow_unfold_order
run_one "G11_result_shortcircuit" gate_G11_result_shortcircuit
run_one "G12_artifact_boundary_cid_commit" gate_G12_artifact_boundary_cid_commit
run_one "G13_error_model_structured" gate_G13_error_model_structured
run_one "G14_perf_floor" gate_G14_perf_floor

printf 'GATE_V1_SUMMARY passed=%s failed=%s\n' "$pass_ct" "$fail_ct"
if [ "$fail_ct" -eq 0 ]; then exit 0; fi
exit 1
