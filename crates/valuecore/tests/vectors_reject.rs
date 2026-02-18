use valuecore::{dec, enc, Value};

#[test]
fn dec_rejects_unknown_tag() {
    let b = br#"{"t":"wat","v":null}"#;
    let e = dec(b).unwrap_err();
    assert!(e.code.contains("DECODE_UNKNOWN_T"), "got {}", e.code);
}

#[test]
fn dec_rejects_duplicate_keys_in_record_value_space() {
    // Duplicate "a" in the pair-array list MUST be rejected by decoder
    let b = br#"{"t":"record","v":[["a",{"t":"unit"}],["a",{"t":"unit"}]]}"#;
    let e = dec(b).unwrap_err();
    assert!(e.code.contains("DECODE_DUP_KEY"), "got {}", e.code);
}

#[test]
fn dec_rejects_bad_int_strings() {
    for b in [
        br#"{"t":"int","v":"-0"}"#.as_slice(),
        br#"{"t":"int","v":"00"}"#.as_slice(),
        br#"{"t":"int","v":"01"}"#.as_slice(),
        br#"{"t":"int","v":"+"}"#.as_slice(),
        br#"{"t":"int","v":" 1"}"#.as_slice(),
    ] {
        let e = dec(b).unwrap_err();
        assert!(e.code.contains("DECODE_BAD_INT") || e.code.contains("DECODE_BAD_KEYS"), "got {}", e.code);
    }
}

#[test]
fn dec_rejects_bad_hex_or_uppercase() {
    for b in [
        br#"{"t":"bytes","v":"0"}"#.as_slice(),
        br#"{"t":"bytes","v":"0g"}"#.as_slice(),
        br#"{"t":"bytes","v":"AA"}"#.as_slice(),
    ] {
        let e = dec(b).unwrap_err();
        assert!(e.code.contains("DECODE_BAD_HEX") || e.code.contains("DECODE_BAD_KEYS"), "got {}", e.code);
    }
}

#[test]
fn dec_roundtrip_for_simple_values() {
    let v = Value::Text("hi\n".to_string());
    let b = enc(&v);
    let v2 = dec(&b).unwrap();
    assert_eq!(v, v2);
}
