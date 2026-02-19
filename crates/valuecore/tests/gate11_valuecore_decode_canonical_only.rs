use valuecore::v0;

fn must_err(bytes: &[u8]) -> String {
    v0::decode_json(bytes).unwrap_err().to_string()
}

#[test]
fn gate11_rejects_uppercase_hex_bytes() {
    let e = must_err(br#"{"t":"bytes","v":"hex:FF"}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);
}

#[test]
fn gate11_rejects_unsorted_map_vector() {
    let e = must_err(br#"{"t":"map","v":[["b",{"t":"int","v":2}],["a",{"t":"int","v":1}]]}"#);
    assert!(
        e.contains("non-canonical map order") || e.contains("ERROR_JSON"),
        "expected non-canonical order error: {}",
        e
    );
}

#[test]
fn gate11_rejects_duplicate_map_keys() {
    let e = must_err(br#"{"t":"map","v":[["a",{"t":"int","v":1}],["a",{"t":"int","v":2}]]}"#);
    assert!(
        e.contains("duplicate map key") || e.contains("ERROR_JSON"),
        "expected duplicate key error: {}",
        e
    );
}

#[test]
fn gate11_accepts_canonical_map_vector() {
    let v =
        v0::decode_json(br#"{"t":"map","v":[["a",{"t":"int","v":1}],["b",{"t":"int","v":2}]]}"#)
            .unwrap();
    let j = v0::encode_json(&v);
    assert_eq!(
        std::str::from_utf8(&j).unwrap(),
        r#"{"t":"map","v":[["a",{"t":"int","v":1}],["b",{"t":"int","v":2}]]}"#
    );
}
