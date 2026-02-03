#!/usr/bin/env bash
set -euo pipefail

SPEC="SPEC.md"

fail() { echo "FAIL G28_SPEC_CONTRACT $1"; exit 1; }

test -f "$SPEC" || fail "MISSING_SPEC_MD"

rg -n '^Version:\s*0\.5\.0\s*$' "$SPEC" >/dev/null || fail "MISSING_VERSION_0_5_0"

req() {
  local h="$1"
  rg -n "^[#]{1,6}[[:space:]]+$h[[:space:]]*$" "$SPEC" >/dev/null || fail "MISSING_HEADING_$h"
}

req "Tokens"
req "Grammar"
req "Eval"
req "Values"
req "Trace"
req "Modules"
req "Out-dir"
req "Errors"
req "Changelog"

echo "PASS G28_SPEC_CONTRACT"
