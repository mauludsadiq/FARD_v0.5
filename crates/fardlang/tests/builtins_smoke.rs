use fardlang::eval::{eval_block, Env};
use fardlang::{check, parse_module};
use valuecore::v0::V;

#[test]
fn builtins_list_text_int_work() {
    let src = br#"
module main

fn main(): Value {
  let xs = [10, 20, 30]
  let a = list_len(xs)
  let b = list_get(xs, 1)
  let c = text_concat("hi", "there")
  let d = int_to_text(42)
  if eq(a, 3) {
    [a, b, c, d]
  } else {
    unit
  }
}
"#;

    let module = parse_module(src).unwrap();
    check::check_module(&module).unwrap();

    let main_fn = module.fns.iter().find(|f| f.name == "main").unwrap();

    let mut env = Env::new();
    for f in &module.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let out = eval_block(&main_fn.body, &mut env).unwrap();

    match out {
        V::List(vs) => {
            assert_eq!(vs.len(), 4);
            assert_eq!(vs[0], V::Int(3));
            assert_eq!(vs[1], V::Int(20));
            assert_eq!(vs[2], V::Text("hithere".into()));
            assert_eq!(vs[3], V::Text("42".into()));
        }
        _ => panic!("expected list"),
    }
}
