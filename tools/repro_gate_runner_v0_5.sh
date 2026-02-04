#!/usr/bin/env bash
set -euo pipefail

G="RUNNER_V0_5"

fail(){ printf 'FAIL %s %s\n' "$G" "${1:-UNKNOWN}"; exit 1; }
pass(){ printf 'PASS %s\n' "$G"; exit 0; }

cd "$(git rev-parse --show-toplevel)" || fail "NOT_A_GIT_REPO"

GATES=(
  tools/repro_gate_g28_spec_contract.sh
  tools/repro_gate_g29_spec_min_content.sh
  tools/repro_gate_g30_spec_ebnf_present.sh
  tools/repro_gate_g31_no_python_in_tools.sh
  tools/repro_gate_g32_spec_impl_conformance.sh
  tools/repro_gate_g33_spec_drift_lock.sh
  tools/repro_gate_g34_fmt_exists.sh
  tools/repro_gate_g35_fmt_idempotent_bytes.sh
  tools/repro_gate_g36_fmt_ast_invariant.sh
  tools/repro_gate_g37_fmt_run_equivalence.sh
  tools/repro_gate_g38_error_span_present.sh
  tools/repro_gate_g39_error_span_correct.sh
  tools/repro_gate_g40_stderr_renders_error_json.sh
  tools/repro_gate_g41_error_codes_stable.sh
  tools/repro_gate_g42_rel_import_exists.sh
  tools/repro_gate_g43_rel_import_deterministic.sh
  tools/repro_gate_g44_pkg_import_requires_lock.sh
  tools/repro_gate_g45_pkg_import_with_lock_registry.sh
  tools/repro_gate_g46_module_graph_includes_nonstd.sh
  tools/repro_gate_g47_import_cache_identity.sh
)

for g in "${GATES[@]}"; do
  test -x "$g" || fail "MISSING_OR_NOT_EXECUTABLE_${g}"
  bash "$g" || fail "GATE_FAILED_${g}"
done

pass

sh tools/repro_gate_g46_std_str_basic.sh
sh tools/repro_gate_g47_std_str_len.sh
sh tools/repro_gate_g48_std_map_ops.sh
sh tools/repro_gate_g49_std_json_roundtrip.sh
