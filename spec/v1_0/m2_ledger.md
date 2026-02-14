# FARD Execution Ledger Contract (M2)

This document is normative for trace.ndjson and ledger-level closure.

Status: sealed by M2 gate suite (canon, ordering, schema, vocab, boundary).

## 1. Canonical JSON
- Objects are emitted in canonical key order as enforced by canon gates.
- Any non-canonical ordering is rejected by verification.

## 2. Trace file format
- `trace.ndjson` is newline-delimited JSON objects.
- Each line is a single JSON object.
- Parsing failures (including invalid NDJSON) are verification failures.

## 3. Boundary rules
- Boundary constraints between parse/runtime failure and output artifacts are enforced by the boundary gates.

## 4. Event vocabulary
The allowed event kinds are exactly those enforced by the vocab gates.
Each event kind has an exact schema enforced by the schema gates.

(Enumerate event kinds here by copying the vocabulary list from the runtime verifier tables.)
