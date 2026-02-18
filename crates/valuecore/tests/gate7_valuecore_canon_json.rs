use valuecore::v0::{self, V};

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

#[test]
fn gate7_valuecore_canon_json_vectors_and_roundtrip() {
    let vectors: Vec<(V, &'static str)> = vec![
        (V::Unit, r#"{"t":"unit"}"#),
        (V::Bool(false), r#"{"t":"bool","v":false}"#),
        (V::Bool(true), r#"{"t":"bool","v":true}"#),
        (V::Int(0), r#"{"t":"int","v":0}"#),
        (V::Int(-7), r#"{"t":"int","v":-7}"#),
        (V::Text("".to_string()), r#"{"t":"text","v":""}"#),
        (V::Text("a".to_string()), r#"{"t":"text","v":"a"}"#),
        (V::Text("a\nb".to_string()), r#"{"t":"text","v":"a\nb"}"#),
        (V::Bytes(vec![]), r#"{"t":"bytes","v":"hex:"}"#),
        (V::Bytes(vec![0, 255, 16]), r#"{"t":"bytes","v":"hex:00ff10"}"#),
        (V::List(vec![]), r#"{"t":"list","v":[]}"#),
        (V::List(vec![V::Unit, V::Int(1)]), r#"{"t":"list","v":[{"t":"unit"},{"t":"int","v":1}]}"#),
        (
            
    V::Map(vec![
                ("a".to_string(), V::Int(1)),
                ("b".to_string(), V::Int(2)),
            ]),
            r#"{"t":"map","v":[["a",{"t":"int","v":1}],["b",{"t":"int","v":2}]]}"#,
        ),
        (V::Ok(Box::new(V::Int(9))), r#"{"t":"ok","v":{"t":"int","v":9}}"#),
        (V::Err("E1".to_string()), r#"{"t":"err","e":"E1"}"#),
    ];

    for (v, expect) in vectors {
        let enc = v0::encode_json(&v);
        assert_eq!(s(&enc), expect, "encode mismatch");
        let dec = v0::decode_json(&enc).expect("decode must succeed");
        assert_eq!(dec, v, "roundtrip mismatch");
        let enc2 = v0::encode_json(&dec);
        assert_eq!(enc2, enc, "re-encode mismatch");
    }
}

#[test]
fn gate7_i64_overflow_is_error_overflow() {
    let e = v0::i64_add(i64::MAX, 1).unwrap_err().to_string();
    assert!(e.contains("ERROR_OVERFLOW"), "missing ERROR_OVERFLOW: {}", e);

    let e = v0::i64_sub(i64::MIN, 1).unwrap_err().to_string();
    assert!(e.contains("ERROR_OVERFLOW"), "missing ERROR_OVERFLOW: {}", e);

    let e = v0::i64_mul(i64::MAX, 2).unwrap_err().to_string();
    assert!(e.contains("ERROR_OVERFLOW"), "missing ERROR_OVERFLOW: {}", e);
}
