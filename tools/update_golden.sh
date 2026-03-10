#!/bin/bash
set -e
echo "Updating m5 preimage golden..."
rm -rf out/m5_ok_bundle
cargo run -q --bin fardrun -- run --program spec/tmp/m5_ok_bundle.fard --out out/m5_ok_bundle 2>/dev/null
cargo run -q --bin fardlock -- show-preimage --out out/m5_ok_bundle > spec/golden/m5_preimage.json

echo "Updating m6 golden_01..."
rm -rf out/m6_golden_01
cargo run -q --bin fardrun -- run --program spec/m6/programs/golden_01.fard --out out/m6_golden_01 2>/dev/null
mkdir -p spec/m6/golden/golden_01
cp out/m6_golden_01/result.json spec/m6/golden/golden_01/result.json
cp out/m6_golden_01/trace.ndjson spec/m6/golden/golden_01/trace.ndjson
cp out/m6_golden_01/module_graph.json spec/m6/golden/golden_01/module_graph.json
cp out/m6_golden_01/digests.json spec/m6/golden/golden_01/digests.json

echo "All golden files updated."
