cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1

cargo fmt
cargo build

bash tests/cli_snapshots/test_fardrun_help_version.sh
bash tools/cli_repro_check.sh
