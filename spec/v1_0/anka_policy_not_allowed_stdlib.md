# ANKA v1.0 — Not allowed stdlib surface

Status: normative (policy)
Authoritative: spec/v1_0/anka_policy_allowed_stdlib.v1.json
Note: This markdown is informational only; gates consume JSON only.

Scope: ANKA (Ez1 kernel + node)

## Principle
If an export is not explicitly listed in ANKA’s allowed policy, it is forbidden.

Additionally, the following categories are explicitly forbidden even if they exist in the global stdlib surface:

## Explicitly forbidden modules (entire module forbidden)
- std/random
- std/regex
- std/num  (unless later pinned to a fully deterministic numeric model needed by canonicalization)
- std/env  (except out_dir if explicitly treated as informational, not access to ambient state)
- any future module providing nondeterministic iteration, clocks without witnessing, host introspection, threads, or OS process control

## Explicitly forbidden behaviors
- Any access to ambient machine state not fully specified in inputs and not witnessed in trace.
  Examples:
  - reading environment variables (except explicitly declared and traced)
  - reading current time (unless witnessed)
  - filesystem/network access (unless witnessed and sandboxed)
- Any hidden randomness (even “seeded” randomness) unless the seed is explicit input and the algorithm is frozen and witnessed.
- Any operation whose semantics depend on host locale, timezone, floating-point flags, or platform-dependent string collation.

## Forbidden exports within otherwise allowed modules
Even within allowed modules, these are forbidden unless later explicitly admitted:

### std/trace
- any export that mutates or rewrites prior trace lines
- any export that emits non-canonical JSON lines

### std/artifact
- any export that deletes, overwrites, or mutates previously emitted artifacts in-place without producing new digests

### std/http
- any export that allows implicit headers, implicit retries, or implicit timeouts not fully specified in request inputs

### std/fs
- any export that reads/writes outside declared roots
- any export that enumerates system-wide directories without sandbox bounds

## Rationale (why these are forbidden)
ANKA’s credibility rests on: E → Z → Digest being reproducible and auditable.
Anything that introduces ambient variability or non-witnessed effects breaks the algebraic guarantees and invalidates the compliance certificate model.
