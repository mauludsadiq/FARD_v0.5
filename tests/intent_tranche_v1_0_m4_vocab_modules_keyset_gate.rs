use std::fs;

#[test]
fn m4_vocab_modules_keyset_is_stable() {
    let p = "spec/stdlib_surface.v1_0.ontology.json";
    let s = fs::read_to_string(p).expect("read ontology json");
    let v: serde_json::Value = serde_json::from_str(&s).expect("ontology must be valid JSON");
    let root = v.as_object().expect("ontology root must be object");

    let modules = root
        .get("modules")
        .expect("ontology must have modules")
        .as_object()
        .expect("modules must be object");

    let mut names: Vec<String> = modules.keys().cloned().collect();
    names.sort();

    // Freeze this list to repo-truth once printed.
    // Start by running the test once and copying the printed list into `want`.
    let want: Vec<String> = vec![
        "std/artifact".to_string(),
        "std/bytes".to_string(),
        "std/codec".to_string(),
        "std/env".to_string(),
        "std/flow".to_string(),
        "std/fs".to_string(),
        "std/grow".to_string(),
        "std/hash".to_string(),
        "std/http".to_string(),
        "std/int".to_string(),
        "std/json".to_string(),
        "std/list".to_string(),
        "std/map".to_string(),
        "std/option".to_string(),
        "std/record".to_string(),
        "std/result".to_string(),
        "std/str".to_string(),
        "std/time".to_string(),
        "std/trace".to_string(),
    ];
    assert_eq!(
        names, want,
        "modules keyset changed; update want to repo-truth"
    );
}
