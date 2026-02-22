use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args[1] != "run" {
        eprintln!("usage: fard run <file.fard> [--input key=value ...]");
        process::exit(1);
    }

    let path = &args[2];

    // parse --input key=value pairs
    let mut inputs: Vec<(String, String)> = vec![];
    let mut i = 3;
    while i < args.len() {
        if args[i] == "--input" && i + 1 < args.len() {
            let kv = &args[i + 1];
            if let Some(eq) = kv.find('=') {
                inputs.push((kv[..eq].to_string(), kv[eq+1..].to_string()));
            }
            i += 2;
        } else {
            i += 1;
        }
    }
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
    fardlang::eval::apply_imports(&mut env, &module.imports);
    // bind --input values
    for (k, v) in &inputs {
        let val = if let Ok(n) = v.parse::<i64>() {
            valuecore::v0::V::Int(n)
        } else if v == "true" {
            valuecore::v0::V::Bool(true)
        } else if v == "false" {
            valuecore::v0::V::Bool(false)
        } else {
            valuecore::v0::V::Text(v.clone())
        };
        env.bindings.push((k.clone(), fardlang::eval::EvalVal::V(val)));
    }
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
