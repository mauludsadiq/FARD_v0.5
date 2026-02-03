cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1
set -eu

CLI_FILE="src/cli/fardlock_cli.rs"
BIN_FILE="src/bin/fardlock.rs"
BAK_FILE="src/bin/fardlock.rs.bak_cli_consolidation_2"

test -f "$CLI_FILE" || (echo "ERROR: missing $CLI_FILE" && exit 1)
test -f "$BIN_FILE" || (echo "ERROR: missing $BIN_FILE" && exit 1)
test -f "$BAK_FILE" || (echo "ERROR: missing $BAK_FILE" && exit 1)

cp -a "$BAK_FILE" "$BIN_FILE"

VARIANT="$(
  perl -0777 -ne '
    $s = $_;

    if ($s !~ /fn\s+parse_compat\s*\(\s*\)\s*->\s*\(\s*Command\s*,\s*bool\s*\)\s*\{/s) {
      exit 2;
    }

    $s =~ /fn\s+parse_compat\s*\(\s*\)\s*->\s*\(\s*Command\s*,\s*bool\s*\)\s*\{(.*?)\n\}/s or exit 3;
    $b = $1;

    if ($b =~ /Command::([A-Za-z0-9_]+)\s*(\{|,|\))/s) {
      print $1;
      exit 0;
    }

    if ($b =~ /\b([A-Za-z0-9_]+)\s*\{\s*.*?\}\s*[,)]/s) {
      print $1;
      exit 0;
    }

    exit 4;
  ' "$CLI_FILE" 2>/dev/null || true
)"

echo "CLI_FILE=$CLI_FILE"
echo "DETECTED_VARIANT=$VARIANT"
test -n "$VARIANT" || (echo "ERROR: could not detect Command variant from parse_compat() in $CLI_FILE" && exit 1)

perl -0777 -i -pe "s/(match\\s+cmd\\s*\\{\\s*\\n\\s*fard_v0_5_language_gate::cli::fardlock_cli::Command::)[A-Za-z0-9_]+/\\\$1$VARIANT/s" "$BIN_FILE"

echo
echo "=== BIN_FILE match arm now ==="
rg -n 'match cmd|cli::fardlock_cli::Command::' -n "$BIN_FILE" | sed -n '1,120p'

cargo fmt
cargo build
