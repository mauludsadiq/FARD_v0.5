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


mod witness {
    use sha2::{Sha256, Digest};
    use std::fs;

    pub struct Receipt {
        pub source_sha256: String,
        pub inputs: Vec<(String, String)>,
        pub output_sha256: String,
        pub trace_sha256: String,
        pub run_id: String,
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn sha256(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        hex(&h.finalize())
    }

    pub fn compute(
        source: &[u8],
        inputs: &[(String, String)],
        output_json: &[u8],
        trace_ndjson: &str,
    ) -> Receipt {
        let source_sha256 = sha256(source);
        let output_sha256 = sha256(output_json);
        let trace_sha256 = sha256(trace_ndjson.as_bytes());

        // canonical witness string: deterministic, order-stable
        let mut witness_str = String::new();
        witness_str.push_str(&format!("source:{}", source_sha256));
        for (k, v) in inputs {
            witness_str.push_str(&format!(",input.{}:{}", k, v));
        }
        witness_str.push_str(&format!(",output:{}", output_sha256));
        witness_str.push_str(&format!(",trace:{}", trace_sha256));
        let run_id = format!("sha256:{}", sha256(witness_str.as_bytes()));

        Receipt {
            source_sha256,
            inputs: inputs.to_vec(),
            output_sha256,
            trace_sha256,
            run_id,
        }
    }

