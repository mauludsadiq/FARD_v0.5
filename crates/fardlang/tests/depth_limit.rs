use fardlang::eval::{eval_block, Env};
use fardlang::{check, parse_module};

#[test]
fn recursion_depth_limit_triggers_error() {
    let src = br#"
module main

fn loop(n: int): int {
  loop(n)
}

fn main(): int {
  loop(0)
}
"#;

    let module = parse_module(src).unwrap();
    check::check_module(&module).unwrap();

    let main_fn = module.fns.iter().find(|f| f.name == "main").unwrap();

    let mut env = Env::new();
    env.max_depth = 64;
    for f in &module.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let err = eval_block(&main_fn.body, &mut env).unwrap_err();
    assert!(err.to_string().contains("ERROR_EVAL_DEPTH"));
}
