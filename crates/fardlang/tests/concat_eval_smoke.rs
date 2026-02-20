use fardlang::eval::{eval_block, Env};
use fardlang::{check, parse_module};
use valuecore::v0::V;

#[test]
fn concat_operator_eval_smoke() {
    let src = br#"
module m

pub fn main() : Value {
  let s = "a" ++ "b"
  s
}
"#;

    let m = parse_module(src).unwrap();
    check::check_module(&m).unwrap();

    let main_fn = m.fns.iter().find(|f| f.name == "main").unwrap();
    let mut env = Env::new();
    env.max_depth = 256;
    for f in &m.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let out = eval_block(&main_fn.body, &mut env).unwrap();
    assert_eq!(out, V::Text("ab".to_string()));
}
