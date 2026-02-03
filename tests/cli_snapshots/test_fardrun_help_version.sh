cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1

exe="./target/debug/fardrun"

mkdir -p /tmp/fard_cli_snapshot

$exe --version > /tmp/fard_cli_snapshot/version.txt
$exe --help > /tmp/fard_cli_snapshot/help.txt
$exe run --help > /tmp/fard_cli_snapshot/run_help.txt

shasum -a 256 /tmp/fard_cli_snapshot/version.txt /tmp/fard_cli_snapshot/help.txt /tmp/fard_cli_snapshot/run_help.txt | awk '{print $2 " " $1}' > /tmp/fard_cli_snapshot/digests.txt

cat /tmp/fard_cli_snapshot/digests.txt
