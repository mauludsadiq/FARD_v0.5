#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

fail(){ echo "FAIL G31_NO_PYTHON_IN_TOOLS $1"; exit 1; }

# Scan only shell scripts; ignore:
# - this file itself
# - full-line comments
# - commented tail after whitespace+#
HITS="$(
  find tools -type f -name '*.sh' -print0 \
  | rg -0 -n --no-heading --fixed-strings -e 'python' -e 'python3' \
      --glob 'tools/*.sh' \
      --glob '!tools/repro_gate_g31_no_python_in_tools.sh' \
  | awk '
      {
        line=$0
        # strip the "path:line:" prefix for comment detection
        sub(/^[^:]+:[0-9]+:/, "", line)
        # drop pure comment lines
        if (line ~ /^[[:space:]]*#/) next
        # drop inline comments (anything after #)
        sub(/[[:space:]]*#.*/, "", line)
        # after stripping comments, require python tokens remain
        if (line ~ /(^|[^A-Za-z0-9_])(python3?|python)([^A-Za-z0-9_]|$)/) print $0
      }
    '
)"

if test -n "$HITS"; then
  echo "FOUND_PYTHON_REFERENCES:"
  printf '%s\n' "$HITS"
  fail "PYTHON_REFERENCE_IN_TOOLS"
fi

echo "PASS G31_NO_PYTHON_IN_TOOLS"
