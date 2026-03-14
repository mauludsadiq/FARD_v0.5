//! fardregistry — Global RunID registry server (v2.1 — SQLite-backed)
//!
//! Routes:
//!   POST /publish              — body: receipt JSON → {ok: run_id} | {err: msg}
//!   GET  /receipt/<run_id>     — fetch receipt by run_id
//!   GET  /verify/<run_id>      — verify receipt chain recursively
//!   GET  /stats                — {count: N, run_ids: [...]}
//!   GET  /packages             — list all packages
//!   GET  /packages/<name>      — list versions of a package
//!   POST /packages/publish     — publish a package entry
//!   GET  /health               — "ok"

use std::sync::{Arc, Mutex};
use anyhow::{bail, Result};
use rusqlite::{Connection, params};

// ── JSON (minimal, no external dep) ──────────────────────────────────────────
fn json_str(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}

fn ok_json(inner: &str) -> String { format!("{{\"ok\":{}}}", inner) }
fn err_json(msg: &str) -> String  { format!("{{\"err\":{}}}", json_str(msg)) }

// ── Minimal JSON parser (reused from fardrun) ────────────────────────────────
#[derive(Clone, Debug)]
enum J { Null, Bool(bool), Int(i64), Str(String), Array(Vec<J>), Object(Vec<(String,J)>) }

impl J {
    fn get(&self, k: &str) -> Option<&J> {
        if let J::Object(m) = self { m.iter().find(|(ky,_)| ky==k).map(|(_,v)| v) } else { None }
    }
    fn as_str(&self) -> Option<&str> { if let J::Str(s) = self { Some(s) } else { None } }
    fn as_array(&self) -> Option<&Vec<J>> { if let J::Array(a) = self { Some(a) } else { None } }
}

fn jparse(s: &str) -> Result<J> {
    let mut p = JP { chars: s.chars().collect(), pos: 0 };
    p.val()
}

struct JP { chars: Vec<char>, pos: usize }
impl JP {
    fn peek(&self) -> char { self.chars.get(self.pos).copied().unwrap_or('\0') }
    fn adv(&mut self) -> char { let c=self.peek(); self.pos+=1; c }
    fn ws(&mut self) { while " \t\r\n".contains(self.peek()) { self.adv(); } }
    fn val(&mut self) -> Result<J> {
        self.ws();
        match self.peek() {
            '"' => self.jstr().map(J::Str),
            '{' => self.obj(),
            '[' => self.arr(),
            't' => { for _ in 0..4 { self.adv(); } Ok(J::Bool(true)) }
            'f' => { for _ in 0..5 { self.adv(); } Ok(J::Bool(false)) }
            'n' => { for _ in 0..4 { self.adv(); } Ok(J::Null) }
            '-'|'0'..='9' => self.num(),
            c => bail!("unexpected: {}", c),
        }
    }
    fn jstr(&mut self) -> Result<String> {
        self.adv();
        let mut s = String::new();
        loop {
            match self.adv() {
                '"' => break,
                '\\' => match self.adv() {
                    '"'=>s.push('"'), '\\'=>s.push('\\'), 'n'=>s.push('\n'),
                    'r'=>s.push('\r'), 't'=>s.push('\t'), c=>s.push(c),
                },
                c => s.push(c),
            }
        }
        Ok(s)
    }
    fn num(&mut self) -> Result<J> {
        let mut s = String::new();
        if self.peek()=='-' { s.push(self.adv()); }
        while self.peek().is_ascii_digit() { s.push(self.adv()); }
        if self.peek()=='.' { s.push(self.adv()); while self.peek().is_ascii_digit() { s.push(self.adv()); } }
        Ok(J::Int(s.parse::<f64>().map(|f| f as i64).unwrap_or(0)))
    }
    fn obj(&mut self) -> Result<J> {
        self.adv();
        let mut m = Vec::new();
        self.ws();
        if self.peek()=='}' { self.adv(); return Ok(J::Object(m)); }
        loop {
            self.ws();
            let k = self.jstr()?;
            self.ws(); self.adv();
            let v = self.val()?;
            m.push((k,v));
            self.ws();
            match self.adv() { ','=> {} '}'=> break, _ => bail!("bad obj") }
        }
        Ok(J::Object(m))
    }
    fn arr(&mut self) -> Result<J> {
        self.adv();
        let mut a = Vec::new();
        self.ws();
        if self.peek()==']' { self.adv(); return Ok(J::Array(a)); }
        loop {
            a.push(self.val()?);
            self.ws();
            match self.adv() { ','=> {} ']'=> break, _ => bail!("bad arr") }
        }
        Ok(J::Array(a))
    }
}

