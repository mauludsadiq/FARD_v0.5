set -euo pipefail

test -x tools/fardfmt || { echo "FAIL G35_FMT_IDEMPOTENT_BYTES MISSING_tools/fardfmt"; exit 1; }

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

test -n "${P:-}" || { echo "FAIL G35_FMT_IDEMPOTENT_BYTES NO_PROGRAM_FOUND"; exit 1; }

a="$(mktemp "/tmp/g35.a.$$.$(date +%s).XXXX")"
b="$(mktemp "/tmp/g35.b.$$.$(date +%s).XXXX")"
c="$(mktemp "/tmp/g35.c.$$.$(date +%s).XXXX")"
trap 'rm -f "$a" "$b" "$c"' EXIT

tools/fardfmt "$P" > "$a"
tools/fardfmt "$a" > "$b"
tools/fardfmt "$b" > "$c"

HA="$(shasum -a 256 "$b" | awk '{print $1}')"
HB="$(shasum -a 256 "$c" | awk '{print $1}')"

test "$HA" = "$HB" || { echo "FAIL G35_FMT_IDEMPOTENT_BYTES HASH_MISMATCH $HA $HB"; exit 1; }

echo "PASS G35_FMT_IDEMPOTENT_BYTES"
