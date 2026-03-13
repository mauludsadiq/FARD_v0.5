//! fardregistry — Global RunID registry server
//! Stores and serves FARD receipts over HTTP.
//!
//! Routes:
//!   POST /publish          — body: receipt JSON → {ok: run_id} | {err: msg}
//!   GET  /receipt/<run_id> — fetch receipt by run_id
//!   GET  /verify/<run_id>  — verify receipt chain recursively
//!   GET  /stats            — {count: N, run_ids: [...]}
//!   GET  /health           — "ok"

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use anyhow::{bail, Result};

// ── JSON (minimal) ────────────────────────────────────────────────────────────
#[derive(Clone, Debug)]
enum J {
    Null, Bool(bool), Int(i64), Str(String),
    Array(Vec<J>), Object(BTreeMap<String, J>),
}

impl J {
    fn get(&self, k: &str) -> Option<&J> {
        if let J::Object(m) = self { m.get(k) } else { None }
    }
    fn as_str(&self) -> Option<&str> {
        if let J::Str(s) = self { Some(s) } else { None }
    }
    fn as_array(&self) -> Option<&Vec<J>> {
        if let J::Array(a) = self { Some(a) } else { None }
    }
}

fn json_parse(s: &str) -> Result<J> {
    let s = s.trim();
    let mut p = JsonParser { chars: s.chars().collect(), pos: 0 };
    p.parse_value()
}

struct JsonParser { chars: Vec<char>, pos: usize }
impl JsonParser {
    fn peek(&self) -> char { self.chars.get(self.pos).copied().unwrap_or('\0') }
    fn advance(&mut self) -> char { let c = self.peek(); self.pos += 1; c }
    fn skip_ws(&mut self) { while matches!(self.peek(), ' '|'\t'|'\r'|'\n') { self.advance(); } }
    fn parse_value(&mut self) -> Result<J> {
        self.skip_ws();
        match self.peek() {
            '"' => self.parse_string().map(J::Str),
            '{' => self.parse_object(),
            '[' => self.parse_array(),
            't' => { for _ in 0..4 { self.advance(); } Ok(J::Bool(true)) }
            'f' => { for _ in 0..5 { self.advance(); } Ok(J::Bool(false)) }
            'n' => { for _ in 0..4 { self.advance(); } Ok(J::Null) }
            '-'|'0'..='9' => self.parse_number(),
            c => bail!("unexpected char: {}", c),
        }
    }
    fn parse_string(&mut self) -> Result<String> {
        self.advance(); // "
        let mut s = String::new();
        loop {
            match self.advance() {
                '"' => break,
                '\\' => { match self.advance() {
                    '"' => s.push('"'), '\\' => s.push('\\'), '/' => s.push('/'),
                    'n' => s.push('\n'), 'r' => s.push('\r'), 't' => s.push('\t'),
                    _ => {}
                }}
                c => s.push(c),
            }
        }
        Ok(s)
    }
    fn parse_number(&mut self) -> Result<J> {
        let mut s = String::new();
        if self.peek() == '-' { s.push(self.advance()); }
        while self.peek().is_ascii_digit() { s.push(self.advance()); }
        if self.peek() == '.' {
            s.push(self.advance());
            while self.peek().is_ascii_digit() { s.push(self.advance()); }
            Ok(J::Int(s.parse::<f64>().map(|f| f as i64).unwrap_or(0)))
        } else {
            Ok(J::Int(s.parse().unwrap_or(0)))
        }
    }
    fn parse_object(&mut self) -> Result<J> {
        self.advance(); // {
        let mut m = BTreeMap::new();
        self.skip_ws();
        if self.peek() == '}' { self.advance(); return Ok(J::Object(m)); }
        loop {
            self.skip_ws();
            let k = self.parse_string()?;
            self.skip_ws(); self.advance(); // :
            let v = self.parse_value()?;
            m.insert(k, v);
            self.skip_ws();
            match self.advance() { ',' => {} '}' => break, _ => bail!("bad object") }
        }
        Ok(J::Object(m))
    }
    fn parse_array(&mut self) -> Result<J> {
        self.advance(); // [
        let mut a = Vec::new();
        self.skip_ws();
        if self.peek() == ']' { self.advance(); return Ok(J::Array(a)); }
        loop {
            a.push(self.parse_value()?);
            self.skip_ws();
            match self.advance() { ',' => {} ']' => break, _ => bail!("bad array") }
        }
        Ok(J::Array(a))
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_emit(v: &J) -> String {
    match v {
        J::Null => "null".to_string(),
        J::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        J::Int(n) => n.to_string(),
        J::Str(s) => json_str(s),
        J::Array(a) => format!("[{}]", a.iter().map(json_emit).collect::<Vec<_>>().join(",")),
        J::Object(m) => {
            let fields: Vec<String> = m.iter()
                .map(|(k, v)| format!("{}:{}", json_str(k), json_emit(v)))
                .collect();
            format!("{{{}}}", fields.join(","))
        }
    }
}

fn ok_json(v: &J) -> String {
    json_emit(&J::Object({
        let mut m = BTreeMap::new();
        m.insert("ok".to_string(), v.clone());
        m
    }))
}

fn err_json(msg: &str) -> String {
    json_emit(&J::Object({
        let mut m = BTreeMap::new();
        m.insert("err".to_string(), J::Str(msg.to_string()));
        m
    }))
}

// ── Receipt validation ────────────────────────────────────────────────────────
fn validate_receipt(r: &J) -> Result<String> {
    let run_id = r.get("run_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing run_id"))?;
    if !run_id.starts_with("sha256:") {
        bail!("run_id must start with sha256:");
    }
    // must have derived_from array
    r.get("derived_from")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing derived_from array"))?;
    // must have output field
    if r.get("output").is_none() {
        bail!("missing output field");
    }
    Ok(run_id.to_string())
}

// ── Registry store ────────────────────────────────────────────────────────────
struct Registry {
    receipts: BTreeMap<String, String>, // run_id -> raw JSON
}

impl Registry {
    fn new() -> Self { Registry { receipts: BTreeMap::new() } }

