set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

echo "== Gate 1 (ABI vector0) =="
cargo test --manifest-path crates/abirunner/Cargo.toml --test vector0_abi

echo
echo "== Gate 2 (Satisfied effect vectorA) =="
cargo test --manifest-path crates/abirunner/Cargo.toml --test vectorA_abi

echo
echo "== Gate 3 (Missing fact precedence) =="
cargo test --manifest-path crates/abirunner/Cargo.toml --test vectorB_missing_fact_precedence

echo
echo "== Gate 4 (Registry round-trip) =="
cargo test --manifest-path crates/abirunner/Cargo.toml --test registry_roundtrip

echo
echo "== Gate 5 (fardc -> bundle -> abirun) =="
cargo test --manifest-path crates/fardc/Cargo.toml --test gate5_compile_to_bundle


echo
echo "== Gate 6 (v1 canon bytes) =="
cargo test --manifest-path crates/fardc/Cargo.toml --test gate6_v1_canon_module_bytes
echo



echo

echo
echo "== Gate 7 (valuecore canon json + overflow) =="
cargo test --manifest-path crates/valuecore/Cargo.toml --test gate7_valuecore_canon_json
echo "[PASS] gates_stack_v0"
