.PHONY: regen-stage gate-stage cg1

regen-stage:
	tools/regen_ontology_stage_pipe.sh

gate-stage:
	cargo test -q stdlib_surface_ontology_gate_v1 -- --nocapture
	cargo test -q --test intent_tranche_v1_0_cg1_color_geometry -- --nocapture

cg1:
	cargo test -q --test intent_tranche_v1_0_cg1_color_geometry -- --nocapture
