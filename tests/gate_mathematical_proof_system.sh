#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/fard"
PROG="$ROOT/examples/mathematical_proof_system/main.fard"

if [ ! -x "$BIN" ]; then
  echo "ERROR: missing $BIN (build first: cargo build -p fard)" >&2
  exit 1
fi

run_case() {
  local stack_arg="$1"
  local label="$2"
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
# CASE A: big/default stack should fully verify
# -------------------------
OUT_OK="$(run_case "--stack-mb=256" "ok_256")"

# Must be a map with keys all_verified/resource_errors/theorems
FILTER_SHAPE='
  .t=="map"
  and ( [(.v[]|.[0])] | index("all_verified") != null )
  and ( [(.v[]|.[0])] | index("resource_errors") != null )
  and ( [(.v[]|.[0])] | index("theorems") != null )
'
echo "$OUT_OK" | jq -e "$FILTER_SHAPE" >/dev/null || jq_fail "ok_256:shape" "$OUT_OK" "$FILTER_SHAPE"

# all_verified == true
FILTER_ALL_VERIFIED='
  (.v[] | select(.[0]=="all_verified") | .[1].t=="bool" and .[1].v==true)
'
echo "$OUT_OK" | jq -e "$FILTER_ALL_VERIFIED" >/dev/null || jq_fail "ok_256:all_verified" "$OUT_OK" "$FILTER_ALL_VERIFIED"

# resource_errors == 0
FILTER_RESOURCE_ZERO='
  (.v[] | select(.[0]=="resource_errors") | .[1].t=="int" and .[1].v==0)
'
echo "$OUT_OK" | jq -e "$FILTER_RESOURCE_ZERO" >/dev/null || jq_fail "ok_256:resource_errors==0" "$OUT_OK" "$FILTER_RESOURCE_ZERO"

# every theorem has error == unit AND verified == true
FILTER_THEOREMS_ALL_PASS='
  def getk($k):
    (.v[] | select(.[0]==$k) | .[1]);

  def map_get($m; $k):
    ($m.v[] | select(.[0]==$k) | .[1]);

  def is_unit($v): ($v.t=="unit");
  def is_true($v): ($v.t=="bool" and $v.v==true);

  (getk("theorems") | .t=="list") and
  (
    (getk("theorems").v | length) == 5
    and
    ( getk("theorems").v
      | all(
          (.t=="map")
          and is_unit(map_get(.;"error"))
          and is_true(map_get(.;"verified"))
        )
    )
  )
'
echo "$OUT_OK" | jq -e "$FILTER_THEOREMS_ALL_PASS" >/dev/null || jq_fail "ok_256:theorems_all_pass" "$OUT_OK" "$FILTER_THEOREMS_ALL_PASS"

# -------------------------
# CASE B: small stack should report resource failure (at least one)
# -------------------------
OUT_SMALL="$(run_case "--stack-mb=64" "small_64")"

# resource_errors > 0
FILTER_RESOURCE_POS='
  (.v[] | select(.[0]=="resource_errors") | .[1].t=="int" and .[1].v > 0)
'
echo "$OUT_SMALL" | jq -e "$FILTER_RESOURCE_POS" >/dev/null || jq_fail "small_64:resource_errors>0" "$OUT_SMALL" "$FILTER_RESOURCE_POS"

# at least one theorem has error != unit (and it should be depth)
FILTER_HAS_DEPTH_ERR='
  def getk($k):
    (.v[] | select(.[0]==$k) | .[1]);

  def map_get($m; $k):
    ($m.v[] | select(.[0]==$k) | .[1]);

  def is_unit($v): ($v.t=="unit");

  (getk("theorems") | .t=="list") and
  (
    getk("theorems").v
    | any(
        (map_get(.;"error") | is_unit(.) | not)
        and
        (map_get(.;"error").t=="text")
        and
        (map_get(.;"error").v | contains("ERROR_EVAL_DEPTH"))
      )
  )
'
echo "$OUT_SMALL" | jq -e "$FILTER_HAS_DEPTH_ERR" >/dev/null || jq_fail "small_64:has_depth_error" "$OUT_SMALL" "$FILTER_HAS_DEPTH_ERR"

echo "PASS gate_mathematical_proof_system"
