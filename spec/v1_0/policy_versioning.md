# Versioning Policy: Semantic Freeze

This document is normative for what changes require a version bump.

## 1. Semantic changes (require version bump)
A change is semantic if it can change any of:
- parsing acceptance/rejection
- evaluation behavior or order
- result.json shape rules (M1)
- trace.ndjson schema/vocabulary/canonicalization (M2)
- artifact causality graph rules (M3)
- stdlib surface / ontology / manifest alignment rules (M4)
- bundle digest computation or required files (M5)
- golden bundle byte outputs

## 2. Non-semantic changes (no bump)
Allowed without bump:
- docs changes that do not contradict gates
- adding tests that assert already-current behavior
- refactors that preserve byte-identical outputs under the same distribution identity

## 3. Version bump procedure
A bump must include:
- updated version identifiers
- regenerated golden bundle for the new version path
- updated spec docs as needed
- all gates green
