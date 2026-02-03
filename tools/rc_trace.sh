cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1
set -u

run() {
  echo
  echo "CMD: $*"
  "$@"
  rc=$?
  echo "RC=$rc"
  return 0
}

run command -v bash
run command -v perl
run command -v awk
run command -v rg
run command -v cargo

run cargo fmt
run cargo build
