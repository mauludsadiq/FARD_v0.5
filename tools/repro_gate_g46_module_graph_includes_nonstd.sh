set -euo pipefail

cargo build >/dev/null

FARDRUN="target/debug/fardrun"
test -x "$FARDRUN" || { echo "FAIL G46_MODULE_GRAPH_INCLUDES_NONSTD MISSING_fardrun"; exit 1; }

P="tests/fixtures/imports/app_rel/main.fard"
test -f "$P" || { echo "FAIL G46_MODULE_GRAPH_INCLUDES_NONSTD MISSING_FIXTURE $P"; exit 1; }

OUT="/tmp/fard_g46.$$.$(date +%s)"
rm -rf "$OUT"
mkdir -p "$OUT"

"$FARDRUN" run --program "$P" --out "$OUT" >/dev/null 2>&1 || { echo "FAIL G46_MODULE_GRAPH_INCLUDES_NONSTD RUN_FAILED"; exit 1; }

MG="$OUT/module_graph.json"
test -f "$MG" || { echo "FAIL G46_MODULE_GRAPH_INCLUDES_NONSTD MISSING_module_graph.json"; exit 1; }

rg -n '"nodes"' "$MG" >/dev/null || { echo "FAIL G46_MODULE_GRAPH_INCLUDES_NONSTD NO_nodes"; exit 1; }
rg -n 'tests/fixtures/imports/app_rel' "$MG" >/dev/null || { echo "FAIL G46_MODULE_GRAPH_INCLUDES_NONSTD MISSING_rel_node"; exit 1; }

echo "PASS G46_MODULE_GRAPH_INCLUDES_NONSTD"
