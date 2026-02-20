use fardlang::parse::parse_module;

#[test]
fn record_literal_and_field_access_parse_smoke() {
    let src = br#"
module m

pub fn main() : Value {
  let x = {b: 2, a: 1}
  let y = x.a
  y
}
"#;

    let _m = parse_module(src).unwrap();
}
