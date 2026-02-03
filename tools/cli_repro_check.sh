cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1

bash tests/cli_snapshots/test_fardrun_help_version.sh >/tmp/fard_cli_snapshot/run1.txt
bash tests/cli_snapshots/test_fardrun_help_version.sh >/tmp/fard_cli_snapshot/run2.txt

h1="$(shasum -a 256 /tmp/fard_cli_snapshot/run1.txt | awk "{print \$1}")"
h2="$(shasum -a 256 /tmp/fard_cli_snapshot/run2.txt | awk "{print \$1}")"

echo "run1_sha256=$h1"
echo "run2_sha256=$h2"

test "$h1" = "$h2" && echo "PASS_CLI_REPRO_BYTES" || echo "FAIL_CLI_REPRO_BYTES"
