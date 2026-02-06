.PHONY: regen-stage gate-stage

regen-stage:
tools/regen_ontology_stage_pipe.sh

gate-stage:
cargo test -q stdlib_surface_ontology_gate_v1 -- --nocapture
