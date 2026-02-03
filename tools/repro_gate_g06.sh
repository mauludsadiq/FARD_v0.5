cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1
set -eu

PROG="tests/gate.bak_syntaxfix_1/programs/g06_std_grow_unfold.fard"
OUT="/tmp/fard_gate_g06"

rm -rf "$OUT"
mkdir -p "$OUT"

./target/debug/fardrun run --program "$PROG" --out "$OUT" >"$OUT/stdout.txt" 2>"$OUT/stderr.txt" || true

for f in stdout.txt stderr.txt trace.ndjson result.json error.json; do
  test -f "$OUT/$f" && echo "HAS $f" || echo "MISS $f"
done

echo
echo "=== stderr ==="
sed -n '1,120p' "$OUT/stderr.txt" || true

echo
echo "=== error.json ==="
test -f "$OUT/error.json" && cat "$OUT/error.json" || true

echo
echo "=== digests (raw bytes) ==="
shasum -a 256 "$OUT"/* 2>/dev/null | awk '{print $2 " " $1}' | sort
