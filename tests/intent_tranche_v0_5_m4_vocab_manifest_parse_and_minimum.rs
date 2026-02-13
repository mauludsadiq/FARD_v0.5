use std::fs;

#[path = "../src/verify/ontology_verify.rs"]
mod ontology_verify;

#[test]
fn m4_manifest_parse_and_minimum() {
    let bytes = fs::read("spec/ontology_surface_v0_5_m4.json").expect("READ_MANIFEST_FAIL");
    let (_v, mods) = ontology_verify::parse_manifest_bytes(&bytes).expect("MANIFEST_PARSE_FAIL");
    ontology_verify::require_minimum_surface(&mods).expect("M4_MINIMUM_FAIL");
}
