FARD 1.0 GOLDEN BUNDLE (CANONICAL ARTIFACT)

This directory holds a canonical run bundle whose bytes are used as a stop condition.
The spec references this bundle as the byte-level anchor for:
- bundle file set rules
- digests schema
- canonical JSON preimage canonicalization
- module_graph.json emission and binding

Files expected in this directory (normative file set)
- program.fard
- result.json
- trace.ndjson
- module_graph.json
- digests.json
- run_bundle.cid.txt or equivalent CID binding file (if used by the repo)
- artifact_graph.json and artifact bytes (if the golden includes artifacts)

Generation and verification are performed by:
- bash tools/gen_golden_bundle_v1.sh
- bash tools/verify_golden_bundle_v1.sh
