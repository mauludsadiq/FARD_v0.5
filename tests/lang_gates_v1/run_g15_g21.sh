ROOT="tests/lang_gates_v1"
OUT="$ROOT/out_g15_g21"
FARD_BUNDLE_BIN="${FARD_BUNDLE_BIN:-target/debug/fardbundle}"

has() { test -f "$1"; }
grepq() { rg -n "$1" "$2" >/dev/null 2>/dev/null; }

run_bundle_build() {
  _root="$1"
  _entry="$2"
  _out="$3"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_BUNDLE_BIN" build --root "$_root" --entry "$_entry" --out "$_out" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -ne 0 ]; then
    return 1
  fi
  return 0
}

run_bundle_verify() {
  _bundle="$1"
  _out="$2"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_BUNDLE_BIN" verify --bundle "$_bundle" --out "$_out" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -ne 0 ]; then
    return 1
  fi
  return 0
}

run_bundle_run_expect_ok() {
  _bundle="$1"
  _out="$2"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_BUNDLE_BIN" run --bundle "$_bundle" --out "$_out" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -ne 0 ]; then
    return 1
  fi
  return 0
}

run_bundle_run_expect_fail() {
  _bundle="$1"
  _out="$2"
  rm -rf "$_out"
  mkdir -p "$_out"
  "$FARD_BUNDLE_BIN" run --bundle "$_bundle" --out "$_out" >/dev/null 2>"$_out/stderr.txt"
  _code=$?
  if [ $_code -eq 0 ]; then
    return 1
  fi
  return 0
}


gate_G15_bundle_build_artifacts() {
  out="$OUT/g15"
  root="tests/lang_gates_v1/projects/g15_bundle_basic"
  run_bundle_build "$root" "src/main.fard" "$out/build" || return 1
  has "$out/build/bundle/bundle.json" || return 1
  has "$out/build/bundle/imports.lock.json" || return 1
  test -d "$out/build/bundle/files" || return 1
  jq -e '.schema == "fard.bundle.v0_1"' "$out/build/bundle/bundle.json" >/dev/null 2>/dev/null || return 1
  jq -e '.schema == "fard.imports_lock.v0_1"' "$out/build/bundle/imports.lock.json" >/dev/null 2>/dev/null || return 1
  jq -e '.entry == "src/main.fard"' "$out/build/bundle/bundle.json" >/dev/null 2>/dev/null || return 1
  jq -e '(.files | length) >= 2' "$out/build/bundle/bundle.json" >/dev/null 2>/dev/null || return 1
}

gate_G16_bundle_determinism_bytes() {
  root="tests/lang_gates_v1/projects/g15_bundle_basic"
  a="$OUT/g16a"
  b="$OUT/g16b"
  run_bundle_build "$root" "src/main.fard" "$a/build" || return 1
  run_bundle_build "$root" "src/main.fard" "$b/build" || return 1
  cmp "$a/build/bundle/bundle.json" "$b/build/bundle/bundle.json" >/dev/null 2>/dev/null || return 1
  cmp "$a/build/bundle/imports.lock.json" "$b/build/bundle/imports.lock.json" >/dev/null 2>/dev/null || return 1
}

gate_G17_bundle_run_without_sources() {
  out="$OUT/g17"
  tmp="$out/tmp_proj"
  rm -rf "$out"
  mkdir -p "$out"
  cp -R "tests/lang_gates_v1/projects/g15_bundle_basic" "$tmp"
  run_bundle_build "$tmp" "src/main.fard" "$out/build" || return 1
  rm -rf "$tmp"
  bundle="$out/build/bundle/bundle.json"
  run_bundle_run_expect_ok "$bundle" "$out/run" || return 1
  has "$out/run/result.json" || return 1
  jq -e '.result == 42' "$out/run/result.json" >/dev/null 2>/dev/null || return 1
}

