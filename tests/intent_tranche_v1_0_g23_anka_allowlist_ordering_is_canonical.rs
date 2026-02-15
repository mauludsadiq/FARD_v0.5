use std::collections::BTreeSet;
use std::fs;

fn read_json(path: &str) -> serde_json::Value {
    let bytes = fs::read(path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn is_strictly_sorted(xs: &[String]) -> bool {
    xs.windows(2).all(|w| w[0] < w[1])
}

#[test]
fn g23_anka_allowlist_has_frozen_shape_and_sorted_keys_and_exports() {
    let path = "spec/v1_0/anka_policy_allowed_stdlib.v1.json";
    let v = read_json(path);

    let obj = v.as_object().unwrap();

    let keys: Vec<String> = obj.keys().cloned().collect();
    let keyset: BTreeSet<String> = keys.iter().cloned().collect();

    let expected: BTreeSet<String> = ["schema", "source", "modules"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(
        keyset, expected,
        "g23: top-level keys must be exactly schema, source, modules (no extras)"
    );

    let schema = obj.get("schema").and_then(|x| x.as_str()).unwrap();
    assert_eq!(schema, "fard.anka.policy.allowed_stdlib.v1");

    let source = obj.get("source").and_then(|x| x.as_str()).unwrap();
    assert_eq!(source, "ontology/stdlib_surface.v1_0.ontology.json");

    let modules = obj.get("modules").and_then(|x| x.as_object()).unwrap();

    let mod_keys: Vec<String> = modules.keys().cloned().collect();
    let mut mod_keys_sorted = mod_keys.clone();
    mod_keys_sorted.sort();

    assert_eq!(
        mod_keys, mod_keys_sorted,
        "g23: modules keys must be sorted ascending"
    );

    for (m, exports_val) in modules.iter() {
        let arr = exports_val.as_array().unwrap();
        assert!(!arr.is_empty(), "g23: empty exports for module {}", m);

        let mut exports: Vec<String> = Vec::with_capacity(arr.len());
        for x in arr.iter() {
            exports.push(x.as_str().unwrap().to_string());
        }

        let mut exports_sorted = exports.clone();
        exports_sorted.sort();

        assert_eq!(
            exports, exports_sorted,
            "g23: exports must be sorted ascending for module {}",
            m
        );

        let set: BTreeSet<String> = exports.iter().cloned().collect();
        assert_eq!(
            set.len(),
            exports.len(),
            "g23: duplicate export in module {}",
            m
        );

        assert!(
            is_strictly_sorted(&exports),
            "g23: exports must be strictly increasing (no duplicates) for module {}",
            m
        );
    }
}
