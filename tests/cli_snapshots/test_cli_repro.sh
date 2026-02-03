cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1
set -u

bash tools/fard_cli.sh build
bash tools/fard_cli.sh repro_help /tmp/fard_cli_snapshot
