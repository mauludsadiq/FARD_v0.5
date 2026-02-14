ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

SPEC_DIR="spec/v1_0/golden_bundle/v1"
OUTDIR="out/golden_bundle_verify_v1"
mkdir -p "$OUTDIR"

ok=1

if [ ! -f "$SPEC_DIR/program.fard" ]; then
  printf "MISSING spec golden program: %s\n" "$SPEC_DIR/program.fard"
  ok=0
fi

if [ "$ok" = "1" ]; then
  bash tools/_fardrun_try.sh "$SPEC_DIR/program.fard" "$OUTDIR" || ok=0
fi

for f in result.json trace.ndjson module_graph.json digests.json artifact_graph.json run_bundle.cid.txt; do
  if [ -f "$SPEC_DIR/$f" ] && [ -f "$OUTDIR/$f" ]; then
    if cmp -s "$SPEC_DIR/$f" "$OUTDIR/$f"; then
      :
    else
      printf "MISMATCH %s\n" "$f"
      ok=0
    fi
  fi
done

if [ "$ok" = "1" ]; then
  printf "OK golden bundle bytes match\n"
else
  printf "FAIL golden bundle bytes mismatch\n"
  false
fi
