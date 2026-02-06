set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
J="$ROOT/spec/stdlib_surface_tables.v1_0.ontology.json"
P="$ROOT/src/builtin_pipe_v1.rs"

STAGE_TXT="$(mktemp -t fard.stage.XXXXXX)"
STAGE_JSON="$(mktemp -t fard.stage.XXXXXX).json"
TMP="$(mktemp -t fard.ontology.XXXXXX).json"

rg -n 's\.insert\("std/[^"]+"\)\s*;' "$P" \
  | sed -E 's/.*s\.insert\("([^"]+)"\).*/\1/' \
  | sort -u > "$STAGE_TXT"

jq -Rn '[inputs]' < "$STAGE_TXT" > "$STAGE_JSON"
jq -e . "$STAGE_JSON" >/dev/null

jq --slurpfile stage "$STAGE_JSON" '
  def in_stage($fq): ($stage[0] | index($fq)) != null;
  .modules |= map(
    . as $m
    | .exports |= map(
        . as $e
        | ($m.name + "::" + $e.name) as $fq
        | .pipe = (if in_stage($fq) then "Stage" else "No" end)
      )
  )
' "$J" > "$TMP"

test -s "$TMP"
jq -e . "$TMP" >/dev/null

mv -f "$TMP" "$J"