// ── Database ──────────────────────────────────────────────────────────────────
struct Db { conn: Connection }

impl Db {
    fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS receipts (
                run_id       TEXT PRIMARY KEY,
                raw_json     TEXT NOT NULL,
                published_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            CREATE TABLE IF NOT EXISTS packages (
                name         TEXT NOT NULL,
                version      TEXT NOT NULL,
                entry_digest TEXT NOT NULL,
                tarball_url  TEXT NOT NULL,
                published_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
                PRIMARY KEY (name, version)
            );
        ")?;
        Ok(Db { conn })
    }

    fn publish_receipt(&mut self, raw: &str) -> Result<String> {
        let v = jparse(raw)?;
        let run_id = v.get("run_id").and_then(|x| x.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing run_id"))?;
        if !run_id.starts_with("sha256:") { bail!("run_id must start with sha256:"); }
        v.get("derived_from").and_then(|x| x.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing derived_from"))?;
        if v.get("output").is_none() { bail!("missing output"); }
        self.conn.execute(
            "INSERT OR REPLACE INTO receipts (run_id, raw_json) VALUES (?1, ?2)",
            params![run_id, raw],
        )?;
        Ok(run_id.to_string())
    }

    fn get_receipt(&self, run_id: &str) -> Option<String> {
        self.conn.query_row(
            "SELECT raw_json FROM receipts WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        ).ok()
    }

    fn verify_chain(&self, run_id: &str, depth: u32) -> Result<u32> {
        if depth > 64 { bail!("chain too deep"); }
        let raw = self.get_receipt(run_id)
            .ok_or_else(|| anyhow::anyhow!("receipt not found: {}", run_id))?;
        let v = jparse(&raw)?;
        let deps = v.get("derived_from").and_then(|x| x.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing derived_from"))?;
        let mut max_depth = depth;
        for dep in deps {
            if let Some(dep_id) = dep.as_str() {
                let d = self.verify_chain(dep_id, depth+1)?;
                if d > max_depth { max_depth = d; }
            }
        }
        Ok(max_depth)
    }

    fn stats(&self) -> Result<String> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM receipts", [], |r| r.get(0))?;
        let mut stmt = self.conn.prepare("SELECT run_id FROM receipts ORDER BY published_at DESC LIMIT 100")?;
        let ids: Vec<String> = stmt.query_map([], |r| r.get(0))?
            .filter_map(|x| x.ok())
            .collect();
        let arr = ids.iter().map(|s| json_str(s)).collect::<Vec<_>>().join(",");
        Ok(format!("{{\"count\":{},\"run_ids\":[{}]}}", count, arr))
    }

    fn publish_package(&mut self, name: &str, version: &str, entry_digest: &str, tarball_url: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO packages (name, version, entry_digest, tarball_url) VALUES (?1,?2,?3,?4)",
            params![name, version, entry_digest, tarball_url],
        )?;
        Ok(())
    }

    fn list_packages(&self) -> Result<String> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, entry_digest, tarball_url, published_at FROM packages ORDER BY name, published_at DESC")?;
        let rows: Vec<String> = stmt.query_map([], |r| {
            Ok(format!("{{\"name\":{},\"version\":{},\"entry_digest\":{},\"tarball_url\":{},\"published_at\":{}}}",
                json_str(&r.get::<_,String>(0)?),
                json_str(&r.get::<_,String>(1)?),
                json_str(&r.get::<_,String>(2)?),
                json_str(&r.get::<_,String>(3)?),
                r.get::<_,i64>(4)?))
        })?.filter_map(|x| x.ok()).collect();
        Ok(format!("[{}]", rows.join(",")))
    }

    fn list_package_versions(&self, name: &str) -> Result<String> {
        let mut stmt = self.conn.prepare(
            "SELECT version, entry_digest, tarball_url, published_at FROM packages WHERE name=?1 ORDER BY published_at DESC")?;
        let rows: Vec<String> = stmt.query_map(params![name], |r| {
            Ok(format!("{{\"version\":{},\"entry_digest\":{},\"tarball_url\":{},\"published_at\":{}}}",
                json_str(&r.get::<_,String>(0)?),
                json_str(&r.get::<_,String>(1)?),
                json_str(&r.get::<_,String>(2)?),
                r.get::<_,i64>(3)?))
        })?.filter_map(|x| x.ok()).collect();
        Ok(format!("[{}]", rows.join(",")))
    }

    fn seed_dir(&mut self, dir: &str) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                    if self.publish_receipt(&raw).is_ok() { count += 1; }
                }
            }
        }
        count
    }
}

