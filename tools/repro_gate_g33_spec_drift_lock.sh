#!/usr/bin/env bash
set -euo pipefail

SPEC="SPEC.md"

fail(){ printf 'FAIL G33_SPEC_DRIFT_LOCK %s\n' "$1"; exit 1; }
pass(){ printf 'PASS G33_SPEC_DRIFT_LOCK\n'; }

test -f "$SPEC" || fail "MISSING_SPEC_MD"

git rev-parse --is-inside-work-tree >/dev/null 2>&1 || fail "NOT_A_GIT_REPO"

if git diff --name-only -- "$SPEC" | rg -n '^SPEC\.md$' >/dev/null 2>&1; then
  rg -n '^[#]+[[:space:]]*Changelog[[:space:]]*$' "$SPEC" >/dev/null 2>&1 || fail "MISSING_CHANGELOG_HEADING"
  rg -n '^[[:space:]]*[-*][[:space:]]*[0-9]{4}-[0-9]{2}-[0-9]{2}[[:space:]]+' "$SPEC" >/dev/null 2>&1 || fail "MISSING_DATED_CHANGELOG_ENTRY"
fi

pass
