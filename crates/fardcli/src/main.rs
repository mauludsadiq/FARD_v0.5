use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args[1] != "run" {
        eprintln!("usage: fard run <file.fard>");
        process::exit(1);
    }

    let path = &args[2];
    let src = fs::read(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {}", path, e);
        process::exit(1);
    });

    let module = fardlang::parse_module(&src).unwrap_or_else(|e| {
        eprintln!("parse error: {}", e);
        process::exit(1);
    });

    fardlang::check::check_module(&module).unwrap_or_else(|e| {
        eprintln!("check error: {}", e);
        process::exit(1);
    });

    let main_fn = module
        .fns
        .iter()
        .find(|f| f.name == "main")
        .unwrap_or_else(|| {
            eprintln!("error: no main function");
            process::exit(1);
        });

    let mut env = fardlang::eval::Env::new();
    for f in &module.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let v = fardlang::eval::eval_block(&main_fn.body, &mut env).unwrap_or_else(|e| {
        eprintln!("eval error: {}", e);
        process::exit(1);
    });

    let bytes = valuecore::v0::encode_json(&v);
    print!("{}", String::from_utf8(bytes).unwrap());
}
