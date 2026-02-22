use std::env;
use std::fs;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};
use valuecore::v0::V;
use fardlang::effects::{EffectHandler, EffectTrace};
use anyhow::{anyhow, Result};

struct StdEffectHandler {
    traces: Vec<EffectTrace>,
}

impl StdEffectHandler {
    fn new() -> Self { Self { traces: vec![] } }
}

impl EffectHandler for StdEffectHandler {
    fn call(&mut self, name: &str, args: &[V]) -> Result<V> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
        let result = match name {
            "read_file" => {
                let path = match args.first() { Some(V::Text(s)) => s.clone(), _ => return Err(anyhow!("read_file expects text path")) };
                let bytes = fs::read(&path).map_err(|e| anyhow!("read_file {}: {}", path, e))?;
                let s = String::from_utf8(bytes).map_err(|e| anyhow!("read_file utf8: {}", e))?;
                let v: serde_json::Value = serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s));
                json_to_v(&v)
            }
            "write_file" => {
                let path = match args.first() { Some(V::Text(s)) => s.clone(), _ => return Err(anyhow!("write_file expects text path")) };
                let data = args.get(1).cloned().unwrap_or(V::Unit);
                let bytes = valuecore::v0::encode_json(&data);
                fs::write(&path, &bytes).map_err(|e| anyhow!("write_file {}: {}", path, e))?;
                V::Bool(true)
            }
            "clock_now" => {
                V::Int(ts as i64)
            }
            "random_bytes" => {
                let n = match args.first() { Some(V::Int(n)) => *n as usize, _ => return Err(anyhow!("random_bytes expects int")) };
                use rand::RngCore;
                let mut buf = vec![0u8; n];
                rand::thread_rng().fill_bytes(&mut buf);
                V::Bytes(buf)
            }
            "http_get" => {
                let url = match args.first() { Some(V::Text(s)) => s.clone(), _ => return Err(anyhow!("http_get expects text url")) };
                let body = ureq::get(&url).call().map_err(|e| anyhow!("http_get {}: {}", url, e))?.into_string().map_err(|e| anyhow!("http_get body: {}", e))?;
                V::Text(body)
            }
            other => return Err(anyhow!("ERROR_EFFECT unknown effect {}", other)),
        };
        self.traces.push(EffectTrace { name: name.to_string(), args: args.to_vec(), result: result.clone(), timestamp_ms: ts });
        Ok(result)
    }
    fn trace(&self) -> &[EffectTrace] { &self.traces }
}

fn json_to_v(j: &serde_json::Value) -> V {
    match j {
        serde_json::Value::Null => V::Unit,
        serde_json::Value::Bool(b) => V::Bool(*b),
        serde_json::Value::Number(n) => if let Some(i) = n.as_i64() { V::Int(i) } else { V::Text(n.to_string()) },
        serde_json::Value::String(s) => V::Text(s.clone()),
        serde_json::Value::Array(a) => V::List(a.iter().map(json_to_v).collect()),
        serde_json::Value::Object(o) => V::Map(o.iter().map(|(k,v)| (k.clone(), json_to_v(v))).collect()),
    }
}

fn main() {

fn json_to_v_json(v: &V) -> serde_json::Value {
    match v {
        V::Unit        => serde_json::Value::Null,
        V::Bool(b)     => serde_json::json!(b),
        V::Int(i)      => serde_json::json!(i),
        V::Text(s)     => serde_json::json!(s),
        V::Bytes(b)    => serde_json::json!(hex::encode(b)),
        V::Ok(x)       => serde_json::json!({"ok": json_to_v_json(x)}),
        V::Err(e)      => serde_json::json!({"error": e}),
        V::List(items) => serde_json::Value::Array(items.iter().map(json_to_v_json).collect()),
        V::Map(pairs)  => {
            let mut m = serde_json::Map::new();
            for (k, val) in pairs { m.insert(k.clone(), json_to_v_json(val)); }
            serde_json::Value::Object(m)
        }
    }
}
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args[1] != "run" {
        eprintln!("usage: fard run <file.fard> [--input key=value ...]");
        process::exit(1);
    }

    let path = &args[2];

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

    let src = fs::read(path).unwrap_or_else(|e| { eprintln!("error reading {}: {}", path, e); process::exit(1); });
    let module = fardlang::parse_module(&src).unwrap_or_else(|e| { eprintln!("parse error: {}", e); process::exit(1); });
    fardlang::check::check_module(&module).unwrap_or_else(|e| { eprintln!("check error: {}", e); process::exit(1); });

    let main_fn = module.fns.iter().find(|f| f.name == "main").unwrap_or_else(|| { eprintln!("error: no main function"); process::exit(1); });

    let mut env = fardlang::eval::Env::new();
    fardlang::eval::apply_imports(&mut env, &module.imports);

    // register declared effects
    for eff in &module.effects {
        env.declared_effects.insert(eff.name.clone());
    }

    for (k, v) in &inputs {
        let val = if let Ok(n) = v.parse::<i64>() { V::Int(n) }
                  else if v == "true" { V::Bool(true) }
                  else if v == "false" { V::Bool(false) }
                  else { V::Text(v.clone()) };
        env.bindings.push((k.clone(), fardlang::eval::EvalVal::V(val)));
    }
    for f in &module.fns {
        env.fns.insert(f.name.clone(), f.clone());
    }

    let mut handler = StdEffectHandler::new();
    let mut result: Option<V> = None;
    let mut eval_err: Option<String> = None;

    fardlang::eval::with_effect_handler(&mut handler, || {
        match fardlang::eval::eval_block(&main_fn.body, &mut env) {
            Ok(v) => result = Some(v),
            Err(e) => eval_err = Some(e.to_string()),
        }
    });

    if let Some(e) = eval_err {
        eprintln!("eval error: {}", e);
        process::exit(1);
    }

    // write trace
    if !handler.traces.is_empty() {
        let mut trace_lines = String::new();
        for t in handler.trace() {
            let args_json: Vec<serde_json::Value> = t.args.iter()
                .map(|v| json_to_v_json(v))
                .collect();
            let entry = serde_json::json!({
                "effect":       t.name,
                "args":         args_json,
                "result":       json_to_v_json(&t.result),
                "timestamp_ms": t.timestamp_ms,
            });
            trace_lines.push_str(&entry.to_string());
            trace_lines.push('\n');
        }
        let _ = fs::write("trace.ndjson", trace_lines);
    }

    let v = result.unwrap();
    let bytes = valuecore::v0::encode_json(&v);
    print!("{}", String::from_utf8(bytes).unwrap());
}