    pub fn write(receipt: &Receipt, trace_ndjson: &str, output_bytes: &[u8]) {
        // write trace
        if !trace_ndjson.is_empty() {
            let _ = fs::write("trace.ndjson", trace_ndjson);
        }
        // build receipt json manually to avoid serde_json derive
        let mut inputs_json = String::from("[");
        for (i, (k, v)) in receipt.inputs.iter().enumerate() {
            if i > 0 { inputs_json.push(','); }
            inputs_json.push_str(&format!("{{\"key\":\"{}\",\"value\":\"{}\"}}", k, v));
        }
        inputs_json.push(']');
        let output_str = String::from_utf8_lossy(output_bytes);
        let json = format!(
            "{{\"run_id\":\"{}\",\"source_sha256\":\"{}\",\"inputs\":{},\"output_sha256\":\"{}\",\"trace_sha256\":\"{}\",\"output\":{}}}", 
            receipt.run_id,
            receipt.source_sha256,
            inputs_json,
            receipt.output_sha256,
            receipt.trace_sha256,
            output_str,
        );
        let _ = fs::write("receipt.json", json.clone());
        // also persist to fact store for future imports
        let _ = fs::create_dir_all("receipts");
        let store_path = format!("receipts/{}.json", receipt.run_id.replace(":", "_"));
        let _ = fs::write(&store_path, json);
    }
}

fn fard_json_to_v(j: &serde_json::Value) -> valuecore::v0::V {
    use valuecore::v0::V;
    // FARD canonical wire format: {"t": "int", "v": 42}
    if let (Some(t), Some(v)) = (j.get("t").and_then(|x| x.as_str()), j.get("v")) {
        match t {
            "unit" => return V::Unit,
            "bool" => if let Some(b) = v.as_bool() { return V::Bool(b); },
            "int" => if let Some(n) = v.as_i64() { return V::Int(n); },
            "text" => if let Some(s) = v.as_str() { return V::Text(s.to_string()); },
            "bytes" => if let Some(s) = v.as_str() {
                if let Ok(b) = hex::decode(s) { return V::Bytes(b); }
            },
            "list" => if let Some(arr) = v.as_array() {
                return V::List(arr.iter().map(fard_json_to_v).collect());
            },
            "map" => if let Some(arr) = v.as_array() {
                let pairs = arr.iter().filter_map(|item| {
                    let pair = item.as_array()?;
                    let k = pair.get(0)?.as_str()?.to_string();
                    let val = fard_json_to_v(pair.get(1)?);
                    Some((k, val))
                }).collect();
                return V::Map(pairs);
            },
            "ok" => return V::Ok(Box::new(fard_json_to_v(v))),
            "err" => if let Some(s) = v.as_str() { return V::Err(s.to_string()); },
            _ => {}
        }
    }
    // fallback to plain json
    json_to_v(j)
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
    if args.len() >= 2 && args[1] == "verify" {
        // fard verify <file.fard> <receipt.json> [--input k=v ...]
        if args.len() < 4 {
            eprintln!("usage: fard verify <file.fard> <receipt.json> [--input key=value ...]");
            process::exit(1);
        }
        let src_path = &args[2];
        let receipt_path = &args[3];
        let mut inputs: Vec<(String, String)> = vec![];
        let mut i = 4;
        while i < args.len() {
            if args[i] == "--input" && i + 1 < args.len() {
                let kv = &args[i + 1];
                if let Some(eq) = kv.find('=') {
                    inputs.push((kv[..eq].to_string(), kv[eq+1..].to_string()));
                }
                i += 2;
            } else { i += 1; }
        }
        let src = fs::read(src_path).unwrap_or_else(|e| { eprintln!("error reading {}: {}", src_path, e); process::exit(1); });
        let receipt_bytes = fs::read(receipt_path).unwrap_or_else(|e| { eprintln!("error reading {}: {}", receipt_path, e); process::exit(1); });
        let receipt_json: serde_json::Value = serde_json::from_slice(&receipt_bytes).unwrap_or_else(|e| { eprintln!("malformed receipt: {}", e); process::exit(1); });
        let claimed_run_id = receipt_json.get("run_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let output_val = receipt_json.get("output").cloned().unwrap_or(serde_json::Value::Null);
        let output_bytes = output_val.to_string().into_bytes();
        let trace_str = receipt_json.get("trace").and_then(|v| v.as_str()).unwrap_or("");
        let computed = witness::compute(&src, &inputs, &output_bytes, trace_str);
        if computed.run_id == claimed_run_id {
            eprintln!("verified: {}", claimed_run_id);
            process::exit(0);
        } else {
            eprintln!("MISMATCH");
            eprintln!("  claimed:  {}", claimed_run_id);
            eprintln!("  computed: {}", computed.run_id);
            process::exit(2);
        }
    }
    if args.len() < 3 || args[1] != "run" {
        eprintln!("usage: fard run <file.fard> [--input key=value ...]");
        eprintln!("       fard verify <file.fard> <receipt.json> [--input key=value ...]");
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

    // resolve fact imports
    for fi in &module.fact_imports {
        let store_path = format!("receipts/{}.json", fi.run_id.replace(":", "_"));
        let fact_bytes = fs::read(&store_path).unwrap_or_else(|_| {
            eprintln!("error: fact not found: {} (expected at {})", fi.run_id, store_path);
            process::exit(1);
        });
        let fact_json: serde_json::Value = serde_json::from_slice(&fact_bytes).unwrap_or_else(|e| {
            eprintln!("error: malformed fact receipt {}: {}", fi.run_id, e);
            process::exit(1);
        });
        // verify run_id matches
        let stored_id = fact_json.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
        if stored_id != fi.run_id {
            eprintln!("error: fact run_id mismatch: expected {} got {}", fi.run_id, stored_id);
            process::exit(1);
        }
        // load output value - stored as raw JSON object, not a string
        let output_val = fact_json.get("output").cloned().unwrap_or(serde_json::Value::Null);
        let v = fard_json_to_v(&output_val);
        env.bindings.push((fi.name.clone(), fardlang::eval::EvalVal::V(v)));
    }

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

    // build trace ndjson
    let mut trace_ndjson = String::new();
    for t in handler.trace() {
        let args_json: Vec<serde_json::Value> = t.args.iter().map(|v| json_to_v_json(v)).collect();
        let entry = serde_json::json!({
            "effect": t.name,
            "args": args_json,
            "result": json_to_v_json(&t.result),
            "timestamp_ms": t.timestamp_ms,
        });
        trace_ndjson.push_str(&entry.to_string());
        trace_ndjson.push('\n');
    }

    let v = result.unwrap();
    let output_bytes = valuecore::v0::encode_json(&v);

    let receipt = witness::compute(&src, &inputs, &output_bytes, &trace_ndjson);
    witness::write(&receipt, &trace_ndjson, &output_bytes);
    eprintln!("run_id: {}", receipt.run_id);

    print!("{}", String::from_utf8(output_bytes).unwrap());
}
