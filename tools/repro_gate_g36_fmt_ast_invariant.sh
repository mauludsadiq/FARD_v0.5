set -euo pipefail

test -x tools/fardfmt || { echo "FAIL G36_FMT_AST_INVARIANT MISSING_tools/fardfmt"; exit 1; }

P=""
P="$(rg -n -o '[A-Za-z0-9_./-]+\.fard' tools/repro_gate_g*.sh \
  | cut -d: -f3- \
  | sort -u \
  | while IFS= read -r x; do test -f "$x" && { echo "$x"; break; }; done || true)"

if test -z "${P:-}"; then
  P="$(find tests -type f -name '*.fard' 2>/dev/null | sort | head -n 1 || true)"
fi
if test -z "${P:-}"; then
  P="$(find . -type f -name '*.fard' 2>/dev/null | sort | head -n 1 || true)"
fi

test -n "${P:-}" || { echo "FAIL G36_FMT_AST_INVARIANT NO_PROGRAM_FOUND"; exit 1; }

a="$(mktemp "/tmp/g36.a.$$.$(date +%s).XXXX")"
trap 'rm -f "$a"' EXIT
tools/fardfmt "$P" > "$a"

S1="$(cat "$P" | LC_ALL=C tr -d ' \t\n\r' | shasum -a 256 | awk '{print $1}')"
S2="$(cat "$a" | LC_ALL=C tr -d ' \t\n\r' | shasum -a 256 | awk '{print $1}')"

test "$S1" = "$S2" || { echo "FAIL G36_FMT_AST_INVARIANT NON_WS_CHANGED $S1 $S2"; exit 1; }

echo "PASS G36_FMT_AST_INVARIANT"
