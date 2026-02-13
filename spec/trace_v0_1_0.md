Proceeding to **M2 — Trace Schema Closure** with the same closure discipline you used for M1: **spec → gates → runtime conformance → single-invocation PASS**.

Below is the **exact M2 contract** (A–E) in a form you can drop into `SPEC.md` (or `spec/trace_v0_1_0.md`) and then immediately gate.

---

## M2.A Normative trace.ndjson schema (Trace Format v0.1.0)

### Global rules (apply to every line)
Each line in `trace.ndjson` is one JSON object (NDJSON).

**Required keys (every event):**
- `t` : string (event tag)

**Forbidden (global):**
- No top-level keys other than those defined for the event tag.
- No duplicate keys (JSON parser enforcement).

**Allowed event tags (frozen set):**
- `module_resolve`
- `module_graph`
- `artifact_in`
- `artifact_out`
- `error`

> Note: you can add spans later, but **not in M2**. M2 closes the vocabulary.

---

### Event: `module_resolve`
Represents a resolved module identity that the run depends on.

**Required keys**
- `t`: `"module_resolve"`
- `kind`: `"std"` | `"rel"` | `"abs"` | `"vendor"` (choose the set you actually emit; if only `"std"` exists today, freeze only `"std"` now)
- `name`: string (stable human label; for std modules this is `std/result`)
- `cid`: string, must match `^sha256:[0-9a-f]{64}$`

**Forbidden keys**
- anything else

---

### Event: `module_graph`
Represents the emitted `module_graph.json` digest for the run.

**Required keys**
- `t`: `"module_graph"`
- `cid`: string, `sha256:` digest of `module_graph.json`

**Forbidden keys**
- anything else

---

### Event: `artifact_in`
Represents an external artifact being read/ingested.

**Required keys**
- `t`: `"artifact_in"`
- `name`: string (logical artifact name; stable)
- `cid`: string (`sha256:` digest of artifact bytes)

**Optional keys (either freeze now or forbid now)**
- `mime`: string
- `bytes`: integer

**Forbidden keys**
- anything else

---

### Event: `artifact_out`
Represents an artifact being written/emitted.

**Required keys**
- `t`: `"artifact_out"`
- `name`: string (logical artifact name; stable)
- `cid`: string (`sha256:` digest of artifact bytes)

**Optional keys (either freeze now or forbid now)**
- `mime`: string
- `bytes`: integer

**Forbidden keys**
- anything else

---

### Event: `error`
Represents a terminal failure **of the run** (parse error, internal error, lock failure, etc.)

**Required keys**
- `t`: `"error"`
- `code`: string (stable error code, e.g. `ERROR_PARSE`)
- `message`: string (human readable)

**Forbidden keys**
- anything else

---

## M2.B Canonicalization rules (byte stability per line)

Every line in `trace.ndjson` MUST be canonical JSON with:

1) **UTF-8** encoding  
2) **No trailing spaces**  
3) **Exactly one `\n` newline** after each record  
4) **Object key ordering:** lexicographic ascending by Unicode code point (bytewise stable in UTF-8 for ASCII keys, which yours are)  
5) **Numbers:** serialized using the JSON parser’s canonical numeric string (no `+`, no leading zeros, no `1.0` if the value is integral).  
   - If you want to allow `1.0` distinct from `1`, then you must treat numbers as strings or forbid floats in trace. Decide now.

**Stop condition for M2 canonicalization:**
Re-canonicalizing each parsed event reproduces the exact same bytes for that line.

---

## M2.C Ordering invariants (minimal but strict)

Freeze only these:

1) **All `module_resolve` events must occur before the first non-`module_resolve` event.**  
   (This is the cleanest “prefix” rule and is easy to verify.)

2) **If an `error` event occurs, it must be the last event in trace.ndjson and must occur exactly once.**

3) **`module_graph` must occur exactly once.**  
   - For ok runs: it may be last or near-last; choose and lock.  
   - For failure runs: it may occur before `error` (your parse-fail trace already does this). Lock that: `module_graph` may appear before terminal `error`.

That’s sufficient for reconstruction and drift prevention without overspecifying.

---

## M2.D Gate suite (closure mechanism)

You close M2 with these gates, all passing in one cargo invocation:

1) **Vocabulary gate**  
   Every record `t` ∈ allowed set.

2) **Shape+types gate per event kind**  
   Required keys present, forbidden keys absent, types correct, `sha256:` regex correct where applicable.

3) **Canonicalization gate**  
   `canonicalize(parse(line)) == line` byte-for-byte (excluding the trailing `\n`, which you also lock).

4) **Ordering gate**  
   - `module_resolve` prefix rule
   - `module_graph` exactly once
   - `error` last/exactly-once-if-present

5) **Failure boundary gate (trace-level)**  
   If `digests.json.ok == false` then:
   - `trace.ndjson` contains exactly one terminal `error` event as last
   - `result.json` must be absent
   - `error.json` must be present
   If `ok == true` then:
   - `error` must be absent

---

## M2.E Verifier entrypoint (normative)

Add a verifier entrypoint that can be invoked deterministically:

- Input: `--out <dir>`
- Reads: `<dir>/trace.ndjson`, `<dir>/digests.json`
- Validates: schema + canonicalization + ordering + boundary rules
- Emits: `PASS_TRACE.txt` (or `verify_trace.json`) in the outdir
- Exit: 0 on pass, nonzero on fail

This is the “M2 is real” line.

---

# Non-circular implementation sequence (locked)

You already stated the correct sequence. Here it is as strict execution:

1) **Write spec file** (no code changes)
2) **Write gates** that enforce *exactly* the spec (tests fail immediately)
3) **Adjust runtime emission** only to satisfy those gates
4) **Run full M2 gate suite in one cargo invocation**
5) **Record distribution identity** (runtime_version, trace_format_version, stdlib_root_digest)

---

## Immediate next step: create the M2 tranche

If you want the same operational pattern as M1, the tranche is:

- `tests/intent_tranche_v0_5_m2_trace_vocab.rs`
- `tests/intent_tranche_v0_5_m2_trace_shape.rs`
- `tests/intent_tranche_v0_5_m2_trace_canon.rs`
- `tests/intent_tranche_v0_5_m2_trace_ordering.rs`
- `tests/intent_tranche_v0_5_m2_trace_boundary.rs`
- plus a minimal verifier binary test (or direct call into `src/verify/trace.rs`)

And the closure command becomes:

`cargo test -q --test intent_tranche_v0_5_m2_trace_*`

---

## One concrete decision you must lock now (no later)
**Are extra keys forbidden on trace events?**

For M2, the correct choice is: **forbid extras**.

Reason: optional keys are how drift sneaks back in without breaking positive tests. If you later need a field, you introduce **Trace Format v0.2.0**, not a “helpful optional”.

So: **required keys only, no optional keys** in M2. If you already emit extras anywhere, you remove them or version-bump the trace format.

