set -euo pipefail

test -x tools/fardfmt || { echo "FAIL G34_FMT_EXISTS MISSING_tools/fardfmt"; exit 1; }

tools/fardfmt --help >/dev/null

t="$(mktemp "/tmp/g34.$$.$(date +%s).XXXX")"
trap 'rm -f "$t"' EXIT
printf '%s\n' "let x = 1 in x" > "$t"

tools/fardfmt --check "$t" || true
tools/fardfmt --write "$t"
tools/fardfmt --check "$t"

echo "PASS G34_FMT_EXISTS"
