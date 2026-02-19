use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

fn read_json(path: &str) -> serde_json::Value {
    let bytes = fs::read(path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn surface_entries_as_map(surface_entries_path: &str) -> BTreeMap<String, BTreeSet<String>> {
    let v = read_json(surface_entries_path);

    let schema = v.get("schema").and_then(|x| x.as_str()).unwrap();
    assert_eq!(schema, "fard.stdlib_surface.entries.v1_0");

    let entries = v.get("entries").and_then(|x| x.as_array()).unwrap();

    let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for e in entries {
        let module = e
            .get("module")
            .and_then(|x| x.as_str())
            .unwrap()
            .to_string();
        let export = e
            .get("export")
            .and_then(|x| x.as_str())
            .unwrap()
            .to_string();
        m.entry(module).or_default().insert(export);
    }

    m
}

fn allowlist_as_map(allowlist_path: &str) -> BTreeMap<String, BTreeSet<String>> {
    let v = read_json(allowlist_path);

    let schema = v.get("schema").and_then(|x| x.as_str()).unwrap();
    assert_eq!(schema, "fard.anka.policy.allowed_stdlib.v1");

    let modules = v.get("modules").and_then(|x| x.as_object()).unwrap();

    let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (module, exports_val) in modules.iter() {
        let arr = exports_val.as_array().unwrap();
        let mut set: BTreeSet<String> = BTreeSet::new();
        for x in arr {
            set.insert(x.as_str().unwrap().to_string());
        }
        m.insert(module.to_string(), set);
    }

    m
}

#[test]
fn g19_anka_allowlist_is_subset_of_surface_entries() {
    let surface = "ontology/stdlib_surface.v1_0.ontology.json";
    let allow = "spec/v1_0/anka_policy_allowed_stdlib.v1.json";

    let surface_map = surface_entries_as_map(surface);
    let allow_map = allowlist_as_map(allow);

    for (m, allow_ex) in allow_map.iter() {
        let surf_ex = surface_map
            .get(m)
            .unwrap_or_else(|| panic!("g19: module not in surface: {}", m));
        for ex in allow_ex.iter() {
            assert!(
                surf_ex.contains(ex),
                "g19: export not in surface: module={} export={}",
                m,
                ex
            );
        }
    }

    for (m, exs) in allow_map.iter() {
        assert!(!exs.is_empty(), "g19: empty module in allowlist: {}", m);
    }

    let allow_v = read_json(allow);
    let src = allow_v.get("source").and_then(|x| x.as_str()).unwrap();
    assert_eq!(src, surface);

    let _ = PathBuf::from(allow);
}
