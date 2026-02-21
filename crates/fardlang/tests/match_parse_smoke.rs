use fardlang::parse::parse_module;

#[test]
fn match_parse_smoke() {
    let src = br#"
module m

pub fn main() : Value {
  let x = "a"
  let y = match x { "a" => "ok", _ => "no" }
  y
}
"#;

    let _m = parse_module(src).unwrap();
}
