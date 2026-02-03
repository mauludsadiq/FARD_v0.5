cd "$(git rev-parse --show-toplevel)" 2>/dev/null || exit 1
set -u

BIN_FARDRUN="${BIN_FARDRUN:-./target/debug/fardrun}"
BIN_FARDLOCK="${BIN_FARDLOCK:-./target/debug/fardlock}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "ERROR: missing $1"; exit 2; }; }
need rg
need shasum
need sed
need awk

build() {
  cargo fmt
  cargo build
}

snap_help() {
  out="${1:-/tmp/fard_cli_snapshot}"
  rm -rf "$out"
  mkdir -p "$out"

  "$BIN_FARDRUN" --version > "$out/fardrun.version.txt"
  "$BIN_FARDRUN" --help    > "$out/fardrun.help.txt"
  "$BIN_FARDRUN" run --help > "$out/fardrun.run.help.txt"

  "$BIN_FARDLOCK" --version > "$out/fardlock.version.txt" 2>/dev/null || true
  "$BIN_FARDLOCK" --help    > "$out/fardlock.help.txt" 2>/dev/null || true
  "$BIN_FARDLOCK" lock --help > "$out/fardlock.lock.help.txt" 2>/dev/null || true

  shasum -a 256 "$out"/*.txt | awk '{print $2 " " $1}' | sort > "$out/digests.txt"
  cat "$out/digests.txt"
}

repro_help() {
  out="${1:-/tmp/fard_cli_snapshot}"
  snap_help "$out" > "$out/run1.txt"
  snap_help "$out" > "$out/run2.txt"

  h1="$(shasum -a 256 "$out/run1.txt" | awk '{print $1}')"
  h2="$(shasum -a 256 "$out/run2.txt" | awk '{print $1}')"

  echo "run1_sha256=$h1"
  echo "run2_sha256=$h2"
  test "$h1" = "$h2" && echo "PASS_CLI_REPRO_BYTES" || echo "FAIL_CLI_REPRO_BYTES"
}

run_fard() {
  prog="$1"
  out="$2"
  lockfile="${3:-}"
  registry="${4:-}"

  rm -rf "$out"
  mkdir -p "$out"

  args=(run --program "$prog" --out "$out")
  test -n "$lockfile" && args+=("--lockfile" "$lockfile")
  test -n "$registry" && args+=("--registry" "$registry")

  "$BIN_FARDRUN" "${args[@]}" >"$out/stdout.txt" 2>"$out/stderr.txt"
  rc=$?

  echo "RC=$rc"
  for f in stdout.txt stderr.txt trace.ndjson result.json error.json; do
    test -f "$out/$f" && echo "HAS $f" || echo "MISS $f"
  done

  echo
  echo "=== digests (raw bytes) ==="
  shasum -a 256 "$out"/* 2>/dev/null | awk '{print $2 " " $1}' | sort

  return 0
}

case "${1:-}" in
  build) build ;;
  snap_help) snap_help "${2:-/tmp/fard_cli_snapshot}" ;;
  repro_help) repro_help "${2:-/tmp/fard_cli_snapshot}" ;;
  run) shift; run_fard "$@" ;;
  *)
    echo "usage:"
    echo "  tools/fard_cli.sh build"
    echo "  tools/fard_cli.sh snap_help [OUT_DIR]"
    echo "  tools/fard_cli.sh repro_help [OUT_DIR]"
    echo "  tools/fard_cli.sh run <PROGRAM.fard> <OUT_DIR> [LOCKFILE] [REGISTRY]"
    exit 2
    ;;
esac
