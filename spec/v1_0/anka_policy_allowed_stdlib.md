# ANKA v1.0 — Allowed stdlib surface

Status: normative (policy)
Authoritative: spec/v1_0/anka_policy_allowed_stdlib.v1.json
Note: This markdown is informational only; gates consume JSON only.

Scope: ANKA (Ez1 kernel + node) as described in "ANKA (where we are today) — Nov 13, 2025"

## Principle
ANKA requires exactly the operations needed to:
1) Canonicalize evidence into Z
2) Hash canonical Z into Digest
3) Sign/verify digests + assertions
4) Emit records/certificates + ledger events under witnessed boundaries

Everything else is forbidden.

## Allowed modules and exports

### std/hash
Allowed because ANKA’s terminal object is a SHA-256 digest.
- sha256
- sha256Text
- toHex
- is_sha256
- cid_hex

### std/bytes
Allowed for digest/signature material and deterministic encoding surfaces.
- len
- slice
- toHex
- fromHex

### std/codec
Allowed for deterministic, fully-specified encodings (no ambient data).
- hex_encode
- hex_decode
- base64_encode
- base64_decode

### std/json
Allowed for deterministic serialization of records/certificates and protocol messages.
- encode
- decode
- parse
- stringify
- pretty
- pathGet
- pathSet

### std/str
Allowed for canonicalization support and stable string transforms.
- len
- trim
- toLower
- toUpper
- split
- join
- replace
- contains
- startsWith
- endsWith
- slice
- padLeft
- padRight
- concat

### std/rec
Allowed for record assembly and field-stable manipulation.
- empty
- keys
- values
- has
- get
- getOr
- getOrErr
- set
- remove
- merge
- select
- rename
- update

### std/list
Allowed for deterministic collection transforms used in ledger/witness aggregation.
- len
- isEmpty
- push
- map
- filter
- flatMap
- fold
- take
- drop
- slice
- enumerate
- groupBy
- sort
- sortBy
- stableSortBy
- unique
- uniqueBy
- chunk
- hist_int
- sort_by_int_key
- sort_int
- dedupe_sorted_int
- get

### std/result
Allowed because ANKA’s node protocol is Result-shaped and must be composable.
- ok
- err
- isOk
- isErr
- map
- mapErr
- andThen
- orElse
- unwrapOr
- unwrapOrElse
- toOption
- fromOption
- unwrap_ok
- unwrap_err

### std/option
Allowed for absence-bearing flows in reconstruction/lookups.
- Some
- None
- isSome
- isNone
- map
- andThen
- unwrapOr
- unwrapOrElse
- toResult
- fromNullable
- toNullable

### std/trace
Allowed only as witnessed, append-only logging (ledger + audit).
- emit
- info
- warn
- error
- module_graph
- artifact_in
- artifact_out

### std/artifact
Allowed only as witnessed IO surfaces for record/certificate persistence.
- import
- emit
- list
- bytes
- cid_of_bytes
- in
- out

### std/time
Allowed only if the runtime witnesses it as an oracle boundary event.
ANKA uses timestamps for Record fields; therefore time is permitted only through witnessed calls.
- parse
- format
- add
- sub
- now_utc  (effect; must be witnessed; may be disabled in “strict determinism” profiles)

### std/fs
Allowed only if the runtime witnesses it as an oracle boundary event and the roots are sandboxed.
- read
- readAll
- writeAll
- exists
- listDir
- open
- create
- close

### std/http
Allowed only for networked node operation and only as witnessed requests with fully specified payloads.
- get
- post
- request
- okOr

### std/schema
Allowed for admission-gate checking (K7) when ANKA upgrades from ALLOWALL to real jurisdictions.
- check

## Notes
- Any allowed "effect" export (time/fs/http/artifact/trace) is only allowed under witnessed trace semantics.
- ANKA policy explicitly allows network service behavior because an Ez1 node is, by definition, a network participant.
