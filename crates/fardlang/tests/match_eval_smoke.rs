use fardlang::eval::Env;
use fardlang::parse::parse_module;

#[test]
fn match_eval_smoke() {
    let src = br#"
module main
pub fn main(): text {
  match 2 { 1 => "one", 2 => "two", _ => "other" }
}
"#;
    let m = parse_module(src).unwrap();
    let mut fns = std::collections::BTreeMap::new();
    for f in m.fns {
        fns.insert(f.name.clone(), f);
    }
    let decl = fns.get("main").unwrap().clone();

    let mut env = Env::with_fns(fns);
    let v = fardlang::eval::eval_block(&decl.body, &mut env).unwrap();
    assert_eq!(format!("{:?}", v), r#"Text("two")"#);
}
