# FARD v0.5 — M6 Notation Closure (Normative)

This document is normative. M6 is closed when the M6 tranche proves this document matches runtime behavior.

## A. Lexing / Tokenization
### A1. Whitespace + newline significance
### A2. Comments (forms, nesting, termination)
### A3. Identifiers (allowed chars, reserved words)
### A4. String literals (forms, escapes, invalid sequences)
### A5. Number literals (grammar, forbidden forms; align with canonicalization restrictions if applicable)

## B. Grammar (Parsing)
### B1. Expression grammar (precedence table; pipeline)
### B2. let bindings (expression-only; scope)
### B3. match / patterns / guards
### B4. function forms (if present) / call syntax
### B5. module syntax (import forms, aliasing, resolution surface)

## C. Evaluation (Normative)
### C1. Evaluation order (strictness; left-to-right)
### C2. let scope + shadowing
### C3. match evaluation + bind + guard evaluation order
### C4. ? operator semantics (unwind shape, propagation)
### C5. pipeline insertion semantics (exact rewrite rule)

## D. Meaning → Ledger Mapping
For each effect boundary primitive, define:
- trace.ndjson events required
- bundle files required
- verifier guarantees relied upon (M2/M5)

### D1. trace emission rules
### D2. module_graph.json emission rules
### D3. artifact_graph.json (if in play)
### D4. digests.json + distribution identity tuple (references M5)

## E. Versioning / No-breaking-change Rule
### E1. What increments runtime_version vs trace_format_version vs stdlib_root_digest
### E2. (Optional) language_version definition and when it increments

