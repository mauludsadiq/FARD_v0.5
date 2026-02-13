use std::collections::{BTreeMap, BTreeSet};
use std::fs;

#[path = "../src/verify/ontology_verify.rs"]
mod ontology_verify;

fn load_builtin_index_from_runtime() -> BTreeMap<String, BTreeSet<String>> {
  // This assumes you already have runtime export maps in one of these common locations.
  // If your repo uses a different path/name, adjust the include path ONLY here.
  //
  // Expected shape:
  //   pub static BUILTIN_STD_EXPORTS: &[(&str, &[&str])];
  //
  // Example:
  //   ("std/result", &["ok","err", ...])
  //
  #[path = "../src/builtin_std.rs"]
  mod builtin_std;

  let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
  for (m, ex) in builtin_std::BUILTIN_STD_EXPORTS.iter() {
    let mut s: BTreeSet<String> = BTreeSet::new();
    for e in (*ex).iter() { s.insert((*e).to_string()); }
    out.insert((*m).to_string(), s);
  }
  out
}

#[test]
fn m4_builtin_satisfies_manifest() {
  let bytes = fs::read("spec/ontology_surface_v0_5_m4.json").expect("READ_MANIFEST_FAIL");
  let (_v, mods) = ontology_verify::parse_manifest_bytes(&bytes).expect("MANIFEST_PARSE_FAIL");
  ontology_verify::require_minimum_surface(&mods).expect("M4_MINIMUM_FAIL");

  let builtin = load_builtin_index_from_runtime();
  ontology_verify::assert_builtin_satisfies_manifest(&mods, &builtin).expect("M4_BUILTIN_SURFACE_FAIL");
}
