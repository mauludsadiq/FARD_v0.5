#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)" || exit 1
SPEC="SPEC.md"

fail(){ echo "FAIL G29_SPEC_MIN_CONTENT $1"; exit 1; }

test -f "$SPEC" || fail "MISSING_SPEC_MD"

REQ=(
  "Changelog"
  "Tokens"
  "Grammar"
  "Eval"
  "Values"
  "Trace"
  "Modules"
  "Out-dir"
  "Errors"
)

section_body() {
  local H="$1"
  awk -v H="$H" '
    BEGIN{inside=0}
    $0 ~ "^[#]{1,6}[[:space:]]+" H "[[:space:]]*$" {inside=1; next}
    inside && $0 ~ "^[#]{1,6}[[:space:]]+[^[:space:]]" {exit}
    inside {print}
  ' "$SPEC"
}

has_substantive() {
  local H="$1"
  section_body "$H" | awk '
    function trim(s){ sub(/^[ \t\r\n]+/, "", s); sub(/[ \t\r\n]+$/, "", s); return s }
    BEGIN{ok=0}
    {
      t=trim($0)
      if (t == "") next
      if (substr(t,1,1) == "#") next
      low=tolower(t)
      if (index(low, "this section is normative") == 1) next

      # Bullet lines must contain a colon.
      if (t ~ /^[-*][ \t]+/) {
        if (index(t, ":") > 0) { ok=1; exit }
        next
      }

      ok=1; exit
    }
    END{ exit(ok ? 0 : 1) }
  '
}

for h in "${REQ[@]}"; do
  grep -Eq "^[#]{1,6}[[:space:]]+${h}[[:space:]]*$" "$SPEC" || fail "MISSING_HEADING_${h}"

  if ! has_substantive "$h"; then
    if test "${DEBUG_G29:-0}" = "1"; then
      echo "DEBUG_G29_SECTION_BEGIN ${h}"
      section_body "$h" | nl -ba
      echo "DEBUG_G29_SECTION_END ${h}"
    fi
    fail "EMPTY_OR_PLACEHOLDER_${h}"
  fi
done

echo "PASS G29_SPEC_MIN_CONTENT"
