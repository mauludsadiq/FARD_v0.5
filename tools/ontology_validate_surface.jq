def in_set($xs): . as $x | ($xs | index($x)) != null;
def is_nonempty_string: (type=="string") and (length>0);

def req_keys: ["module","export","intent","return","pipe"];

def validate_entry($i; $e):
  if ($e|type) != "object" then
    "BAD_ENTRY_TYPE entry_index=\($i) type=\($e|type)"
  elif (req_keys | all(.[]; . as $k | ($e | has($k)))) | not then
    "MISSING_REQUIRED_KEYS entry_index=\($i) keys_missing=" +
    (req_keys | [ .[] | . as $k | select(($e|has($k))|not) | $k ] | join(","))
  elif (($e.module | is_nonempty_string) | not) then
    "BAD_MODULE entry_index=\($i) module=\($e.module|tostring)"
  elif (($e.export | is_nonempty_string) | not) then
    "BAD_EXPORT entry_index=\($i) export=\($e.export|tostring)"
  elif (($e.intent | in_set(["construct","transform","query","effect"])) | not) then
    "BAD_INTENT entry_index=\($i) intent=\($e.intent|tostring)"
  elif (($e.return | in_set(["Value","Option","Result"])) | not) then
    "BAD_RETURN entry_index=\($i) return=\($e.return|tostring)"
  elif (($e.pipe | in_set(["Stage","No"])) | not) then
    "BAD_PIPE entry_index=\($i) pipe=\($e.pipe|tostring)"
  elif ($e.notes? and (($e.notes|type) != "string")) then
    "BAD_NOTES_TYPE entry_index=\($i) notes_type=\($e.notes|type)"
  else
    empty
  end;

. as $doc
| ($doc.kind // "") as $kind
| ($doc.version // "") as $ver
| if ($kind|type)!="string" or ($kind|length)==0 then
    "BAD_HEADER kind"
  elif ($ver|type)!="string" or ($ver|length)==0 then
    "BAD_HEADER version"
  elif ($doc.entries|type)!="array" then
    "BAD_HEADER entries_not_array type=\($doc.entries|type)"
  else
    ($doc.entries
     | to_entries[]
     | validate_entry(.key; .value)
    )
  end
