use std::fs;

#[test]
fn m4_vocab_ontology_top_level_keyset_is_stable() {
    let p = "spec/stdlib_surface.v1_0.ontology.json";
    let s = fs::read_to_string(p).expect("read ontology json");
    let v: serde_json::Value = serde_json::from_str(&s).expect("ontology must be valid JSON");
    let obj = v.as_object().expect("ontology root must be object");

    let mut keys: Vec<String> = obj.keys().cloned().collect();
    keys.sort();

    // Adjust this list to the exact root keyset you intend to freeze for 1.0.
    // The point: no surprise keys can appear without changing the gate.
    let want: Vec<String> = vec!["modules".to_string(), "schema".to_string()];

    assert_eq!(keys, want, "ontology root keyset changed");
}
