use num_bigint::BigInt;
use pretty_assertions::assert_eq;
use valuecore::{vdig, Value};

#[test]
fn vdig_reference_hashes() {
    // From your frozen spec appendix
    let unit = Value::Unit;
    assert_eq!(
        vdig(&unit),
        "sha256:91e321035af75af8327b2d94d23e1fa73cfb5546f112de6a65e494645148a3ea"
    );

    let hello = Value::Bytes(b"hello".to_vec());
    assert_eq!(
        vdig(&hello),
        "sha256:4a8661598853a17a123957153c2ca6d1b690010ea3e774f60b6654325b6915ce"
    );

    let rec = Value::record(vec![
        ("a".to_string(), Value::Int(BigInt::from(1))),
        ("b".to_string(), Value::Int(BigInt::from(2))),
    ]);
    assert_eq!(
        vdig(&rec),
        "sha256:9d9aad0e20a4852a66077c456fc848416c55b3fba757cd38dc5f7b86c47e2067"
    );
}
