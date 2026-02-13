use std::collections::{BTreeMap, BTreeSet};
use std::fs;

#[path = "../src/verify/ontology_verify.rs"]
mod ontology_verify;

#[test]
fn m4_negative_missing_module_fails() {
    let bytes = fs::read("spec/ontology_surface_v0_5_m4.json").expect("READ_MANIFEST_FAIL");
    let (_v, mods) = ontology_verify::parse_manifest_bytes(&bytes).expect("MANIFEST_PARSE_FAIL");

    let mut builtin: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (m, ex) in mods.iter() {
        let mut s: BTreeSet<String> = BTreeSet::new();
        for e in ex.iter() {
            s.insert(e.to_string());
        }
        builtin.insert(m.to_string(), s);
    }

    builtin.remove("std/http");

    let r = ontology_verify::assert_builtin_satisfies_manifest(&mods, &builtin);
    assert!(r.is_err(), "EXPECTED_ERR");
    let msg = r.err().unwrap();
    assert!(
        msg.contains("M4_BUILTIN_MISSING_MODULE std/http"),
        "WRONG_ERR {}",
        msg
    );
}
