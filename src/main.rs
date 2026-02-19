use std::env;
use std::fs;

fn main() {
    let mut args = env::args().skip(1);

    let cmd = args.next().expect("usage: fard run <file.fard>");
    if cmd != "run" {
        panic!("only `run` supported");
    }

    let path = args.next().expect("usage: fard run <file.fard>");

    let src = fs::read(&path).expect("read failed");

    let module = fardlang::parse_module(&src).expect("parse failed");
    fardlang::check::check_module(&module).expect("check failed");

    let main_fn = module.fns.iter()
        .find(|f| f.name == "main")
        .expect("no main function");

    let mut env = fardlang::eval::Env::new();
    for f in &module.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let v = fardlang::eval::eval_block(&main_fn.body, &mut env)
        .expect("eval failed");

    let bytes = valuecore::v0::encode_json(&v);
    print!("{}", String::from_utf8(bytes).unwrap());
}