    fn publish(&mut self, raw: &str) -> Result<String> {
        let v = json_parse(raw)?;
        let run_id = validate_receipt(&v)?;
        self.receipts.insert(run_id.clone(), raw.to_string());
        Ok(run_id)
    }

    fn get(&self, run_id: &str) -> Option<&str> {
        self.receipts.get(run_id).map(|s| s.as_str())
    }

    fn verify_chain(&self, run_id: &str, depth: u32) -> Result<u32> {
        if depth > 64 { bail!("chain too deep"); }
        let raw = self.receipts.get(run_id)
            .ok_or_else(|| anyhow::anyhow!("receipt not found: {}", run_id))?;
        let v = json_parse(raw)?;
        let deps = v.get("derived_from")
            .and_then(|d| d.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing derived_from"))?;
        let mut max_depth = depth;
        for dep in deps {
            if let Some(dep_id) = dep.as_str() {
                let d = self.verify_chain(dep_id, depth + 1)?;
                if d > max_depth { max_depth = d; }
            }
        }
        Ok(max_depth)
    }

    fn stats(&self) -> J {
        let run_ids: Vec<J> = self.receipts.keys()
            .map(|k| J::Str(k.clone()))
            .collect();
        let mut m = BTreeMap::new();
        m.insert("count".to_string(), J::Int(self.receipts.len() as i64));
        m.insert("run_ids".to_string(), J::Array(run_ids));
        J::Object(m)
    }
}

// ── HTTP handler ──────────────────────────────────────────────────────────────
fn handle(
    req: &mut tiny_http::Request,
    registry: &Arc<Mutex<Registry>>,
) -> (u16, String, String) {
    let method = req.method().to_string();
    let url = req.url().to_string();

    // Read body
    let mut body = String::new();
    let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);

    let (status, content_type, response) = match (method.as_str(), url.as_str()) {

        ("GET", "/health") => (200, "text/plain", "ok".to_string()),

        ("GET", "/stats") => {
            let reg = registry.lock().unwrap();
            (200, "application/json", json_emit(&reg.stats()))
        }

        ("POST", "/publish") => {
            let mut reg = registry.lock().unwrap();
            match reg.publish(&body) {
                Ok(run_id) => (200, "application/json", ok_json(&J::Str(run_id))),
                Err(e) => (400, "application/json", err_json(&e.to_string())),
            }
        }

        _ if url.starts_with("/receipt/") => {
            let run_id = url.trim_start_matches("/receipt/");
            let reg = registry.lock().unwrap();
            match reg.get(run_id) {
                Some(raw) => (200, "application/json", raw.to_string()),
                None => (404, "application/json", err_json(&format!("not found: {}", run_id))),
            }
        }

        _ if url.starts_with("/verify/") => {
            let run_id = url.trim_start_matches("/verify/");
            let reg = registry.lock().unwrap();
            match reg.verify_chain(run_id, 0) {
                Ok(depth) => {
                    let mut m = BTreeMap::new();
                    m.insert("depth".to_string(), J::Int(depth as i64));
                    m.insert("run_id".to_string(), J::Str(run_id.to_string()));
                    m.insert("ok".to_string(), J::Bool(true));
                    (200, "application/json", json_emit(&J::Object(m)))
                }
                Err(e) => (404, "application/json", err_json(&e.to_string())),
            }
        }

        _ => (404, "application/json", err_json("not found")),
    };

    (status, content_type.to_string(), response)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = args.windows(2)
        .find(|w| w[0] == "--port")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "7370".to_string());

    // Optionally seed from a receipts/ directory
    let seed_dir = args.windows(2)
        .find(|w| w[0] == "--seed")
        .map(|w| w[1].clone());

    let registry = Arc::new(Mutex::new(Registry::new()));

    // Seed from local receipts/ if requested
    if let Some(dir) = seed_dir {
        let mut reg = registry.lock().unwrap();
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                    if reg.publish(&raw).is_ok() { count += 1; }
                }
            }
        }
        eprintln!("[fardregistry] seeded {} receipts from {}", count, dir);
    }

    let addr = format!("0.0.0.0:{}", port);
    let server = tiny_http::Server::http(&addr)
        .expect("failed to start server");
    eprintln!("[fardregistry] listening on http://{}", addr);
    eprintln!("[fardregistry] routes: GET /health /stats /receipt/<id> /verify/<id>  POST /publish");

    for mut req in server.incoming_requests() {
        let reg = Arc::clone(&registry);
        let (status, ct, body) = handle(&mut req, &reg);
        let response = tiny_http::Response::from_string(body)
            .with_status_code(status)
            .with_header(
                tiny_http::Header::from_bytes(b"Content-Type", ct.as_bytes()).unwrap()
            )
            .with_header(
                tiny_http::Header::from_bytes(b"Access-Control-Allow-Origin", b"*").unwrap()
            );
        let _ = req.respond(response);
    }
}
