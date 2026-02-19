use fardlang::parse::parse_module;

#[test]
fn operators_parse_precedence_smoke() {
    // this only needs to PARSE; eval/check currently bail on operators by design.
    // precedence: * / % bind tighter than + -, comparisons, &&, ||.
    let src = r#"
module m

pub fn main() : int {
  // tokens and precedence coverage
  let a = 1 + 2 * 3
  let b = (1 + 2) * 3
  let c = 10 / 2 + 7 % 4
  let d = 1 == 1
  let e = 1 < 2 && 3 <= 4 || 5 >= 6
  a
}
"#;

    let m = parse_module(src.as_bytes()).unwrap();
    let _ = m; // parse succeeded
}
