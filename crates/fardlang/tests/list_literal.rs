use fardlang::canon::print_expr_public as print_expr;
use fardlang::eval::{eval_block, Env};
use fardlang::{check, parse_module};
use valuecore::v0::V;

#[test]
fn list_literal_parses_canon_and_evals() {
    let src = br#"
module main

fn main(): Value {
  let x = [1, 2, add(3, 4)]
  let y = [x, [unit, true, false]]
  y
}
"#;

    let module = parse_module(src).unwrap();
    check::check_module(&module).unwrap();

    // Canon check: tail is ident `y`
    let main_fn = module.fns.iter().find(|f| f.name == "main").unwrap();
    let tail = main_fn.body.tail.as_ref().unwrap();
    let s = print_expr(tail);
    assert_eq!(s, "y");

    // Eval check
    let mut env = Env::new();
    env.max_depth = 256;
    for f in &module.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let out = eval_block(&main_fn.body, &mut env).unwrap();

    // Expect: y = [x, [unit, true, false]] where x = [1,2,7]
    match out {
        V::List(vs) => {
            assert_eq!(vs.len(), 2);

            match &vs[0] {
                V::List(xs) => assert_eq!(xs, &vec![V::Int(1), V::Int(2), V::Int(7)]),
                _ => panic!("expected x to be a list"),
            }

            match &vs[1] {
                V::List(ys) => assert_eq!(ys, &vec![V::Unit, V::Bool(true), V::Bool(false)]),
                _ => panic!("expected second element to be a list"),
            }
        }
        _ => panic!("expected outer value to be a list"),
    }
}
