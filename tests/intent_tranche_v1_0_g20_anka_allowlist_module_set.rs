use std::collections::BTreeSet;
use std::fs;

fn read_json(path: &str) -> serde_json::Value {
    let bytes = fs::read(path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn g20_anka_allowlist_modules_are_restricted() {
    let allow = "spec/v1_0/anka_policy_allowed_stdlib.v1.json";
    let v = read_json(allow);

    let schema = v.get("schema").and_then(|x| x.as_str()).unwrap();
    assert_eq!(schema, "fard.anka.policy.allowed_stdlib.v1");

    let mods_obj = v.get("modules").and_then(|x| x.as_object()).unwrap();

    let allowed: BTreeSet<&'static str> = [
        "std/hash",
        "std/bytes",
        "std/codec",
        "std/json",
        "std/str",
        "std/record",
        "std/list",
        "std/result",
        "std/option",
        "std/trace",
        "std/artifact",
        "std/time",
        "std/fs",
        "std/http",
        "std/schema",
    ]
    .into_iter()
    .collect();

    for (m, exports_val) in mods_obj.iter() {
        assert!(
            allowed.contains(m.as_str()),
            "g20: module not permitted in ANKA allowlist: {}",
            m
        );

        let arr = exports_val.as_array().unwrap();
        assert!(!arr.is_empty(), "g20: empty export list for module: {}", m);
        for x in arr.iter() {
            assert!(x.is_string(), "g20: non-string export in module {}", m);
        }
    }
}
