ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

try_run() {
  local prog="$1"
  local out="$2"

  cargo build -q

  if target/debug/fardrun run --help >/dev/null 2>&1; then
    target/debug/fardrun run --program "$prog" --out "$out" && return 0
    target/debug/fardrun run --program="$prog" --out="$out" && return 0
    target/debug/fardrun run --program "$prog" --out "$out" -- && return 0
  fi

  target/debug/fardrun --help >/dev/null 2>&1

  target/debug/fardrun -- "$prog" --out "$out" && return 0
  target/debug/fardrun "$prog" --out "$out" && return 0

  false
}

try_run "$1" "$2"
