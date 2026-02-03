#!/usr/bin/env bash
set -euo pipefail

G="G32_SPEC_IMPL_CONFORMANCE"

SPEC="SPEC.md"
FILE="src/bin/fardrun.rs"

fail(){ printf 'FAIL %s %s\n' "$G" "$1"; exit 1; }
pass(){ printf 'PASS %s\n' "$G"; exit 0; }

test -f "$SPEC" || fail "MISSING_SPEC_MD"
test -f "$FILE" || fail "MISSING_SRC_BIN_FARDRUN_RS"

tmp="${TMPDIR:-/tmp}/fard_${G}.$$"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp"

spec_kw="$tmp/spec_kw.txt"
src_kw="$tmp/src_kw.txt"
spec_op="$tmp/spec_op.txt"
src_op="$tmp/src_op.txt"
spec_out="$tmp/spec_out.txt"
need_out="$tmp/need_out.txt"

rg -o '\b(let|in|if|then|else|fn|import|as|export)\b' "$SPEC" --replace '$1' \
  | sort -u > "$spec_kw" || true
test -s "$spec_kw" || fail "NO_KEYWORDS_FOUND_IN_SPEC"

rg -o '"(let|in|if|then|else|fn|import|as|export)"' "$FILE" --replace '$1' \
  | sort -u > "$src_kw" || true
test -s "$src_kw" || fail "NO_KEYWORDS_FOUND_IN_SRC"

if ! diff -u "$spec_kw" "$src_kw" >/dev/null 2>&1; then
  printf 'SPEC_KEYWORDS:\n'
  cat "$spec_kw"
  printf 'SRC_KEYWORDS:\n'
  cat "$src_kw"
  fail "KEYWORDS_MISMATCH"
fi

rg -o '(==|<=|>=|&&|\|\||->)' "$SPEC" --replace '$1' \
  | sort -u > "$spec_op" || true
test -s "$spec_op" || fail "NO_OPERATORS_FOUND_IN_SPEC"

rg -o '"(==|<=|>=|&&|\|\||->)"' "$FILE" --replace '$1' \
  | sort -u > "$src_op" || true
test -s "$src_op" || fail "NO_OPERATORS_FOUND_IN_SRC"

if ! diff -u "$spec_op" "$src_op" >/dev/null 2>&1; then
  printf 'SPEC_OPERATORS:\n'
  cat "$spec_op"
  printf 'SRC_OPERATORS:\n'
  cat "$src_op"
  fail "OPERATORS_MISMATCH"
fi

printf '%s\n' trace.ndjson result.json stderr.txt error.json | sort -u > "$need_out"

rg -o '\b(trace\.ndjson|result\.json|stderr\.txt|error\.json)\b' "$SPEC" --replace '$1' \
  | sort -u > "$spec_out" || true
test -s "$spec_out" || fail "NO_OUTDIR_FILES_FOUND_IN_SPEC"

if ! diff -u "$need_out" "$spec_out" >/dev/null 2>&1; then
  printf 'REQUIRED_OUTDIR_FILES:\n'
  cat "$need_out"
  printf 'SPEC_OUTDIR_FILES:\n'
  cat "$spec_out"
  fail "OUTDIR_FILES_MISMATCH"
fi

pass
