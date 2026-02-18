use valuecore::v0;

fn must_err(bytes: &[u8]) -> String {
    v0::decode_json(bytes).unwrap_err().to_string()
}

#[test]
fn gate9_rejects_missing_fields_and_unknown_tags() {
    let e = must_err(br#"{}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"int"}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"nope","v":0}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);
}

#[test]
fn gate9_rejects_wrong_types() {
    let e = must_err(br#"{"t":"int","v":"7"}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"bool","v":"true"}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"text","v":7}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"list","v":{}}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"map","v":{}}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);
}

#[test]
fn gate9_rejects_bytes_bad_hex() {
    let e = must_err(br#"{"t":"bytes","v":"00ff"}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"bytes","v":"hex:0"}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"bytes","v":"hex:zz"}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);
}

#[test]
fn gate9_rejects_map_bad_pairs() {
    let e = must_err(br#"{"t":"map","v":[["a"]]}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"map","v":[["a",{"t":"int","v":1},"extra"]]}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);

    let e = must_err(br#"{"t":"map","v":[[7,{"t":"int","v":1}]]}"#);
    assert!(e.contains("ERROR_JSON"), "expected ERROR_JSON: {}", e);
}
