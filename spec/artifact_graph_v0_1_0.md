Proceeding to **M3 — Artifact Causality Closure** with the same closure discipline as M1/M2: **spec → verifier → gates → runtime conformance → single-invocation PASS**.

M3 freezes the *artifact provenance graph* as a verifier-authoritative ledger fact.

---

## M3.A Artifact node identity rules (frozen)

**Artifact identity is `cid` (sha256 of artifact bytes).**
Names are labels. `cid` is identity.

Trace declares artifact nodes via existing trace events (M2 vocabulary is unchanged):

- `artifact_in`  : declares an input artifact node
- `artifact_out` : declares an output artifact node

For M3, these are the only artifact node declarations.

### Event: `artifact_in`
Required keys:
- `t`: "artifact_in"
- `name`: string
- `cid`: string, must match ^sha256:[0-9a-f]{64}$

Forbidden keys:
- anything else

### Event: `artifact_out`
Required keys:
- `t`: "artifact_out"
- `name`: string
- `cid`: string, must match ^sha256:[0-9a-f]{64}$

Forbidden keys:
- anything else

---

## M3.B Artifact bytes layout (frozen)

For every `artifact_in` / `artifact_out` event with cid `sha256:<hex>`, the run bundle MUST contain the artifact bytes at:

- out/<run>/artifacts/<hex>.bin

Where `<hex>` is the 64-hex digest without the `sha256:` prefix.

Stop condition:
- every artifact cid referenced in trace has a corresponding artifact bytes file present.

---

## M3.C artifact_graph.json (frozen)

Each run bundle MUST include a single file:

- artifact_graph.json (canonical JSON)

Schema (strict):
{
  "v": "0.1.0",
  "nodes": [
    {"cid":"sha256:…","name":"…","role":"in"|"out"}
  ],
  "edges": [
    {"from":"sha256:…","kind":"used_by","to":"sha256:…"}
  ]
}

Constraints:
- v MUST equal "0.1.0"
- nodes MUST be unique by cid
- every edge endpoint cid MUST exist in nodes
- kind is frozen to "used_by" in M3
- artifact_graph.json MUST be canonical JSON (same canonicalization rule as M2)

---

## M3.D Ledger invariants tying trace ↔ artifacts ↔ graph (frozen)

Verifier MUST enforce:

1) Trace schema remains valid (M2 verifier must pass).

2) artifact_graph.json exists and parses, is canonical, matches schema, and is internally valid:
   - unique node cids
   - all edge endpoints exist as nodes

3) For every node cid in artifact_graph.json:
   - there exists an artifact bytes file artifacts/<hex>.bin

4) digests.json must acknowledge artifact_graph.json as part of the bundle:
   - digests.json.files["artifact_graph.json"] exists

---

## M3.E Verifier entrypoint (normative)

Add verifier entrypoint:

- fardverify artifact --out <dir>

It:
- runs M2 trace verification
- validates artifact_graph.json (schema + canonical bytes)
- validates artifact bytes presence for referenced cids
- validates digests.json includes artifact_graph.json
- emits:
  - <dir>/PASS_ARTIFACT.txt on pass
  - <dir>/FAIL_ARTIFACT.txt on fail
- exits 0 on pass; nonzero on fail

---

## M3.F Gate tranche (closure mechanism)

Five tests, all passing in one cargo invocation:

1) m3_artifact_vocab_ok
2) m3_artifact_missing_file_rejected
3) m3_artifact_graph_missing_rejected
4) m3_artifact_graph_noncanonical_rejected
5) m3_artifact_graph_bad_edge_rejected

Stop condition for M3:
- the full M3 tranche passes together under a single runner build.
