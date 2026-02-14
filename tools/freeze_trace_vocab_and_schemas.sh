ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

OUT="spec/v1_0/trace/TRACE_VOCAB_AND_SCHEMAS_EXTRACT.txt"
mkdir -p "spec/v1_0/trace"

SRC_FILE="$(rg -n --hidden --no-ignore-vcs 'TRACE_VOCAB|TRACE_KINDS|allowed_kinds|trace vocabulary' src -S | head -n 1 | cut -d: -f1)"
SCHEMA_FILE="$(rg -n --hidden --no-ignore-vcs 'trace schema|per-kind|kind schema|schema gate' src -S | head -n 1 | cut -d: -f1)"

printf "SOURCE_VOCAB_FILE=%s\n" "${SRC_FILE:-UNKNOWN}" > "$OUT"
printf "SOURCE_SCHEMA_FILE=%s\n\n" "${SCHEMA_FILE:-UNKNOWN}" >> "$OUT"

if [ -n "$SRC_FILE" ]; then
  perl -0777 -ne '
    if (m/(TRACE_VOCAB|TRACE_KINDS|ALLOWED_TRACE_KINDS)[^=]*=\s*&?\s*\[(.*?)\]\s*;/s) {
      $b=$2;
      $b=~s/[\r\n]//g;
      $b=~s/\s+//g;
      print "EXTRACTED_VOCAB_RUST_ARRAY=".$b."\n";
    } else {
      print "EXTRACTED_VOCAB_RUST_ARRAY=NOT_FOUND\n";
    }
  ' "$SRC_FILE" >> "$OUT"
else
  printf "EXTRACTED_VOCAB_RUST_ARRAY=UNKNOWN\n" >> "$OUT"
fi

printf "\nSCHEMA_TEXT_SNIPPET_BEGIN\n" >> "$OUT"
if [ -n "$SCHEMA_FILE" ]; then
  sed -n '1,240p' "$SCHEMA_FILE" >> "$OUT"
else
  printf "UNKNOWN\n" >> "$OUT"
fi
printf "\nSCHEMA_TEXT_SNIPPET_END\n" >> "$OUT"

printf "\nWROTE %s\n" "$OUT"
