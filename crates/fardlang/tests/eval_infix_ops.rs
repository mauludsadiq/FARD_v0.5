use fardlang::eval::{eval_block, Env};
use fardlang::{check, parse_module};
use valuecore::v0::V;

#[test]
fn eval_infix_ops_precedence_and_unary_minus() {
    let src = br#"
module m

pub fn main() : Value {
  let a = 1 + 2 * 3
  let b = (1 + 2) * 3
  let c = -1 + 2
  [a, b, c]
}
"#;

    let module = parse_module(src).unwrap();
    check::check_module(&module).unwrap();

    let main_fn = module.fns.iter().find(|f| f.name == "main").unwrap();

    let mut env = Env::new();
    env.max_depth = 256;
    for f in &module.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let out = eval_block(&main_fn.body, &mut env).unwrap();

    match out {
        V::List(vs) => {
            assert_eq!(vs, vec![V::Int(7), V::Int(9), V::Int(1)]);
        }
        _ => panic!("expected list output, got {out:?}"),
    }
}
