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
echo "[PASS] gates_stack_v0"