// ── HTTP handler ──────────────────────────────────────────────────────────────
fn handle(req: &mut tiny_http::Request, db: &Arc<Mutex<Db>>) -> (u16, &'static str, String) {
    let method = req.method().to_string();
    let url    = req.url().to_string();
    let mut body = String::new();
    let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);

    match (method.as_str(), url.as_str()) {
        ("GET", "/health") =>
            (200, "text/plain", "ok".into()),

        ("GET", "/stats") => {
            let db = db.lock().unwrap();
            match db.stats() {
                Ok(s)  => (200, "application/json", s),
                Err(e) => (500, "application/json", err_json(&e.to_string())),
            }
        }

        ("POST", "/publish") => {
            let mut db = db.lock().unwrap();
            match db.publish_receipt(&body) {
                Ok(id) => (200, "application/json", ok_json(&json_str(&id))),
                Err(e) => (400, "application/json", err_json(&e.to_string())),
            }
        }

        ("GET", "/packages") => {
            let db = db.lock().unwrap();
            match db.list_packages() {
                Ok(s)  => (200, "application/json", s),
                Err(e) => (500, "application/json", err_json(&e.to_string())),
            }
        }

        ("POST", "/packages/publish") => {
            match jparse(&body) {
                Ok(v) => {
                    let name    = v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let digest  = v.get("entry_digest").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let url     = v.get("tarball_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    if name.is_empty() || version.is_empty() {
                        return (400, "application/json", err_json("name and version required"));
                    }
                    let mut db = db.lock().unwrap();
                    match db.publish_package(&name, &version, &digest, &url) {
                        Ok(_)  => (200, "application/json", ok_json(&json_str(&format!("{}@{}", name, version)))),
                        Err(e) => (500, "application/json", err_json(&e.to_string())),
                    }
                }
                Err(e) => (400, "application/json", err_json(&e.to_string())),
            }
        }

        _ if url.starts_with("/packages/") => {
            let name = url.trim_start_matches("/packages/");
            let db = db.lock().unwrap();
            match db.list_package_versions(name) {
                Ok(s)  => (200, "application/json", s),
                Err(e) => (404, "application/json", err_json(&e.to_string())),
            }
        }

        _ if url.starts_with("/receipt/") => {
            let run_id = url.trim_start_matches("/receipt/");
            let db = db.lock().unwrap();
            match db.get_receipt(run_id) {
                Some(raw) => (200, "application/json", raw),
                None      => (404, "application/json", err_json(&format!("not found: {}", run_id))),
            }
        }

        _ if url.starts_with("/verify/") => {
            let run_id = url.trim_start_matches("/verify/");
            let db = db.lock().unwrap();
            match db.verify_chain(run_id, 0) {
                Ok(depth) => (200, "application/json",
                    format!("{{\"ok\":true,\"run_id\":{},\"depth\":{}}}", json_str(run_id), depth)),
                Err(e) => (404, "application/json", err_json(&e.to_string())),
            }
        }

        _ => (404, "application/json", err_json("not found")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let port = args.windows(2).find(|w| w[0]=="--port").map(|w| w[1].clone())
        .unwrap_or_else(|| "7370".to_string());
    let db_path = args.windows(2).find(|w| w[0]=="--db").map(|w| w[1].clone())
        .unwrap_or_else(|| "fardregistry.db".to_string());
    let seed_dir = args.windows(2).find(|w| w[0]=="--seed").map(|w| w[1].clone());

    let mut db = Db::open(&db_path).expect("failed to open database");

    if let Some(dir) = seed_dir {
        let count = db.seed_dir(&dir);
        eprintln!("[fardregistry] seeded {} receipts from {}", count, dir);
    }

    let db = Arc::new(Mutex::new(db));
    let addr = format!("0.0.0.0:{}", port);
    let server = tiny_http::Server::http(&addr).expect("failed to start server");

    eprintln!("[fardregistry] db: {}", db_path);
    eprintln!("[fardregistry] listening on http://{}", addr);
    eprintln!("[fardregistry] routes: GET /health /stats /receipt/<id> /verify/<id> /packages /packages/<name>  POST /publish /packages/publish");

    for mut req in server.incoming_requests() {
        let db = Arc::clone(&db);
        let (status, ct, body) = handle(&mut req, &db);
        let resp = tiny_http::Response::from_string(body)
            .with_status_code(status)
            .with_header(tiny_http::Header::from_bytes(b"Content-Type", ct.as_bytes()).unwrap())
            .with_header(tiny_http::Header::from_bytes(b"Access-Control-Allow-Origin", b"*").unwrap());
        let _ = req.respond(resp);
    }
}
