ROOT="tests/lang_gates_v1"
OUT="$ROOT/out_tranche_a"
PKG="$ROOT/packages"
REG="$ROOT/registry"

mkdir -p "$OUT"

FARD_RUN_BIN="${FARD_RUN_BIN:-target/debug/fardrun}"
FARD_BUNDLE_BIN="${FARD_BUNDLE_BIN:-target/debug/fardbundle}"
FARD_LOCK_BIN="${FARD_LOCK_BIN:-target/debug/fardlock}"
FARD_PKG_BIN="${FARD_PKG_BIN:-target/debug/fardpkg}"

has() { test -f "$1"; }
grepq() { rg -n "$1" "$2" >/dev/null 2>/dev/null; }

run_publish() {
  _root="$1"
  _out="$2"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_PKG_BIN" publish --root "$_root" --registry "$REG" --out "$_out" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -ne 0 ]; then
    return 1
  fi
  return 0
}

run_lockgen() {
  _root="$1"
  _out="$2"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_LOCK_BIN" gen --root "$_root" --registry "$REG" --out "$_out" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -ne 0 ]; then
    return 1
  fi
  return 0
}

run_app() {
  _entry="$1"
  _lock="$2"
  _out="$3"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_RUN_BIN" run "$_entry" --out "$_out" --lock "$_lock" --registry "$REG" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -ne 0 ]; then
    return 1
  fi
  return 0
}

run_app_expect_fail() {
  _entry="$1"
  _lock="$2"
  _out="$3"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_RUN_BIN" run "$_entry" --out "$_out" --lock "$_lock" --registry "$REG" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -eq 0 ]; then
    return 1
  fi
  return 0
}

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1"; return 1; }

gate_G22_publish_writes_registry() {
  out="$OUT/g22"
  rm -rf "$REG"
  mkdir -p "$REG"

  run_publish "$PKG/libmath" "$out/pub_math" || return 1
  run_publish "$PKG/libutil" "$out/pub_util" || return 1
  run_publish "$PKG/app"    "$out/pub_app"  || return 1

  test -d "$REG/pkgs/libmath/0.1.0" || return 1
  test -d "$REG/pkgs/libutil/0.1.0" || return 1
  test -d "$REG/pkgs/app/0.1.0" || return 1

  pass G22_publish_writes_registry
}

gate_G23_lockgen_schema_and_files() {
  out="$OUT/g23"

  run_lockgen "$PKG/app" "$out/lock" || return 1

  has "$out/lock/fard.lock.json" || return 1
  has "$out/lock/fard.lock.json.cid" || return 1
  jq -e '.schema == "fard.lock.v0_1"' "$out/lock/fard.lock.json" >/dev/null 2>/dev/null || return 1
  jq -e '.package.name == "app"' "$out/lock/fard.lock.json" >/dev/null 2>/dev/null || return 1
  jq -e '.package.version == "0.1.0"' "$out/lock/fard.lock.json" >/dev/null 2>/dev/null || return 1

  pass G23_lockgen_schema_and_files
}

gate_G24_lockgen_determinism_bytes() {
  out="$OUT/g24"
  a="$out/a"
  b="$out/b"

  run_lockgen "$PKG/app" "$a" || return 1
  run_lockgen "$PKG/app" "$b" || return 1

  cmp -s "$a/fard.lock.json" "$b/fard.lock.json" || return 1

  pass G24_lockgen_determinism_bytes
}

gate_G25_run_app_with_lock_and_registry() {
  out="$OUT/g25"

  run_lockgen "$PKG/app" "$out/lock" || return 1
  run_app "$PKG/app/src/main.fard" "$out/lock/fard.lock.json" "$out/run" || return 1

  has "$out/run/result.json" || return 1
  jq -e '.result.c == 35' "$out/run/result.json" >/dev/null 2>/dev/null || return 1

  pass G25_run_app_with_lock_and_registry
}

gate_G26_lock_mismatch_blocks_registry_tamper() {
  out="$OUT/g26"

  run_lockgen "$PKG/app" "$out/lock" || return 1

  tmp="$out/tmp"
  rm -rf "$tmp"
  mkdir -p "$tmp"
  cp -a "$out/lock/fard.lock.json" "$tmp/fard.lock.json"
  perl -0777 -i -pe 's/"digest"\s*:\s*"sha256:[0-9a-f]{64}"/"digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"/' "$tmp/fard.lock.json" >/dev/null 2>/dev/null || true

  run_app_expect_fail "$PKG/app/src/main.fard" "$tmp/fard.lock.json" "$out/run" || return 1
  grepq "LOCK_MISMATCH" "$out/run/stderr.txt" || return 1
  has "$out/run/error.json" || return 1

  pass G26_lock_mismatch_blocks_registry_tamper
}

gate_G27_exports_enforced() {
  out="$OUT/g27"


  mkdir -p "$out"

  prog="$out/bad_exports.fard"
  cat > "$prog" <<'FARD'
import("pkg:libmath@0.1.0/std/math") as M
{ x: M._hidden }
FARD

  run_lockgen "$PKG/app" "$out/lock" || return 1
  run_app_expect_fail "$prog" "$out/lock/fard.lock.json" "$out/run" || return 1
  grepq "EXPORT_MISSING" "$out/run/stderr.txt" || return 1
  has "$out/run/error.json" || return 1

  pass G27_exports_enforced
}

passed=0
failed=0

gate_G22_publish_writes_registry || failed=$((failed+1))
gate_G23_lockgen_schema_and_files || failed=$((failed+1))
gate_G24_lockgen_determinism_bytes || failed=$((failed+1))
gate_G25_run_app_with_lock_and_registry || failed=$((failed+1))
gate_G26_lock_mismatch_blocks_registry_tamper || failed=$((failed+1))
gate_G27_exports_enforced || failed=$((failed+1))

passed=$((6 - failed))
echo "G22_G27_SUMMARY passed=$passed failed=$failed"

test "$failed" -eq 0
