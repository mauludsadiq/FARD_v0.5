#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)" || exit 1
SPEC="SPEC.md"
fail(){ echo "FAIL G30_SPEC_EBNF_PRESENT $1"; exit 1; }

test -f "$SPEC" || fail "MISSING_SPEC_MD"

GRAMMAR="$(
  awk '
    BEGIN{inside=0}
    $0 ~ "^[#]{1,6}[[:space:]]+Grammar[[:space:]]*$" {inside=1; next}
    inside && $0 ~ "^[#]{1,6}[[:space:]]+[^[:space:]]" {exit}
    inside {print}
  ' "$SPEC"
)"

test -n "$GRAMMAR" || fail "EMPTY_Grammar_SECTION"

echo "$GRAMMAR" | awk '
  BEGIN{inside=0; saw=0; ok=0}
  /^```/{
    if (inside==0) { inside=1; saw=1; next }
    else { inside=0; next }
  }
  inside==1 {
    if (index($0, "::=") > 0 || index($0, "->") > 0) ok=1
  }
  END{
    if (saw==0) exit 10
    if (ok==0) exit 11
    exit 0
  }
' || {
  rc=$?
  if test "$rc" = "10"; then fail "NO_CODE_BLOCK_IN_Grammar"; fi
  if test "$rc" = "11"; then fail "NO_EBNF_SIGNAL"; fi
  fail "UNKNOWN"
}

echo "PASS G30_SPEC_EBNF_PRESENT"
