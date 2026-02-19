use num_bigint::BigInt;
use pretty_assertions::assert_eq;
use valuecore::{enc, Value};

fn s(x: &str) -> String {
    x.to_string()
}

#[test]
fn enc_unit() {
    let v = Value::Unit;
    assert_eq!(String::from_utf8(enc(&v)).unwrap(), r#"{"t":"unit"}"#);
}

#[test]
fn enc_bool() {
    assert_eq!(
        String::from_utf8(enc(&Value::Bool(false))).unwrap(),
        r#"{"t":"bool","v":false}"#
    );
    assert_eq!(
        String::from_utf8(enc(&Value::Bool(true))).unwrap(),
        r#"{"t":"bool","v":true}"#
    );
}

#[test]
fn enc_int_canonical_string() {
    let v = Value::Int(BigInt::from(0));
    assert_eq!(
        String::from_utf8(enc(&v)).unwrap(),
        r#"{"t":"int","v":"0"}"#
    );

    let v2 = Value::Int(BigInt::from(-12));
    assert_eq!(
        String::from_utf8(enc(&v2)).unwrap(),
        r#"{"t":"int","v":"-12"}"#
    );
}

#[test]
fn enc_bytes_hex_lower() {
    let v = Value::Bytes(vec![0x00, 0xff]);
    assert_eq!(
        String::from_utf8(enc(&v)).unwrap(),
        r#"{"t":"bytes","v":"00ff"}"#
    );
}

#[test]
fn enc_text_escaping_single_form() {
    let v = Value::Text("a\"b\\c\n".to_string());
    assert_eq!(
        String::from_utf8(enc(&v)).unwrap(),
        r#"{"t":"text","v":"a\"b\\c\n"}"#
    );

    let v2 = Value::Text("\u{0001}".to_string());
    assert_eq!(
        String::from_utf8(enc(&v2)).unwrap(),
        r#"{"t":"text","v":"\u0001"}"#
    );
}

#[test]
fn enc_list_nesting() {
    let v = Value::List(vec![Value::Int(BigInt::from(1))]);
    assert_eq!(
        String::from_utf8(enc(&v)).unwrap(),
        r#"{"t":"list","v":[{"t":"int","v":"1"}]}"#
    );
}

#[test]
fn enc_record_pair_array_sorted() {
    // input kv order b then a; Value::record sorts by UTF-8 order
    let v = Value::record(vec![
        (s("b"), Value::Int(BigInt::from(2))),
        (s("a"), Value::Int(BigInt::from(1))),
    ]);
    assert_eq!(
        String::from_utf8(enc(&v)).unwrap(),
        r#"{"t":"record","v":[["a",{"t":"int","v":"1"}],["b",{"t":"int","v":"2"}]]}"#
    );
}

#[test]
fn record_constructor_duplicate_key_yields_err() {
    let v = Value::record(vec![
        (s("a"), Value::Int(BigInt::from(1))),
        (s("a"), Value::Int(BigInt::from(2))),
    ]);

    assert_eq!(
        String::from_utf8(enc(&v)).unwrap(),
        r#"{"t":"err","v":{"code":"ERROR_DUP_KEY","data":{"t":"record","v":[["key",{"t":"text","v":"a"}],["value",{"t":"unit"}]]}}}"#
    );
}

#[test]
fn enc_err_shape() {
    let v = Value::err(
        "ERROR_INEXACT_DIV",
        Value::record(vec![
            (s("x"), Value::Int(BigInt::from(1))),
            (s("y"), Value::Int(BigInt::from(2))),
        ]),
    );

    assert_eq!(
        String::from_utf8(enc(&v)).unwrap(),
        r#"{"t":"err","v":{"code":"ERROR_INEXACT_DIV","data":{"t":"record","v":[["x",{"t":"int","v":"1"}],["y",{"t":"int","v":"2"}]]}}}"#
    );
}
