ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

OUTDIR="out/golden_bundle_v1"
SPEC_DIR="spec/v1_0/golden_bundle/v1"
mkdir -p "$OUTDIR"
mkdir -p "$SPEC_DIR"

if [ ! -f "$SPEC_DIR/program.fard" ]; then
  if [ -f "spec/tmp/golden_program_01.fard" ]; then
    cp "spec/tmp/golden_program_01.fard" "$SPEC_DIR/program.fard"
  elif [ -f "spec/tmp/g54_g60_kitchen_sink_4.fard" ]; then
    cp "spec/tmp/g54_g60_kitchen_sink_4.fard" "$SPEC_DIR/program.fard"
  elif [ -f "spec/tmp/g54_g60_kitchen_sink_3.fard" ]; then
    cp "spec/tmp/g54_g60_kitchen_sink_3.fard" "$SPEC_DIR/program.fard"
  fi
fi

if [ ! -f "$SPEC_DIR/program.fard" ]; then
  printf "MISSING program.fard\n"
  printf "Place the canonical golden program at %s\n" "$SPEC_DIR/program.fard"
  false
fi

bash tools/_fardrun_try.sh "$SPEC_DIR/program.fard" "$OUTDIR" || false

for f in result.json trace.ndjson module_graph.json digests.json artifact_graph.json run_bundle.cid.txt; do
  if [ -f "$OUTDIR/$f" ]; then
    cp "$OUTDIR/$f" "$SPEC_DIR/$f"
  fi
done

printf "WROTE golden bundle files into %s\n" "$SPEC_DIR"
