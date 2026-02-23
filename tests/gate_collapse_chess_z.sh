#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/fard"
PROG="$ROOT/examples/collapse_chess_z/main.fard"

if [ ! -x "$BIN" ]; then
  echo "ERROR: missing $BIN (build first: cargo build -p fard)" >&2
  exit 1
fi

run_case() {
  local stack_arg="${1:-}"
  local out
  if [ -n "$stack_arg" ]; then
    out="$("$BIN" run "$PROG" "$stack_arg" 2>/dev/null)"
  else
    out="$("$BIN" run "$PROG" 2>/dev/null)"
  fi
  printf "%s" "$out"
}

jq_fail() {
  local label="$1"
  local out="$2"
  local filter="$3"
  echo "---- FAIL [$label] jq assertion ----" >&2
  echo "FILTER:" >&2
  echo "$filter" >&2
  echo "" >&2
  echo "OUTPUT:" >&2
  echo "$out" | jq . >&2 || true
  echo "------------------------------------" >&2
  exit 1
}

# -------------------------
# CASE A: normal stack should pass and match exact expected metrics
# -------------------------
OUT_OK="$(run_case "" "ok_default")"

# Must be valid JSON
echo "$OUT_OK" | jq -e . >/dev/null || jq_fail "ok_default:invalid_json" "$OUT_OK" '.'

# Must be a top-level map with required keys
FILTER_SHAPE='
  .t=="map"
  and ( [(.v[]|.[0])] | index("total_games") != null )
  and ( [(.v[]|.[0])] | index("n_z_edges") != null )
  and ( [(.v[]|.[0])] | index("h_opening_mb") != null )
  and ( [(.v[]|.[0])] | index("h_z_mb") != null )
  and ( [(.v[]|.[0])] | index("h_endgame_mb") != null )
  and ( [(.v[]|.[0])] | index("oz_gap_mb") != null )
  and ( [(.v[]|.[0])] | index("ze_gap_mb") != null )
  and ( [(.v[]|.[0])] | index("factorization_valid") != null )
  and ( [(.v[]|.[0])] | index("row_sum_errors") != null )
  and ( [(.v[]|.[0])] | index("isentropic") != null )
'
echo "$OUT_OK" | jq -e "$FILTER_SHAPE" >/dev/null || jq_fail "ok_default:shape" "$OUT_OK" "$FILTER_SHAPE"

# Helper fns for canonical-map access
FILTER_EXPECTED='
  def getk($k):
    (.v[] | select(.[0]==$k) | .[1]);

  def is_int($v):  ($v.t=="int");
  def is_bool($v): ($v.t=="bool");
  def is_true($v): ($v.t=="bool" and $v.v==true);

  # Exact invariants (from the example run)
  (getk("total_games")      | is_int(.) and .v==325977) and
  (getk("n_z_edges")        | is_int(.) and .v==22) and
  (getk("h_opening_mb")     | is_int(.) and .v==3986) and
  (getk("h_z_mb")           | is_int(.) and .v==3886) and
  (getk("h_endgame_mb")     | is_int(.) and .v==3854) and
  (getk("oz_gap_mb")        | is_int(.) and .v==100) and
  (getk("ze_gap_mb")        | is_int(.) and .v==32) and
  (getk("row_sum_errors")   | is_int(.) and .v==0) and
  (getk("factorization_valid") | is_true(.)) and
  (getk("isentropic")          | is_true(.))
'
echo "$OUT_OK" | jq -e "$FILTER_EXPECTED" >/dev/null || jq_fail "ok_default:expected" "$OUT_OK" "$FILTER_EXPECTED"

# Optional: redundancy check — bool must agree with row_sum_errors==0
FILTER_REDUNDANCY='
  def getk($k):
    (.v[] | select(.[0]==$k) | .[1]);

  (getk("row_sum_errors").t=="int") and
  (getk("factorization_valid").t=="bool") and
  ((getk("row_sum_errors").v==0) == (getk("factorization_valid").v==true))
'
echo "$OUT_OK" | jq -e "$FILTER_REDUNDANCY" >/dev/null || jq_fail "ok_default:redundancy_factorization" "$OUT_OK" "$FILTER_REDUNDANCY"

echo "PASS gate_collapse_chess_z"
