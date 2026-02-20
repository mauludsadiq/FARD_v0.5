use fardlang::parse::parse_module;

#[test]
fn concat_operator_parse_smoke() {
    let src = br#"
module m

pub fn main() : Value {
  let s = "a" ++ "b"
  s
}
"#;

    let _m = parse_module(src).unwrap();
}