gate_G18_bundle_lock_mismatch_diagnostics() {
  out="$OUT/g18"
  root="tests/lang_gates_v1/projects/g15_bundle_basic"
  run_bundle_build "$root" "src/main.fard" "$out/build" || return 1
  lock="$out/build/bundle/imports.lock.json"
  test -f "$lock" || return 1
  perl -0777 -i -pe 's/"bundle_digest"\s*:\s*"sha256:[^"]+"/"bundle_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"/g' "$lock"
  bundle="$out/build/bundle/bundle.json"
  run_bundle_run_expect_fail "$bundle" "$out/run" || return 1
  has "$out/run/error.json" || return 1
  grepq 'ERROR_BUNDLE_LOCK_MISMATCH' "$out/run/error.json" || return 1
  grepq 'LOCK_MISMATCH' "$out/run/stderr.txt" || return 1
}

gate_G19_bundle_entry_missing_diagnostics() {
  out="$OUT/g19"
  root="tests/lang_gates_v1/projects/g15_bundle_basic"
  run_bundle_build "$root" "src/main.fard" "$out/build" || return 1
  bundle="$out/build/bundle/bundle.json"
  test -f "$bundle" || return 1
  perl -0777 -i -pe 's/"entry"\s*:\s*"src\/main\.fard"/"entry":"src\/missing\.fard"/g' "$bundle"
  run_bundle_run_expect_fail "$bundle" "$out/run" || return 1
  has "$out/run/error.json" || return 1
  grepq 'ERROR_BUNDLE_VERIFY' "$out/run/error.json" || return 1
  grepq 'entry missing in manifest' "$out/run/error.json" || return 1
}

gate_G20_bundle_path_traversal_blocked() {
  out="$OUT/g20"
  root="tests/lang_gates_v1/projects/g15_bundle_basic"
  run_bundle_build "$root" "src/main.fard" "$out/build" || return 1
  bundle="$out/build/bundle/bundle.json"
  test -f "$bundle" || return 1
  perl -0777 -i -pe 's/"path"\s*:\s*"lib\/util\.fard"/"path":"..\/pwn\.fard"/g' "$bundle"
  run_bundle_run_expect_fail "$bundle" "$out/run" || return 1
  has "$out/run/error.json" || return 1
  grepq 'ERROR_BUNDLE_VERIFY' "$out/run/error.json" || return 1
  grepq 'unsafe file path' "$out/run/error.json" || return 1
}

gate_G21_bundle_verify_roundtrip() {
  out="$OUT/g21"
  root="tests/lang_gates_v1/projects/g15_bundle_basic"
  run_bundle_build "$root" "src/main.fard" "$out/build" || return 1
  bundle="$out/build/bundle/bundle.json"
  run_bundle_verify "$bundle" "$out/verify" || return 1
  return 0
}

run_one() {
  name="$1"
  fn="$2"
  "$fn"
  code=$?
  if [ $code -eq 0 ]; then
    printf 'PASS %s\n' "$name"
    return 0
  else
    printf 'FAIL %s\n' "$name"
    return 1
  fi
}

main() {
  test -x "$FARD_BUNDLE_BIN" || { echo "missing $FARD_BUNDLE_BIN (build first: cargo build)"; return 1; }

  rm -rf "$OUT"
  mkdir -p "$OUT"

  passed=0
  failed=0

  run_one "G15_bundle_build_artifacts" gate_G15_bundle_build_artifacts && passed=$((passed+1)) || failed=$((failed+1))
  run_one "G16_bundle_determinism_bytes" gate_G16_bundle_determinism_bytes && passed=$((passed+1)) || failed=$((failed+1))
  run_one "G17_bundle_run_without_sources" gate_G17_bundle_run_without_sources && passed=$((passed+1)) || failed=$((failed+1))
  run_one "G18_bundle_lock_mismatch_diagnostics" gate_G18_bundle_lock_mismatch_diagnostics && passed=$((passed+1)) || failed=$((failed+1))
  run_one "G19_bundle_entry_missing_diagnostics" gate_G19_bundle_entry_missing_diagnostics && passed=$((passed+1)) || failed=$((failed+1))
  run_one "G20_bundle_path_traversal_blocked" gate_G20_bundle_path_traversal_blocked && passed=$((passed+1)) || failed=$((failed+1))
  run_one "G21_bundle_verify_roundtrip" gate_G21_bundle_verify_roundtrip && passed=$((passed+1)) || failed=$((failed+1))

  printf 'G15_G21_SUMMARY passed=%s failed=%s\n' "$passed" "$failed"
  if [ $failed -ne 0 ]; then
    return 1
  fi
  return 0
}

main "$@"
