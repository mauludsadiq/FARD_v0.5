#!/bin/sh

MAN="ontology/stdlib_surface.v1_0.ontology.json"
TMP="/tmp/ontology_validate.out"
CAN="/tmp/stdlib_surface.v1_0.ontology.canon.json"

echo "CHECK_JSON_PARSE $MAN"
jq -e '.' "$MAN" >/dev/null

echo "CHECK_HEADER kind+version"
jq -r '.kind+" "+.version' "$MAN"

echo "CHECK_ENTRIES_VALIDATE"
rm -f "$TMP"
jq -r -f tools/ontology_validate_surface.jq "$MAN" | tee "$TMP"
if [ -s "$TMP" ]; then
  echo "ENTRIES_VALIDATE_FAIL count=$(wc -l < "$TMP" | tr -d ' ')"
  sed -n '1,200p' "$TMP"
  false
else
  echo "OK ENTRIES_VALIDATE"
fi

echo "CHECK_UNIQUE module::export"
jq -r '
  .entries
  | map(.module + "::" + .export)
  | sort
  | . as $xs
  | [range(1; length) | select($xs[.] == $xs[.-1]) | $xs[.]]
  | if length == 0 then
      "OK UNIQUE module::export"
    else
      "DUPLICATE_EXPORTS " + (join(","))
    end
' "$MAN" | tee /tmp/ontology_unique.out
rg -n '^DUPLICATE_EXPORTS\b' /tmp/ontology_unique.out >/dev/null && false || true

echo "CHECK_STAGE_NOT_CONSTRUCT"
jq -r '
  .entries
  | map(select(.pipe=="Stage") | select(.intent=="construct"))
  | if length == 0 then "OK STAGE_NOT_CONSTRUCT" else "BAD_STAGE_CONSTRUCT " + (length|tostring) end
' "$MAN" | tee /tmp/ontology_stage.out
rg -n '^BAD_STAGE_CONSTRUCT\b' /tmp/ontology_stage.out >/dev/null && false || true

echo "CANONICALIZE_AND_DIGEST"
jq -cS . "$MAN" > "$CAN"
shasum -a 256 "$CAN" | awk '{print "CANON_SHA256 " $1}'
wc -c < "$CAN" | tr -d ' ' | awk '{print "CANON_BYTES " $1}'
