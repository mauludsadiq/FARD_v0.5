use anyhow::{anyhow, bail, Context, Result};
const QMARK_EXPECT_RESULT: &str = "QMARK_EXPECT_RESULT";
const QMARK_PROPAGATE_ERR: &str = "QMARK_PROPAGATE_ERR";
const RESULT_OK_TAG: &str = "ok";
const RESULT_ERR_TAG: &str = "err";
const RESULT_TAG_KEY: &str = "t";
const RESULT_OK_VAL_KEY: &str = "v";
const RESULT_ERR_VAL_KEY: &str = "e";
const ERROR_PAT_MISMATCH: &str = "ERROR_PAT_MISMATCH";
const ERROR_MATCH_NO_ARM: &str = "ERROR_MATCH_NO_ARM";
use serde_json::{Map, Value as J};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}
fn canon_json(v: &serde_json::Value) -> Result<String> {
    fn canon_value(v: &serde_json::Value, out: &mut String) -> Result<()> {
        match v {
            serde_json::Value::Null => {
                out.push_str("null");
                Ok(())
            }
            serde_json::Value::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
                Ok(())
            }
            serde_json::Value::Number(n) => {
                let s = n.to_string();
                if s.contains('+') {
                    bail!("M5_CANON_NUM_PLUS");
                }
                if s.starts_with('0') && s.len() > 1 && !s.starts_with("0.") {
                    bail!("M5_CANON_NUM_LEADING_ZERO");
                }
                if s.ends_with(".0") {
                    bail!("M5_CANON_NUM_DOT0");
                }
                out.push_str(&s);
                Ok(())
            }
            serde_json::Value::String(s) => {
                out.push_str(&serde_json::to_string(s).context("M5_CANON_STRING_FAIL")?);
                Ok(())
            }
            serde_json::Value::Array(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    canon_value(x, out)?;
                }
                out.push(']');
                Ok(())
            }
            serde_json::Value::Object(m) => {
                out.push('{');

                // Canonical key order:
                // emit "k" first (if present), then emit remaining keys in sorted order.
                let mut first = true;

                if m.contains_key("k") {
                    let k = "k";
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    out.push_str(&serde_json::to_string(k).context("M5_CANON_KEY_ESC_FAIL")?);
                    out.push(':');
                    canon_value(&m[k], out)?;
                }

                let mut ks: Vec<&String> = m.keys().collect();
                ks.sort();
                for k in ks {
                    if k.as_str() == "k" {
                        continue;
                    }
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    out.push_str(&serde_json::to_string(k).context("M5_CANON_KEY_ESC_FAIL")?);
                    out.push(':');
                    canon_value(&m[k], out)?;
                }

                out.push('}');
                Ok(())
            }
        }
    }

    let mut out = String::new();
    canon_value(v, &mut out)?;
    Ok(out)
}

fn sha256_file_hex(path: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("read failed: {}", path.display()))?;
    Ok(sha256_bytes_hex(&bytes))
}

fn write_m5_digests(
    out_dir: &std::path::Path,
    runtime_version: &str,
    trace_format_version: &str,
    stdlib_root_digest: &str,
    ok: bool,
) -> Result<()> {
    let trace_path = out_dir.join("trace.ndjson");
    let modg_path = out_dir.join("module_graph.json");

    let trace_h = format!("sha256:{}", sha256_file_hex(&trace_path)?);
    let modg_h = format!("sha256:{}", sha256_file_hex(&modg_path)?);

    let (leaf_name, leaf_path) = if ok {
        ("result.json", out_dir.join("result.json"))
    } else {
        ("error.json", out_dir.join("error.json"))
    };
    let leaf_h = format!("sha256:{}", sha256_file_hex(&leaf_path)?);

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    files.insert("trace.ndjson".to_string(), trace_h.clone());
    files.insert("module_graph.json".to_string(), modg_h.clone());
    files.insert(leaf_name.to_string(), leaf_h.clone());
    let preimage = serde_json::json!({
      "files": files,
      "ok": ok,
      "runtime_version": runtime_version,
      "stdlib_root_digest": stdlib_root_digest,
      "trace_format_version": trace_format_version
    });
    let canon = canon_json(&preimage)?;
    let preimage_sha256 = format!("sha256:{}", sha256_bytes_hex(canon.as_bytes()));
    let dig = serde_json::json!({
      "runtime_version": runtime_version,
      "trace_format_version": trace_format_version,
      "stdlib_root_digest": stdlib_root_digest,
      "ok": ok,
      "files": files,
      "preimage_sha256": preimage_sha256
    });

    let out = canonical_json_bytes(&dig);
    std::fs::write(out_dir.join("digests.json"), out).with_context(|| "write digests.json")?;
    Ok(())
}

fn main() -> Result<()> {
    let (run, want_version) = fard_v0_5_language_gate::cli::fardrun_cli::Cli::parse_compat();
    if want_version {
        println!("fard_runtime_version={}", env!("CARGO_PKG_VERSION"));
        println!("trace_format_version=0.1.0");
        println!("stdlib_root_cid=sha256:dev");
        return Ok(());
    }
    let program = run.program;
    let out_dir = run.out;
    let lockfile = run.lockfile;
    let registry_dir = run.registry;
    fs::create_dir_all(&out_dir).ok();
    let trace_path = out_dir.join("trace.ndjson");
    let result_path = out_dir.join("result.json");
    let mut tracer = Tracer::new(&out_dir, &trace_path)?;
    let mut loader = ModuleLoader::new(program.parent().unwrap_or(Path::new(".")));
    let runtime_version = env!("CARGO_PKG_VERSION");
    let trace_format_version = "0.1.0";
    if let Some(rp) = registry_dir.clone() {
        loader.registry_dir = Some(rp);
    }
    if let Some(lockp) = lockfile {
        loader.lock = Some(Lockfile::load(&lockp)?);
    }
    let v = match loader.eval_main(&program, &mut tracer) {
        Ok(v) => v,
        Err(e) => {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let v = loader.graph.to_json();
                let b = canonical_json_bytes(&v);
                let _ = fs::write(tracer.out_dir.join("module_graph.json"), &b);
            }));
            let msg0 = format!("{}", e);
            let code = {
                const PINNED: &[&str] = &[
                    QMARK_EXPECT_RESULT,
                    QMARK_PROPAGATE_ERR,
                    ERROR_PAT_MISMATCH,
                    ERROR_MATCH_NO_ARM,
                ];
                msg0.split_whitespace()
                    .find(|w| PINNED.contains(w))
                    .or_else(|| {
                        msg0.split_whitespace()
                            .find(|w| w.starts_with("ERROR_") && *w != "ERROR_RUNTIME")
                    })
                    .or_else(|| msg0.split_whitespace().find(|w| w.starts_with("ERROR_")))
                    .unwrap_or("ERROR_RUNTIME")
                    .to_string()
            };
            let msg = {
                let mut s = msg0.clone();
                if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
                    s = rest.to_string();
                }
                if code != "ERROR_RUNTIME" {
                    if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
                        s = rest.to_string();
                    }
                    format!("{} {}", code, s)
                } else {
                    s
                }
            };
            let mut em = Map::new();
            let mut extra_e: Option<J> = None;
            if code == QMARK_PROPAGATE_ERR {
                if let Some(q) = e.downcast_ref::<QMarkUnwind>() {
                    if let Some(j) = q.err.to_json() {
                        extra_e = Some(j);
                    }
                }
            }

            em.insert("code".to_string(), J::String(code.clone()));
            em.insert("message".to_string(), J::String(msg.clone()));
            if let Some(se) = e.downcast_ref::<SpannedRuntimeError>() {
                let mut bs = se.span.byte_start;
                let mut be = se.span.byte_end;
                let mut ln = se.span.line;
                let mut cl = se.span.col;
                if let Ok(src) = fs::read_to_string(&se.span.file) {
                    let abs_s = bs.min(src.len());
                    let ls = src[..abs_s].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_s = abs_s.saturating_sub(ls);
                    let abs_e = be.min(src.len());
                    let le = src[..abs_e].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_e = abs_e.saturating_sub(le);
                    bs = rel_s;
                    be = rel_e;
                    cl = rel_s + 1;
                    ln = src[..ls].bytes().filter(|b| *b == b"\n"[0]).count() + 1;
                }
                let mut sm = Map::new();
                sm.insert("file".to_string(), J::String(se.span.file.clone()));
                sm.insert("byte_start".to_string(), J::Number((bs as u64).into()));
                sm.insert("byte_end".to_string(), J::Number((be as u64).into()));
                sm.insert("line".to_string(), J::Number((ln as u64).into()));
                sm.insert("col".to_string(), J::Number((cl as u64).into()));
                em.insert("span".to_string(), J::Object(sm));
            } else if let Some(pe) = e.downcast_ref::<ParseError>() {
                // Stored spans are absolute offsets; G39 expects line-relative byte offsets.
                let mut bs = pe.span.byte_start;
                let mut be = pe.span.byte_end;
                let mut ln = pe.span.line;
                let mut cl = pe.span.col;
                if let Ok(src) = fs::read_to_string(&pe.span.file) {
                    let abs_s = bs.min(src.len());
                    let ls = src[..abs_s].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_s = abs_s.saturating_sub(ls);
                    let abs_e = be.min(src.len());
                    let le = src[..abs_e].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_e = abs_e.saturating_sub(le);
                    bs = rel_s;
                    be = rel_e;
                    cl = rel_s + 1;
                    ln = src[..ls].bytes().filter(|b| *b == b"\n"[0]).count() + 1;
                }
                let mut sm = Map::new();
                sm.insert("file".to_string(), J::String(pe.span.file.clone()));
                sm.insert("byte_start".to_string(), J::Number((bs as u64).into()));
                sm.insert("byte_end".to_string(), J::Number((be as u64).into()));
                sm.insert("line".to_string(), J::Number((ln as u64).into()));
                sm.insert("col".to_string(), J::Number((cl as u64).into()));
                em.insert("span".to_string(), J::Object(sm));
            }
            fs::write(
                out_dir.join("error.json"),
                serde_json::to_vec(&J::Object(em))?,
            )?;

            {
                let stdlib_root_digest = loader.stdlib_root_digest();

                {
                    let mg = loader.graph.to_json();
                    let b = canonical_json_bytes(&mg);
                    fs::write(out_dir.join("module_graph.json"), &b)?;
                    let cid = sha256_bytes(&b);
                    let _ = tracer.module_graph_event(&cid);
                }

                if let Some(ev) = &extra_e {
                    tracer.error_event_with_e(&code, &msg, ev).ok();
                } else {
                    tracer.error_event(&code, &msg).ok();
                }

                drop(tracer);
                write_m5_digests(
                    &out_dir,
                    runtime_version,
                    trace_format_version,
                    &stdlib_root_digest,
                    false,
                )?;
            }
            bail!(msg);
        }
    };
    let j = v.to_json().context("final result must be jsonable")?;
    let mut root = Map::new();
    root.insert("result".to_string(), j);
    {
        let v = J::Object(root);
        fs::write(&result_path, canonical_json_bytes(&v))?;

        {
            let mg = loader.graph.to_json();
            let b = canonical_json_bytes(&mg);
            fs::write(out_dir.join("module_graph.json"), &b)?;
            let cid = sha256_bytes(&b);
            let _ = tracer.module_graph_event(&cid);

            let stdlib_root_digest = loader.stdlib_root_digest();
            drop(tracer);
            write_m5_digests(
                &out_dir,
                runtime_version,
                trace_format_version,
                &stdlib_root_digest,
                true,
            )?;
        }
    }
    Ok(())
}
struct Tracer {
    first_event: bool,
    artifact_cids: std::collections::BTreeMap<String, String>,
    w: fs::File,
    out_dir: PathBuf,
}
impl Tracer {
    fn module_graph_event(&mut self, cid: &str) -> Result<()> {
        let mut m = serde_json::Map::new();
        m.insert("t".to_string(), J::String("module_graph".to_string()));
        m.insert("cid".to_string(), J::String(cid.to_string()));
        self.emit_event(J::Object(m))
    }

    fn write_ndjson(&mut self, line: &str) -> Result<()> {
        if !self.first_event {
            std::io::Write::write_all(&mut self.w, b"\n")?;
        }
        std::io::Write::write_all(&mut self.w, line.as_bytes())?;
        self.first_event = false;
        Ok(())
    }
    fn new(out_dir: &Path, path: &Path) -> Result<Self> {
        fs::create_dir_all(out_dir).ok();
        fs::create_dir_all(out_dir.join("artifacts")).ok();
        let w = fs::File::create(path)?;
        Ok(Self {
            first_event: true,
            w,
            out_dir: out_dir.to_path_buf(),
            artifact_cids: std::collections::BTreeMap::new(),
        })
    }
    fn emit(&mut self, v: &J) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("emit".to_string()));
        m.insert("v".to_string(), v.clone());
        let line = serde_json::to_string(&J::Object(m))?;
        self.write_ndjson(&line)?;
        Ok(())
    }

    fn emit_event(&mut self, ev: J) -> Result<()> {
        let line = serde_json::to_string(&ev)?;
        self.write_ndjson(&line)?;
        Ok(())
    }
    fn grow_node(&mut self, v: &Val) -> Result<()> {
        let j = v.to_json().context("grow_node must be jsonable")?;
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("grow_node".to_string()));
        m.insert("v".to_string(), j);
        let line = serde_json::to_string(&J::Object(m))?;
        self.write_ndjson(&line)?;
        Ok(())
    }
    fn artifact_in(&mut self, path: &str, cid: &str) -> Result<()> {
        // legacy import_artifact: treat path as the stable name
        self.artifact_cids.insert(path.to_string(), cid.to_string());

        let mut m = serde_json::Map::new();
        m.insert("t".to_string(), J::String("artifact_in".to_string()));
        m.insert("name".to_string(), J::String(path.to_string()));
        m.insert("path".to_string(), J::String(path.to_string()));
        m.insert("cid".to_string(), J::String(cid.to_string()));
        self.emit_event(J::Object(m))
    }
    fn artifact_out(&mut self, name: &str, cid: &str, bytes: &[u8]) -> Result<()> {
        // legacy emit_artifact: name is also the stable name
        self.artifact_cids.insert(name.to_string(), cid.to_string());

        let out_path = self.out_dir.join("artifacts").join(name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out_path, bytes)?;

        let mut m = serde_json::Map::new();
        m.insert("t".to_string(), J::String("artifact_out".to_string()));
        m.insert("name".to_string(), J::String(name.to_string()));
        m.insert("cid".to_string(), J::String(cid.to_string()));
        m.insert("parents".to_string(), J::Array(vec![]));
        self.emit_event(J::Object(m))
    }

    fn artifact_in_named(&mut self, name: &str, path: &str, cid: &str) -> Result<()> {
        self.artifact_cids.insert(name.to_string(), cid.to_string());

        let mut m = serde_json::Map::new();
        m.insert("t".to_string(), J::String("artifact_in".to_string()));
        m.insert("name".to_string(), J::String(name.to_string()));
        m.insert("path".to_string(), J::String(path.to_string()));
        m.insert("cid".to_string(), J::String(cid.to_string()));
        self.emit_event(J::Object(m))
    }

    fn artifact_out_derived(
        &mut self,
        name: &str,
        filename: &str,
        cid: &str,
        bytes: &[u8],
        parents: &[(String, String)],
    ) -> Result<()> {
        for (pname, pcid) in parents {
            let got = match self.artifact_cids.get(pname) {
                Some(g) => g,
                None => bail!("ERROR_M3_PARENT_NOT_DECLARED {pname} (child {name})"),
            };
            if got != pcid {
                bail!("ERROR_M3_PARENT_CID_MISMATCH {pname}: declared {got} vs {pcid}");
            }
        }

        self.artifact_cids.insert(name.to_string(), cid.to_string());

        let out_path = self.out_dir.join("artifacts").join(filename);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out_path, bytes)?;

        let mut plist: Vec<J> = Vec::new();
        for (pname, pcid) in parents {
            let mut pm = serde_json::Map::new();
            pm.insert("name".to_string(), J::String(pname.clone()));
            pm.insert("cid".to_string(), J::String(pcid.clone()));
            plist.push(J::Object(pm));
        }

        let mut m = serde_json::Map::new();
        m.insert("cid".to_string(), J::String(cid.to_string()));
        m.insert("name".to_string(), J::String(name.to_string()));
        m.insert("parents".to_string(), J::Array(plist));
        m.insert("t".to_string(), J::String("artifact_out".to_string()));
        self.emit_event(J::Object(m))
    }
    fn module_resolve(&mut self, name: &str, kind: &str, cid: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("module_resolve".to_string()));
        m.insert("name".to_string(), J::String(name.to_string()));
        m.insert("kind".to_string(), J::String(kind.to_string()));
        m.insert("cid".to_string(), J::String(cid.to_string()));
        let line = serde_json::to_string(&J::Object(m))?;
        self.write_ndjson(&line)?;
        Ok(())
    }

    fn error_event_with_e(&mut self, code: &str, message: &str, e: &J) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("error".to_string()));
        m.insert("code".to_string(), J::String(code.to_string()));
        let mut s = message.to_string();
        if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
            s = rest.to_string();
        }
        if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
            s = rest.to_string();
        }
        m.insert("message".to_string(), J::String(format!("{} {}", code, s)));
        m.insert("e".to_string(), e.clone());
        let line = serde_json::to_string(&J::Object(m))?;
        self.write_ndjson(&line)?;
        Ok(())
    }

    fn error_event(&mut self, code: &str, message: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("error".to_string()));
        m.insert("code".to_string(), J::String(code.to_string()));
        let mut s = message.to_string();
        if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
            s = rest.to_string();
        }
        if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
            s = rest.to_string();
        }
        m.insert("message".to_string(), J::String(format!("{} {}", code, s)));
        let line = serde_json::to_string(&J::Object(m))?;
        self.write_ndjson(&line)?;
        Ok(())
    }
}
#[derive(Clone, Debug)]
enum Tok {
    OrOr,

    Kw(String),
    Ident(String),
    Num(i64),
    Float(f64),
    Str(String),
    Sym(String),
    Eof,
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct SpanPos {
    byte_start: usize,
    byte_end: usize,
    line: usize,
    col: usize,
}
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
#[derive(Clone, Debug)]
struct ErrorSpan {
    file: String,
    byte_start: usize,
    byte_end: usize,
    line: usize,
    col: usize,
}
#[derive(Debug)]
struct ParseError {
    span: ErrorSpan,
    message: String,
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ParseError {}
#[derive(Debug, Clone)]
struct SpannedRuntimeError {
    span: ErrorSpan,
    message: String,
}
impl std::fmt::Display for SpannedRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for SpannedRuntimeError {}
fn line_col_at(src: &str, byte_pos: usize) -> (usize, usize) {
    let mut line: usize = 1;
    let mut col: usize = 1;
    let mut i: usize = 0;
    for ch in src.chars() {
        if i >= byte_pos {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        i += 1;
    }
    (line, col)
}
struct Lex {
    s: Vec<char>,
    i: usize,
}
impl Lex {
    fn new(src: &str) -> Self {
        Self {
            s: src.chars().collect(),
            i: 0,
        }
    }
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == "#".chars().next().unwrap() {
                while let Some(d) = self.bump() {
                    if d == "\n".chars().next().unwrap() {
                        break;
                    }
                }
                continue;
            }
            if c.is_whitespace() {
                self.i += 1;
                continue;
            }
            if c == '/' && self.s.get(self.i + 1) == Some(&'/') {
                while let Some(d) = self.peek() {
                    self.i += 1;
                    if d == '\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }
    fn next(&mut self) -> Result<Tok> {
        self.skip_ws();
        let Some(c) = self.peek() else {
            return Ok(Tok::Eof);
        };
        if is_ident_start(c) {
            let mut t = String::new();
            t.push(self.bump().unwrap());
            while let Some(d) = self.peek() {
                if is_ident_cont(d) {
                    t.push(self.bump().unwrap());
                } else {
                    break;
                }
            }
            {
                let id = t;
                let kws = [
                    "let", "in", "fn", "if", "then", "else", "import", "as", "export", "match",
                    "using", "true", "false", "null",
                ];
                if kws.contains(&id.as_str()) {
                    return Ok(Tok::Kw(id));
                }
                return Ok(Tok::Ident(id));
            }
        }
        if c.is_ascii_digit() {
            // NOTE: `c` is peeked (not consumed). Anchor at current index.
            let start = self.i;
            let mut n: i64 = 0;
            while let Some(d) = self.peek() {
                if d.is_ascii_digit() {
                    n = n * 10 + (d as i64 - '0' as i64);
                    self.i += 1;
                } else {
                    break;
                }
            }
            let len = self.i - start;
            if len > 1 && self.s[start] == '0' {
                bail!("ERROR_PARSE leading zero integer literal");
            }
            // Check for float: digits '.' digits
            if self.peek() == Some('.') && self.i + 1 < self.s.len() && self.s[self.i + 1].is_ascii_digit() {
                self.i += 1; // consume '.'
                let mut frac: f64 = 0.0;
                let mut place: f64 = 0.1;
                while let Some(d) = self.peek() {
                    if d.is_ascii_digit() {
                        frac += (d as i64 - '0' as i64) as f64 * place;
                        place *= 0.1;
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                return Ok(Tok::Float(n as f64 + frac));
            }
            return Ok(Tok::Num(n));
        }
        if c == '"' {
            self.bump();
            let mut t = String::new();
            while let Some(d) = self.bump() {
                if d == '"' {
                    break;
                }
                if d == '\\' {
                    let e = self.bump().ok_or_else(|| anyhow!("bad escape"))?;
                    match e {
                        'n' => t.push('\n'),
                        't' => t.push('\t'),
                        '"' => t.push('"'),
                        '\\' => t.push('\\'),
                        _ => bail!("bad escape: \\{e}"),
                    }
                } else {
                    t.push(d);
                }
            }
            return Ok(Tok::Str(t));
        }
        let three = if self.i + 2 < self.s.len() {
            let mut t = String::new();
            t.push(self.s[self.i]);
            t.push(self.s[self.i + 1]);
            t.push(self.s[self.i + 2]);
            Some(t)
        } else {
            None
        };
        if three.as_deref() == Some("...") {
            self.i += 3;
            return Ok(Tok::Sym("...".to_string()));
        }
        let two = if self.i + 1 < self.s.len() {
            let mut t = String::new();
            t.push(self.s[self.i]);
            t.push(self.s[self.i + 1]);
            Some(t)
        } else {
            None
        };
        if two.as_deref() == Some("||") {
            self.i += 2;
            return Ok(Tok::OrOr);
        }

        for op in ["!=", "==", "<=", ">=", "&&", "->", "=>", "|>"] {
            if two.as_deref() == Some(op) {
                self.i += 2;
                return Ok(Tok::Sym(op.to_string()));
            }
        }
        let one = self.bump().unwrap();
        let sym = match one {
            '(' | ')' | '{' | '}' | '[' | ']' | ',' | ':' | '.' | '+' | '-' | '*' | '/' | '='
            | '%' | '|' | '<' | '>' | '?' | '!' => one.to_string(),
            _ => bail!("unexpected char: {one}"),
        };
        Ok(Tok::Sym(sym))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Type {
    Int,
    String,
    Bool,
    Unit,
    List(Box<Type>),
    Rec(Vec<(String, Type)>),
    Func(Vec<Type>, Box<Type>),
    #[allow(dead_code)]
    Var(String),
    Named(String, Vec<Type>),
    Dynamic,
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum Pat {
    Wild,
    Bind(String),
    LitInt(i64),
    LitStr(String),
    LitBool(bool),
    LitNull,
    Obj {
        items: Vec<(String, Pat)>,
        rest: Option<String>,
    },
    List {
        items: Vec<Pat>,
        rest: Option<String>,
    },
}

fn pat_reject_duplicate_binds(p: &Pat) -> Result<()> {
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    pat_reject_duplicate_binds_rec(p, &mut seen)
}

fn pat_reject_duplicate_binds_rec(
    p: &Pat,
    seen: &mut std::collections::BTreeSet<String>,
) -> Result<()> {
    match p {
        Pat::Wild => Ok(()),
        Pat::LitInt(_) => Ok(()),
        Pat::LitStr(_) => Ok(()),
        Pat::LitBool(_) => Ok(()),
        Pat::LitNull => Ok(()),
        Pat::Bind(name) => {
            if !seen.insert(name.clone()) {
                bail!("ERROR_PARSE duplicate binding {}", name);
            }
            Ok(())
        }
        Pat::Obj { items, rest } => {
            for (_k, sub) in items {
                pat_reject_duplicate_binds_rec(sub, seen)?;
            }
            if let Some(r) = rest {
                if !seen.insert(r.clone()) {
                    bail!("ERROR_PARSE duplicate binding {}", r);
                }
            }
            Ok(())
        }
        Pat::List { items, rest } => {
            for sub in items {
                pat_reject_duplicate_binds_rec(sub, seen)?;
            }
            if let Some(r) = rest {
                if !seen.insert(r.clone()) {
                    bail!("ERROR_PARSE duplicate binding {}", r);
                }
            }
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct MatchArm {
    pat: Pat,
    guard: Option<Expr>, // match-arm guard: pat if <expr> => body
    guard_span: Option<ErrorSpan>,
    body: Expr,
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum Expr {
    Let(String, Box<Expr>, Box<Expr>),
    LetPat(Pat, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Fn(Vec<Pat>, Box<Expr>),
    Lambda(Vec<Pat>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Get(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),  // e[i] — list/map index
    List(Vec<Expr>),
    Rec(Vec<(String, Expr)>),
    Var(String),
    Int(i64),
    FloatLit(f64),
    Bool(bool),
    Str(String),
    Null,
    Bin(String, Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
    Try(Box<Expr>),
    Match(Box<Expr>, Vec<MatchArm>),
    Using(Pat, Box<Expr>, Box<Expr>),
}
#[derive(Clone, Debug)]
enum Item {
    Import(String, String),
    Let(String, Expr, Option<ErrorSpan>),
    Fn(String, Vec<(Pat, Option<Type>)>, Option<Type>, Expr),
    Export(Vec<String>),
    Expr(Expr, Option<ErrorSpan>),
}
struct Parser {
    toks: Vec<Tok>,
    spans: Vec<(usize, usize)>,
    file: String,
    src: String,
    i: usize,
}
impl Parser {
    fn from_src(src: &str, file: &str) -> Result<Self> {
        let mut lx = Lex::new(src);
        let mut toks = Vec::new();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        loop {
            // IMPORTANT: spans must begin at the first token byte, not at preceding whitespace/comments
            lx.skip_ws();
            let byte_start = lx.i;
            let t = lx.next()?;
            let byte_end = lx.i;
            let done = matches!(t, Tok::Eof);
            toks.push(t);
            spans.push((byte_start, byte_end));
            if done {
                break;
            }
        }
        Ok(Self {
            toks,
            spans,
            file: file.to_string(),
            src: src.to_string(),
            i: 0,
        })
    }
    fn peek(&self) -> &Tok {
        self.toks.get(self.i).unwrap_or(&Tok::Eof)
    }
    fn peek_n(&self, n: usize) -> &Tok {
        self.toks.get(self.i + n).unwrap_or(&Tok::Eof)
    }
    fn bump(&mut self) -> Tok {
        let t = self.peek().clone();
        self.i += 1;
        t
    }
    fn cur_span(&self) -> ErrorSpan {
        // Many parse errors are reported after we already advanced `i`
        // (e.g. via bump / expect_*). Use the previous token span when possible.
        let idx = if self.i > 0 { self.i - 1 } else { 0 };
        let (byte_start, byte_end) = self.spans.get(idx).cloned().unwrap_or((0usize, 0usize));
        let (line, col) = line_col_at(&self.src, byte_start);
        ErrorSpan {
            file: self.file.clone(),
            byte_start,
            byte_end,
            line,
            col,
        }
    }
    fn tok_span(&self, idx: usize) -> ErrorSpan {
        let (byte_start, byte_end) = self.spans.get(idx).cloned().unwrap_or((0usize, 0usize));
        let (line, col) = line_col_at(&self.src, byte_start);
        ErrorSpan {
            file: self.file.clone(),
            byte_start,
            byte_end,
            line,
            col,
        }
    }
    fn span_range(&self, lo_idx: usize, hi_idx: usize) -> ErrorSpan {
        let lo = self.tok_span(lo_idx);
        let hi = self.tok_span(hi_idx);
        ErrorSpan {
            file: lo.file,
            byte_start: lo.byte_start,
            byte_end: hi.byte_end,
            line: lo.line,
            col: lo.col,
        }
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if x == s) && {
            self.i += 1;
            true
        }
    }
    fn expect_sym(&mut self, s: &str) -> Result<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            bail!("ERROR_PARSE expected symbol {s:?}")
        }
    }
    fn eat_kw(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Tok::Kw(x) if x == s)
            || matches!(self.peek(), Tok::Ident(x) if x == s)
        {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect_kw(&mut self, s: &str) -> Result<()> {
        if self.eat_kw(s) {
            Ok(())
        } else {
            bail!("ERROR_PARSE expected keyword {s}")
        }
    }
    fn expect_ident(&mut self) -> Result<String> {
        let t = self.peek().clone();
        match self.bump() {
            Tok::Ident(x) => Ok(x),
            _ => bail!("ERROR_PARSE expected identifier; got {:?}", t),
        }
    }
    fn parse_fn_block_inner(&mut self) -> Result<Expr> {
        let mut binds: Vec<(String, Expr)> = Vec::new();
        while self.eat_kw("let") {
            let name = self.expect_ident()?;
            self.expect_sym("=")?;
            let rhs = self.parse_expr()?;
            binds.push((name, rhs));
        }
        let mut tail = self.parse_expr()?;
        for (name, rhs) in binds.into_iter().rev() {
            tail = Expr::Let(name, Box::new(rhs), Box::new(tail));
        }
        Ok(tail)
    }
    fn parse_fn_block_body(&mut self) -> Result<Expr> {
        let body = self.parse_fn_block_inner()?;
        self.expect_sym("}")?;
        Ok(body)
    }
    fn parse_type(&mut self) -> Result<Type> {
        match self.peek() {
            Tok::Kw(x) | Tok::Ident(x) if x == "Int" => {
                self.i += 1;
                Ok(Type::Int)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "String" => {
                self.i += 1;
                Ok(Type::String)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "Bool" => {
                self.i += 1;
                Ok(Type::Bool)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "Unit" => {
                self.i += 1;
                Ok(Type::Unit)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "Dynamic" => {
                self.i += 1;
                Ok(Type::Dynamic)
            }
            Tok::Kw(name) | Tok::Ident(name) => {
                let name = name.clone();
                self.i += 1;
                if name == "List" {
                    self.expect_sym("<")?;
                    let inner = self.parse_type()?;
                    self.expect_sym(">")?;
                    return Ok(Type::List(Box::new(inner)));
                }
                if name == "Rec" {
                    self.expect_sym("{")?;
                    let mut fields: Vec<(String, Type)> = Vec::new();
                    if !self.eat_sym("}") {
                        loop {
                            let k = self.expect_ident()?;
                            self.expect_sym(":")?;
                            let t = self.parse_type()?;
                            fields.push((k, t));
                            if self.eat_sym("}") {
                                break;
                            }
                            self.expect_sym(",")?;
                        }
                    }
                    return Ok(Type::Rec(fields));
                }
                if name == "Func" {
                    self.expect_sym("(")?;
                    let mut args: Vec<Type> = Vec::new();
                    if !self.eat_sym(")") {
                        loop {
                            let a = self.parse_type()?;
                            args.push(a);
                            if self.eat_sym(")") {
                                break;
                            }
                            self.expect_sym(",")?;
                        }
                    }
                    self.expect_sym("->")?;
                    let ret = self.parse_type()?;
                    return Ok(Type::Func(args, Box::new(ret)));
                }
                if self.eat_sym("<") {
                    let mut args: Vec<Type> = Vec::new();
                    if !self.eat_sym(">") {
                        loop {
                            let a = self.parse_type()?;
                            args.push(a);
                            if self.eat_sym(">") {
                                break;
                            }
                            self.expect_sym(",")?;
                        }
                    }
                    return Ok(Type::Named(name, args));
                }
                Ok(Type::Named(name, Vec::new()))
            }
            Tok::Sym(s) if s == "(" => {
                self.i += 1;
                let t = self.parse_type()?;
                self.expect_sym(")")?;
                Ok(t)
            }
            _ => bail!("ERROR_PARSE expected type"),
        }
    }
    #[allow(dead_code)]
    fn parse_type_annotation(&mut self) -> Result<Option<Type>> {
        if self.eat_sym(":") {
            Ok(Some(self.parse_type()?))
        } else {
            Ok(None)
        }
    }
    fn parse_module(&mut self) -> Result<Vec<Item>> {
        let mut items = Vec::new();
        while !matches!(self.peek(), Tok::Eof) {
            if self.eat_kw("import") {
                self.expect_sym("(")?;
                let p = match self.bump() {
                    Tok::Str(s) => s,
                    _ => bail!("ERROR_PARSE import() requires string"),
                };
                self.expect_sym(")")?;
                self.expect_kw("as")?;
                let alias = self.expect_ident()?;
                items.push(Item::Import(p, alias));
                continue;
            }
            if self.eat_kw("export") {
                self.expect_sym("{")?;
                let mut names = Vec::new();
                loop {
                    let n = self.expect_ident()?;
                    names.push(n);
                    if self.eat_sym("}") {
                        break;
                    }
                    self.expect_sym(",")?;
                    if self.eat_sym("}") {
                        break;
                    }
                }
                items.push(Item::Export(names));
                continue;
            }
            if self.eat_kw("fn") {
                let name = self.expect_ident()?;
                self.expect_sym("(")?;
                let mut params: Vec<(Pat, Option<Type>)> = Vec::new();
                if !self.eat_sym(")") {
                    loop {
                        let p = self.parse_pat()?;
                        let ann = if self.eat_sym(":") {
                            Some(self.parse_type()?)
                        } else {
                            None
                        };
                        params.push((p, ann));
                        if self.eat_sym(")") {
                            break;
                        }
                        self.expect_sym(",")?;
                    }
                }
                let ret: Option<Type> = if self.eat_sym("->") {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect_sym("{")?;
                let body = self.parse_fn_block_body()?;
                items.push(Item::Fn(name, params, ret, body));
                continue;
            }
            if matches!(self.peek(), Tok::Kw(s) if s == "let") {
                let __save = self.i;
                if let Ok(e) = self.parse_expr() {
                    items.push(Item::Expr(e, Some(self.cur_span())));
                    continue;
                }
                self.i = __save;
                self.expect_kw("let")?;
                let name = self.expect_ident()?;
                self.expect_sym("=")?;
                let rhs = self.parse_expr()?;
                items.push(Item::Let(name, rhs, Some(self.cur_span())));
                continue;
            }
            let e = self.parse_expr()?;
            items.push(Item::Expr(e, Some(self.cur_span())));
            continue;
        }
        Ok(items)
    }
    fn parse_pat(&mut self) -> Result<Pat> {
        let p = match self.peek().clone() {
            Tok::Kw(x) | Tok::Ident(x) if x == "true" => {
                self.i += 1;
                Pat::LitBool(true)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "false" => {
                self.i += 1;
                Pat::LitBool(false)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "null" => {
                self.i += 1;
                Pat::LitNull
            }
            Tok::Ident(x) if x == "_" => {
                self.i += 1;
                Pat::Wild
            }
            Tok::Num(n) => {
                self.i += 1;
                Pat::LitInt(n)
            }
            Tok::Str(s) => {
                self.i += 1;
                Pat::LitStr(s)
            }
            Tok::Sym(s) if s == "{" => {
                self.i += 1;
                let mut items: Vec<(String, Pat)> = Vec::new();
                let mut rest: Option<String> = None;
                if !self.eat_sym("}") {
                    loop {
                        if self.eat_sym("...") {
                            let r = self.expect_ident()?;
                            rest = Some(r);
                            self.expect_sym("}")?;
                            break;
                        }
                        let k = self.expect_ident()?;
                        self.expect_sym(":")?;
                        let sub = self.parse_pat()?;
                        items.push((k, sub));
                        if self.eat_sym("}") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym("}") {
                            break;
                        }
                    }
                }
                Pat::Obj { items, rest }
            }
            Tok::Sym(s) if s == "[" => {
                self.i += 1;
                let mut items: Vec<Pat> = Vec::new();
                let mut rest: Option<String> = None;
                if !self.eat_sym("]") {
                    loop {
                        if self.eat_sym("...") {
                            let r = self.expect_ident()?;
                            rest = Some(r);
                            self.expect_sym("]")?;
                            break;
                        }
                        let sub = self.parse_pat()?;
                        items.push(sub);
                        if self.eat_sym("]") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym("]") {
                            break;
                        }
                    }
                }
                Pat::List { items, rest }
            }
            Tok::Ident(x) => {
                self.i += 1;
                Pat::Bind(x)
            }
            Tok::Kw(x) => {
                self.i += 1;
                Pat::Bind(x)
            }
            _ => bail!("ERROR_PARSE expected pattern"),
        };

        pat_reject_duplicate_binds(&p)?;
        Ok(p)
    }
    fn parse_match_arms(&mut self) -> Result<Vec<MatchArm>> {
        self.expect_sym("{")?;
        let mut arms: Vec<MatchArm> = Vec::new();
        if self.eat_sym("}") {
            return Ok(arms);
        }
        loop {
            let pat = self.parse_pat()?;
            let (guard, guard_span) = if self.eat_kw("if") {
                let lo_i = self.i;
                let g = self.parse_expr()?;
                let hi_i = if self.i > 0 { self.i - 1 } else { lo_i };
                (Some(g), Some(self.span_range(lo_i, hi_i)))
            } else {
                (None, None)
            };
            self.expect_sym("=>")?;
            let body = self.parse_expr()?;
            arms.push(MatchArm {
                pat,
                guard,
                guard_span,
                body,
            });
            if self.eat_sym(",") {
                if self.eat_sym("}") {
                    break;
                }
                continue;
            }
            self.expect_sym("}")?;
            break;
        }
        Ok(arms)
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        if self.eat_kw("using") {
            let pat = self.parse_pat()?;
            self.expect_sym("=")?;
            let acquire = self.parse_expr()?;
            self.expect_kw("in")?;
            let body = self.parse_expr()?;
            return Ok(Expr::Using(pat, Box::new(acquire), Box::new(body)));
        }
        if self.eat_kw("match") {
            let scrut = self.parse_expr()?;
            let arms = self.parse_match_arms()?;
            return Ok(Expr::Match(Box::new(scrut), arms));
        }
        if self.eat_kw("let") {
            let pat = self.parse_pat()?;
            self.expect_sym("=")?;
            let e1 = self.parse_expr()?;
            self.expect_kw("in")?;
            let e2 = self.parse_expr()?;
            match &pat {
                Pat::Bind(name) => return Ok(Expr::Let(name.clone(), Box::new(e1), Box::new(e2))),
                _ => return Ok(Expr::LetPat(pat, Box::new(e1), Box::new(e2))),
            }
        }
        if self.eat_kw("if") {
            let c = self.parse_expr()?;
            self.expect_kw("then")?;
            let t = self.parse_expr()?;
            self.expect_kw("else")?;
            let f = self.parse_expr()?;
            return Ok(Expr::If(Box::new(c), Box::new(t), Box::new(f)));
        }
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Expr> {
        let mut e = self.parse_and()?;
        while self.eat_sym("||") {
            let r = self.parse_and()?;
            e = Expr::Bin("||".to_string(), Box::new(e), Box::new(r));
        }
        Ok(e)
    }
    fn parse_and(&mut self) -> Result<Expr> {
        let mut e = self.parse_eq()?;
        while self.eat_sym("&&") {
            let r = self.parse_eq()?;
            e = Expr::Bin("&&".to_string(), Box::new(e), Box::new(r));
        }
        Ok(e)
    }
    fn parse_eq(&mut self) -> Result<Expr> {
        let mut e = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Tok::Sym(x) if x == "==" || x == "!=" || x == "<" || x == ">" || x == "<=" || x == ">=" => {
                    x.clone()
                }
                _ => break,
            };
            self.bump();
            let r = self.parse_add()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }
    fn parse_add(&mut self) -> Result<Expr> {
        let mut e = self.parse_mul()?;
        loop {
            if self.eat_sym("+") {
                let r = self.parse_mul()?;
                e = Expr::Bin("+".to_string(), Box::new(e), Box::new(r));
            } else if self.eat_sym("-") {
                let r = self.parse_mul()?;
                e = Expr::Bin("-".to_string(), Box::new(e), Box::new(r));
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn parse_mul(&mut self) -> Result<Expr> {
        let mut e = self.parse_unary()?;
        loop {
            if self.eat_sym("*") {
                let r = self.parse_unary()?;
                e = Expr::Bin("*".to_string(), Box::new(e), Box::new(r));
            } else if self.eat_sym("/") {
                let r = self.parse_unary()?;
                e = Expr::Bin("/".to_string(), Box::new(e), Box::new(r));
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn parse_unary(&mut self) -> Result<Expr> {
        if self.eat_sym("-") {
            let e = self.parse_unary()?;
            return Ok(Expr::Unary("-".to_string(), Box::new(e)));
        }
        if self.eat_sym("!") {
            let e = self.parse_unary()?;
            return Ok(Expr::Unary("!".to_string(), Box::new(e)));
        }
        self.parse_pipe()
    }
    fn parse_pipe(&mut self) -> Result<Expr> {
        let mut e = self.parse_postfix()?;
        while self.eat_sym("|>") {
            let rhs = self.parse_postfix()?;
            e = match rhs {
                Expr::Call(f, mut args) => {
                    let mut new_args = Vec::new();
                    new_args.push(e);
                    new_args.append(&mut args);
                    Expr::Call(f, new_args)
                }
                other => Expr::Call(Box::new(other), vec![e]),
            };
        }
        Ok(e)
    }
    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            if self.eat_sym("?") {
                e = Expr::Try(Box::new(e));
                continue;
            }
            if self.eat_sym(".") {
                let n = self.expect_ident()?;
                e = Expr::Get(Box::new(e), n);
                continue;
            }
            if self.eat_sym("(") {
                let mut args = Vec::new();
                if !self.eat_sym(")") {
                    loop {
                        let a = self.parse_expr()?;
                        args.push(a);
                        if self.eat_sym(")") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym(")") {
                            break;
                        }
                    }
                }
                e = Expr::Call(Box::new(e), args);
                continue;
            }
            if self.eat_sym("[") {
                let idx = self.parse_expr()?;
                self.expect_sym("]")?;
                e = Expr::Index(Box::new(e), Box::new(idx));
                continue;
            }
            break;
        }
        Ok(e)
    }
    fn parse_primary(&mut self) -> Result<Expr> {
        {
            if let Tok::Ident(name) = self.peek().clone() {
                if matches!(self.peek_n(1), Tok::Sym(s) if s == "=>") {
                    self.bump();
                    self.bump();
                    let body = self.parse_expr()?;
                    return Ok(Expr::Lambda(vec![Pat::Bind(name)], Box::new(body)));
                }
            }
            if matches!(self.peek(), Tok::Sym(s) if s == "(") {
                let save = self.i;
                self.bump();
                let mut ps: Vec<Pat> = Vec::new();
                if matches!(self.peek(), Tok::Sym(s) if s == ")") {
                    self.bump();
                } else {
                    loop {
                        match self.bump() {
                            Tok::Ident(s) => ps.push(Pat::Bind(s)),
                            _ => {
                                self.i = save;
                                break;
                            }
                        }
                        if matches!(self.peek(), Tok::Sym(s) if s == ",") {
                            self.bump();
                            continue;
                        }
                        if matches!(self.peek(), Tok::Sym(s) if s == ")") {
                            self.bump();
                            break;
                        }
                        self.i = save;
                        break;
                    }
                }
                if self.i != save {
                    if matches!(self.peek(), Tok::Sym(s) if s == "=>") {
                        self.bump();
                        let body = self.parse_expr()?;
                        return Ok(Expr::Lambda(ps, Box::new(body)));
                    } else {
                        self.i = save;
                    }
                }
            }
        }
        if self.eat_kw("fn") {
            self.expect_sym("(")?;
            let mut params = Vec::new();
            if !self.eat_sym(")") {
                loop {
                    let p = self.parse_pat()?;
                    params.push(p);
                    if self.eat_sym(")") {
                        break;
                    }
                    self.expect_sym(",")?;
                    if self.eat_sym(")") {
                        break;
                    }
                }
            }
            self.expect_sym("{")?;
            let body = self.parse_fn_block_inner()?;
            self.expect_sym("}")?;
            return Ok(Expr::Fn(params, Box::new(body)));
        }
        match self.bump() {
            Tok::Num(n) => Ok(Expr::Int(n)),
            Tok::Float(f) => Ok(Expr::FloatLit(f)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Kw(s) if s == "true" => Ok(Expr::Bool(true)),
            Tok::Kw(s) if s == "false" => Ok(Expr::Bool(false)),
            Tok::Kw(s) if s == "null" => Ok(Expr::Null),
            Tok::Ident(s) => Ok(Expr::Var(s)),
            Tok::Sym(s) if s == "(" => {
                let e = self.parse_expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            Tok::Sym(s) if s == "[" => {
                let mut xs = Vec::new();
                if !self.eat_sym("]") {
                    loop {
                        xs.push(self.parse_expr()?);
                        if self.eat_sym("]") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym("]") {
                            break;
                        }
                    }
                }
                Ok(Expr::List(xs))
            }
            Tok::Sym(s) if s == "{" => {
                let mut kvs = Vec::new();
                if !self.eat_sym("}") {
                    loop {
                        let k = match self.bump() {
                            Tok::Ident(x) => x,
                            Tok::Kw(x) => x,
                            Tok::Str(x) => x,
                            _ => bail!("record key must be ident or string"),
                        };
                        self.expect_sym(":")?;
                        let v = self.parse_expr()?;
                        kvs.push((k, v));
                        if self.eat_sym("}") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym("}") {
                            break;
                        }
                    }
                }
                Ok(Expr::Rec(kvs))
            }
            other => {
                return Err(anyhow!(ParseError {
                    span: self.cur_span(),
                    message: format!("ERROR_PARSE unexpected token: {other:?}"),
                }));
            }
        }
    }
}
#[derive(Clone, Debug)]
enum Val {
    Bytes(Vec<u8>),
    Int(i64),
    Bool(bool),
    Str(String),
    Null,
    List(Vec<Val>),
    Rec(BTreeMap<String, Val>),
    Func(Func),
    Builtin(Builtin),
}
#[derive(Debug)]
struct QMarkUnwind {
    err: Val,
}
impl std::fmt::Display for QMarkUnwind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {:?}", QMARK_PROPAGATE_ERR, self.err)
    }
}
impl std::error::Error for QMarkUnwind {}
#[derive(Clone, Debug)]
struct Func {
    params: Vec<Pat>,
    body: Expr,
    env: Env,
}
#[derive(Clone, Debug)]
enum Builtin {
    PngRed1x1,
    Unimplemented,
    ListMap,
    ListFilter,
    ListRange,
    ListRepeat,
    ListConcat,
    ListFold,
    StrTrim,
    StrToLower,
    StrSplitLines,
    ResultOk,
    ResultAndThen,
    ResultUnwrapOk,
    ResultUnwrapErr,
    RecEmpty,
    RecKeys,
    RecValues,
    RecHas,
    RecGet,
    RecGetOr,
    RecGetOrErr,
    RecSet,
    RecRemove,
    RecMerge,
    RecSelect,
    RecRename,
    RecUpdate,
    ResultErr,
    ListGet,
    ListLen,
    ListHead,
    ListTail,
    ListAppend,
    ListZip,
    ListReverse,
    ListFlatten,
    ListSortByIntKey,
    GrowUnfoldTree,
    GrowAppend,
    ImportArtifact,
    ImportArtifactNamed,
    EmitArtifact,
    EmitArtifactDerived,
    Emit,
    Len,
    IntParse,
    IntPow,
    IntAdd,
    IntEq,
    SortInt,
    DedupeSortedInt,
    HistInt,
    Unfold,
    FlowPipe,
    FlowId,
    FlowTap,
    StrLen,
    StrConcat,
    MapGet,
    MapSet,
    JsonEncode,
    JsonDecode,
    JsonCanonicalize,
    CryptoEd25519Verify,
    CryptoHmacSha256,
    CodecBase64UrlEncode,
    CodecBase64UrlDecode,
    RandUuidV4,
    StrSplit,
    CodecHexEncode,
    CodecHexDecode,
    HashSha256Text,
    HashSha256Bytes,
    IntMul,
    IntDiv,
    IntSub,
    IntMod,
    IntLt,
    IntGt,
    IntLe,
    IntGe,
    // float builtins
    FloatFromInt,
    FloatToInt,
    FloatFromText,
    FloatToText,
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatExp,
    FloatLn,
    FloatSqrt,
    FloatPow,
    FloatAbs,
    FloatNeg,
    FloatFloor,
    FloatCeil,
    FloatRound,
    FloatLt,
    FloatGt,
    FloatLe,
    FloatGe,
    FloatEq,
    FloatNan,
    FloatInf,
    FloatIsNan,
    FloatIsFinite,
    FloatMin,
    FloatMax,
    // linalg builtins
    LinalgDot,
    LinalgNorm,
    LinalgZeros,
    LinalgEye,
    LinalgMatvec,
    LinalgMatmul,
    LinalgTranspose,
    LinalgEigh,
    LinalgVecAdd,
    LinalgVecSub,
    LinalgVecScale,
    LinalgMatAdd,
    LinalgMatScale,
}

#[derive(Debug)]
#[allow(dead_code)]
struct QmarkPropagateErr {
    e: Val,
}

impl std::fmt::Display for QmarkPropagateErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {:?}", QMARK_PROPAGATE_ERR, self.e)
    }
}

impl std::error::Error for QmarkPropagateErr {}

use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct Env {
    inner: Arc<EnvInner>,
}

#[derive(Debug)]
struct EnvInner {
    vars: Mutex<HashMap<String, Val>>,
    parent: Option<Env>,
}

impl Env {
    fn new() -> Self {
        Self {
            inner: Arc::new(EnvInner {
                vars: Mutex::new(HashMap::new()),
                parent: None,
            }),
        }
    }

    fn child(&self) -> Self {
        Self {
            inner: Arc::new(EnvInner {
                vars: Mutex::new(HashMap::new()),
                parent: Some(self.clone()),
            }),
        }
    }

    fn set(&mut self, k: String, v: Val) {
        let mut g = self.inner.vars.lock().unwrap();
        g.insert(k, v);
    }

    fn get(&self, k: &str) -> Option<Val> {
        // lock only for local lookup; drop before recursing to parent
        if let Some(v) = self.inner.vars.lock().unwrap().get(k).cloned() {
            return Some(v);
        }
        let parent = self.inner.parent.clone();
        parent.as_ref().and_then(|p| p.get(k))
    }
}
impl Val {
    #[allow(dead_code)]
    fn to_vc_json(&self) -> Option<J> {
        match self {
            Val::Int(n) => Some(serde_json::json!({"t":"int","v":*n})),
            Val::Bool(b) => Some(serde_json::json!({"t":"bool","v":*b})),
            Val::Str(s) => Some(serde_json::json!({"t":"str","v":s})),
            Val::Null => Some(serde_json::json!({"t":"null","v":null})),
            Val::List(xs) => {
                let mut out: Vec<J> = Vec::with_capacity(xs.len());
                for x in xs {
                    out.push(x.to_vc_json()?);
                }
                Some(serde_json::json!({"t":"list","v":out}))
            }
            Val::Rec(m) => {
                let mut obj = Map::new();
                for (k, v) in m.iter() {
                    obj.insert(k.clone(), v.to_vc_json()?);
                }
                Some(serde_json::json!({"t":"rec","v":J::Object(obj)}))
            }
            _ => None,
        }
    }

    fn to_json(&self) -> Option<J> {
        match self {
            Val::Int(n) => Some(J::Number((*n).into())),
            Val::Bool(b) => Some(J::Bool(*b)),
            Val::Str(s) => Some(J::String(s.clone())),
            Val::Null => Some(J::Null),
            Val::List(xs) => Some(J::Array(
                xs.iter().map(|x| x.to_json()).collect::<Option<Vec<_>>>()?,
            )),
            Val::Rec(m) => {
                let mut obj = Map::new();

                // Canonical object field order for records:
                // emit "k" first (if present), then emit remaining keys in BTreeMap order.
                if let Some(vk) = m.get("k") {
                    obj.insert("k".to_string(), vk.to_json()?);
                }

                for (k, v) in m.iter() {
                    if k == "k" {
                        continue;
                    }
                    obj.insert(k.clone(), v.to_json()?);
                }

                Some(J::Object(obj))
            }
            Val::Bytes(bs) => {
                let mut obj = Map::new();
                obj.insert("t".to_string(), J::String("bytes".to_string()));
                obj.insert("v".to_string(), J::String(format!("hex:{}", hex_lower(bs))));
                Some(J::Object(obj))
            }
            Val::Func(_) | Val::Builtin(_) => None,
        }
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>> {
    let s = s
        .strip_prefix("hex:")
        .ok_or_else(|| anyhow::anyhow!("bytes v must start with hex:"))?;
    if s.len() % 2 != 0 {
        anyhow::bail!("hex length must be even");
    }
    let bs = s.as_bytes();
    let mut out = Vec::with_capacity(bs.len() / 2);
    let mut i = 0usize;
    while i < bs.len() {
        let hi = (bs[i] as char)
            .to_digit(16)
            .ok_or_else(|| anyhow::anyhow!("bad hex"))? as u8;
        let lo = (bs[i + 1] as char)
            .to_digit(16)
            .ok_or_else(|| anyhow::anyhow!("bad hex"))? as u8;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn val_from_json(j: &J) -> Result<Val> {
    match j {
        J::Null => Ok(Val::Null),
        J::Bool(b) => Ok(Val::Bool(*b)),
        J::Number(n) => {
            let i = n
                .as_i64()
                .ok_or_else(|| anyhow!("ERROR_RUNTIME json number not i64"))?;
            Ok(Val::Int(i))
        }
        J::String(s) => Ok(Val::Str(s.clone())),
        J::Array(xs) => {
            let mut out = Vec::new();
            for x in xs {
                out.push(val_from_json(x)?);
            }
            Ok(Val::List(out))
        }
        J::Object(m) => {
            if m.len() == 2 {
                if let (Some(J::String(t)), Some(J::String(v))) = (m.get("t"), m.get("v")) {
                    if t == "bytes" {
                        return Ok(Val::Bytes(parse_hex_bytes(v)?));
                    }
                }
            }
            let mut out = BTreeMap::new();
            for (k, v) in m.iter() {
                out.insert(k.clone(), val_from_json(v)?);
            }
            Ok(Val::Rec(out))
        }
    }
}
fn fard_pat_match_v0_5(p: &Pat, v: &Val, env: &mut Env) -> Result<bool> {
    match p {
        Pat::Wild => Ok(true),
        Pat::Bind(n) => {
            env.set(n.clone(), v.clone());
            Ok(true)
        }
        Pat::LitInt(i) => Ok(matches!(v, Val::Int(j) if j == i)),
        Pat::LitStr(s) => Ok(matches!(v, Val::Str(t) if t == s)),
        Pat::LitBool(b) => Ok(matches!(v, Val::Bool(c) if c == b)),
        Pat::LitNull => Ok(matches!(v, Val::Null)),
        Pat::List { items, rest } => match v {
            Val::List(xs) => {
                if xs.len() < items.len() {
                    return Ok(false);
                }
                for (i, subp) in items.iter().enumerate() {
                    if !fard_pat_match_v0_5(subp, &xs[i], env)? {
                        return Ok(false);
                    }
                }
                if let Some(rn) = rest {
                    let k = items.len();
                    env.set(rn.clone(), Val::List(xs[k..].to_vec()));
                }
                Ok(true)
            }
            _ => Ok(false),
        },
        Pat::Obj { items, rest } => match v {
            Val::Rec(m) => {
                for (k, subp) in items.iter() {
                    let vv = match m.get(k) {
                        Some(vv) => vv,
                        None => return Ok(false),
                    };
                    if !fard_pat_match_v0_5(subp, vv, env)? {
                        return Ok(false);
                    }
                }
                if let Some(rn) = rest {
                    let mut rm: BTreeMap<String, Val> = BTreeMap::new();
                    for (k, vv) in m.iter() {
                        if items.iter().any(|(kk, _)| kk == k) {
                            continue;
                        }
                        rm.insert(k.clone(), vv.clone());
                    }
                    env.set(rn.clone(), Val::Rec(rm));
                }
                Ok(true)
            }
            _ => Ok(false),
        },
    }
}
fn eval(e: &Expr, env: &mut Env, tracer: &mut Tracer, loader: &mut ModuleLoader) -> Result<Val> {
    match e {
        Expr::Int(n) => Ok(Val::Int(*n)),
        Expr::FloatLit(f) => Ok(Val::Bytes(f.to_le_bytes().to_vec())),
        Expr::Bool(b) => Ok(Val::Bool(*b)),
        Expr::Str(s) => Ok(Val::Str(s.clone())),
        Expr::Null => Ok(Val::Null),
        Expr::Var(n) => env.get(n).ok_or_else(|| anyhow!("unbound var: {n}")),
        Expr::List(xs) => {
            let mut out = Vec::new();
            for x in xs {
                out.push(eval(x, env, tracer, loader)?);
            }
            Ok(Val::List(out))
        }
        Expr::Rec(kvs) => {
            let mut m = BTreeMap::new();
            for (k, v) in kvs {
                m.insert(k.clone(), eval(v, env, tracer, loader)?);
            }
            Ok(Val::Rec(m))
        }
        Expr::Index(obj, idx) => {
            let v = eval(obj, env, tracer, loader)?;
            let i = eval(idx, env, tracer, loader)?;
            match (v, i) {
                (Val::List(xs), Val::Int(n)) => {
                    if n < 0 || n as usize >= xs.len() {
                        bail!("ERROR_OOB index {} out of bounds (len {})", n, xs.len());
                    }
                    Ok(xs[n as usize].clone())
                }
                (Val::Rec(m), Val::Str(k)) => {
                    m.get(&k).cloned().ok_or_else(|| anyhow!("ERROR_KEY key {:?} not found", k))
                }
                _ => bail!("ERROR_BADARG index operator requires list[int] or rec[str]"),
            }
        }
        Expr::Get(obj, k) => {
            let o = eval(obj, env, tracer, loader)?;
            match o {
                Val::Rec(m) => m
                    .get(k)
                    .cloned()
                    .ok_or_else(|| anyhow!("EXPORT_MISSING missing field {k}")),
                _ => bail!("field access on non-record"),
            }
        }
        Expr::Let(name, e1, e2) => {
            let v1 = eval(e1, env, tracer, loader)?;
            let mut child = env.child();
            child.set(name.clone(), v1);
            eval(e2, &mut child, tracer, loader)
        }
        Expr::LetPat(pat, e1, e2) => {
            let v1 = eval(e1, env, tracer, loader)?;
            let mut child = env.child();
            if !fard_pat_match_v0_5(pat, &v1, &mut child)? {
                bail!("{} let pattern did not match", ERROR_PAT_MISMATCH);
            }
            eval(e2, &mut child, tracer, loader)
        }
        Expr::If(c, t, f) => {
            let cv = eval(c, env, tracer, loader)?;
            match cv {
                Val::Bool(true) => eval(t, env, tracer, loader),
                Val::Bool(false) => eval(f, env, tracer, loader),
                _ => bail!("if cond must be bool"),
            }
        }
        Expr::Fn(params, body) => Ok(Val::Func(Func {
            params: params.clone(),
            body: (*body.clone()),
            env: env.clone(),
        })),
        Expr::Lambda(params, body) => Ok(Val::Func(Func {
            params: params.clone(),
            body: (*body.clone()),
            env: env.clone(),
        })),
        Expr::Unary(op, a) => {
            let v = eval(a, env, tracer, loader)?;
            match (op.as_str(), v) {
                ("-", Val::Int(n)) => Ok(Val::Int(-n)),
                ("-", Val::Bytes(b)) => { let f = f64::from_le_bytes(b.as_slice().try_into().unwrap_or([0u8;8])); Ok(Val::Bytes((-f).to_le_bytes().to_vec())) }
                ("!", Val::Bool(b)) => Ok(Val::Bool(!b)),
                _ => bail!("bad unary op"),
            }
        }
        Expr::Bin(op, a, b) => {
            let x = eval(a, env, tracer, loader)?;
            let y = eval(b, env, tracer, loader)?;
            match (op.as_str(), x, y) {
                ("+", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l + r)),
                ("-", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l - r)),
                ("*", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l * r)),
                ("/", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l / r)),
                ("==", l, r) => Ok(Val::Bool(val_eq(&l, &r))),
                ("!=", l, r) => Ok(Val::Bool(!val_eq(&l, &r))),
                ("&&", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l && r)),
                ("||", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l || r)),
                ("<", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l < r)),
                (">", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l > r)),
                ("<=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l <= r)),
                (">=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l >= r)),
                _ => bail!("bad binop {op}"),
            }
        }
        Expr::Call(f, args) => {
            let fv = eval(f, env, tracer, loader)?;
            let mut av = Vec::new();
            for a in args {
                av.push(eval(a, env, tracer, loader)?);
            }
            call(fv, av, tracer, loader)
        }
        Expr::Try(x) => {
            let rv = eval(x, env, tracer, loader)?;

            let ok = match result_is_ok(&rv) {
                Ok(b) => b,
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("QMARK_EXPECT_RESULT ok missing v") {
                        bail!("QMARK_EXPECT_RESULT ok missing v");
                    }
                    if msg.contains("QMARK_EXPECT_RESULT err missing e") {
                        bail!("QMARK_EXPECT_RESULT err missing e");
                    }
                    bail!("{} expected result", QMARK_EXPECT_RESULT);
                }
            };

            if ok {
                result_unwrap_ok(&rv)
            } else {
                let e = result_unwrap_err(&rv)?;
                return Err(QMarkUnwind { err: e }.into());
            }
        }
        Expr::Match(scrut, _arms) => {
            let sv = eval(scrut, env, tracer, loader)?;
            for arm in _arms.iter() {
                let mut env2 = env.child();
                if fard_pat_match_v0_5(&arm.pat, &sv, &mut env2)? {
                    if let Some(g) = &arm.guard {
                        let gv = eval(g, &mut env2, tracer, loader)?;
                        match gv {
                            Val::Bool(true) => {}
                            Val::Bool(false) => {
                                continue;
                            }
                            _ => {
                                let sp = arm
                                    .guard_span
                                    .clone()
                                    .expect("guard_span must exist when guard exists");
                                return Err(anyhow!(SpannedRuntimeError {
                                    span: sp,
                                    message: "ERROR_RUNTIME match guard not bool".to_string(),
                                }));
                            }
                        }
                    }
                    match eval(&arm.body, &mut env2, tracer, loader) {
                        Ok(v) => return Ok(v),
                        Err(e) => return Err(e),
                    };
                }
            }
            bail!("{} no match", ERROR_MATCH_NO_ARM)
        }
        Expr::Using(_pat, acquire, body) => {
            let av = eval(acquire, env, tracer, loader)?;
            let mut env2 = env.child();
            if !fard_pat_match_v0_5(_pat, &av, &mut env2)? {
                bail!("{} using pattern did not match", ERROR_PAT_MISMATCH)
            }
            eval(body, &mut env2, tracer, loader)
        }
    }
}
#[allow(dead_code)]
fn is_result_val(v: &Val) -> bool {
    match v {
        Val::Rec(m) => match m.get(RESULT_TAG_KEY) {
            Some(Val::Str(t)) if t == RESULT_OK_TAG => true,
            Some(Val::Str(t)) if t == RESULT_ERR_TAG => true,
            _ => false,
        },
        _ => false,
    }
}
fn result_is_ok(v: &Val) -> Result<bool> {
    match v {
        Val::Rec(m) => {
            if m.len() != 2 {
                bail!("{} expected result", QMARK_EXPECT_RESULT);
            }
            match m.get(RESULT_TAG_KEY) {
                Some(Val::Str(t)) if t == RESULT_OK_TAG => {
                    if !m.contains_key(RESULT_OK_VAL_KEY) {
                        bail!("QMARK_EXPECT_RESULT ok missing v");
                    }
                    Ok(true)
                }
                Some(Val::Str(t)) if t == RESULT_ERR_TAG => {
                    if !m.contains_key(RESULT_ERR_VAL_KEY) {
                        bail!("QMARK_EXPECT_RESULT err missing e");
                    }
                    Ok(false)
                }
                _ => bail!("{} expected result tag", QMARK_EXPECT_RESULT),
            }
        }
        _ => bail!("{} expected result", QMARK_EXPECT_RESULT),
    }
}

fn result_unwrap_ok(v: &Val) -> Result<Val> {
    match v {
        Val::Rec(m) => match m.get(RESULT_TAG_KEY) {
            Some(Val::Str(t)) if t == RESULT_OK_TAG => m
                .get(RESULT_OK_VAL_KEY)
                .cloned()
                .ok_or_else(|| anyhow!("QMARK_EXPECT_RESULT ok missing v")),
            Some(Val::Str(t)) if t == RESULT_ERR_TAG => {
                bail!("QMARK_EXPECT_RESULT tried unwrap ok on err")
            }
            Some(_) => bail!("{} expected result tag", QMARK_EXPECT_RESULT),
            None => bail!("{} expected result", QMARK_EXPECT_RESULT),
        },
        _ => bail!("{} expected result", QMARK_EXPECT_RESULT),
    }
}
fn result_unwrap_err(v: &Val) -> Result<Val> {
    match v {
        Val::Rec(m) => match m.get(RESULT_TAG_KEY) {
            Some(Val::Str(t)) if t == RESULT_ERR_TAG => match m.get(RESULT_ERR_VAL_KEY) {
                Some(x) => Ok(x.clone()),
                None => bail!("QMARK_EXPECT_RESULT err missing e"),
            },
            Some(Val::Str(t)) if t == RESULT_OK_TAG => {
                bail!("QMARK_EXPECT_RESULT tried unwrap err on ok")
            }
            Some(_) => bail!("{} expected result tag", QMARK_EXPECT_RESULT),
            None => bail!("{} expected result", QMARK_EXPECT_RESULT),
        },
        _ => bail!("{} expected result", QMARK_EXPECT_RESULT),
    }
}
fn mk_result_ok(v: Val) -> Val {
    let mut m = BTreeMap::new();
    m.insert(
        RESULT_TAG_KEY.to_string(),
        Val::Str(RESULT_OK_TAG.to_string()),
    );
    m.insert(RESULT_OK_VAL_KEY.to_string(), v);
    Val::Rec(m)
}

fn mk_result_err(e: Val) -> Val {
    let mut m = BTreeMap::new();
    m.insert(
        RESULT_TAG_KEY.to_string(),
        Val::Str(RESULT_ERR_TAG.to_string()),
    );
    m.insert(RESULT_ERR_VAL_KEY.to_string(), e);
    Val::Rec(m)
}
fn val_eq(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Int(x), Val::Int(y)) => x == y,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Str(x), Val::Str(y)) => x == y,
        (Val::Null, Val::Null) => true,
        (Val::List(xs), Val::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| val_eq(x, y))
        }
        (Val::Rec(xm), Val::Rec(ym)) => {
            xm.len() == ym.len()
                && xm
                    .iter()
                    .all(|(k, xv)| ym.get(k).map(|yv| val_eq(xv, yv)).unwrap_or(false))
        }
        _ => false,
    }
}
fn call(f: Val, args: Vec<Val>, tracer: &mut Tracer, loader: &mut ModuleLoader) -> Result<Val> {
    // Trampoline loop for tail-call optimisation.
    // Instead of recursing into Rust stack frames for every FARD call,
    // we detect when the body of a Func evaluates to another Func call
    // and loop at this level, replacing f/args in place.
    let mut cur_f = f;
    let mut cur_args = args;
    loop {
        match cur_f {
            Val::Builtin(b) => return call_builtin(b, cur_args, tracer, loader),
            Val::Func(fun) => {
                if fun.params.len() != cur_args.len() {
                    bail!("arity mismatch: expected {} args, got {}", fun.params.len(), cur_args.len());
                }
                let mut e = fun.env.child();
                for (p, a) in fun.params.iter().zip(cur_args.into_iter()) {
                    if !fard_pat_match_v0_5(p, &a, &mut e)? {
                        bail!("{} arg pattern did not match", ERROR_PAT_MISMATCH);
                    }
                }
                // Evaluate the body. If the result is a TailCall sentinel, loop.
                // Otherwise return the value directly.
                match eval_tco(&fun.body, &mut e, tracer, loader) {
                    Ok(TcoResult::Done(v)) => return Ok(v),
                    Ok(TcoResult::TailCall(next_f, next_args)) => {
                        cur_f = next_f;
                        cur_args = next_args;
                        // loop continues
                    }
                    Err(err) => {
                        if let Some(q) = err.downcast_ref::<QMarkUnwind>() {
                            return Ok(mk_result_err(q.err.clone()));
                        } else {
                            return Err(err);
                        }
                    }
                }
            }
            _ => bail!("call on non-function"),
        }
    }
}

enum TcoResult {
    Done(Val),
    TailCall(Val, Vec<Val>),
}

fn eval_tco(expr: &Expr, env: &mut Env, tracer: &mut Tracer, loader: &mut ModuleLoader) -> Result<TcoResult> {
    // Only the outermost Call in tail position can be a TailCall.
    // Everything else delegates to normal eval.
    match expr {
        Expr::Call(f, args) => {
            let fv = eval(f, env, tracer, loader)?;
            let mut av = Vec::new();
            for a in args {
                av.push(eval(a, env, tracer, loader)?);
            }
            match fv {
                Val::Func(_) => Ok(TcoResult::TailCall(fv, av)),
                Val::Builtin(b) => Ok(TcoResult::Done(call_builtin(b, av, tracer, loader)?)),
                _ => bail!("call on non-function"),
            }
        }
        Expr::If(c, t, f) => {
            let cv = eval(c, env, tracer, loader)?;
            match cv {
                Val::Bool(true) => eval_tco(t, env, tracer, loader),
                Val::Bool(false) => eval_tco(f, env, tracer, loader),
                _ => bail!("if cond must be bool"),
            }
        }
        Expr::Let(name, rhs, body) => {
            let v = eval(rhs, env, tracer, loader)?;
            let mut env2 = env.child();
            env2.set(name.clone(), v);
            eval_tco(body, &mut env2, tracer, loader)
        }
        Expr::Match(scrut, arms) => {
            let sv = eval(scrut, env, tracer, loader)?;
            for arm in arms.iter() {
                let mut env2 = env.child();
                if fard_pat_match_v0_5(&arm.pat, &sv, &mut env2)? {
                    if let Some(g) = &arm.guard {
                        let gv = eval(g, &mut env2, tracer, loader)?;
                        match gv {
                            Val::Bool(true) => {}
                            Val::Bool(false) => continue,
                            _ => bail!("ERROR_RUNTIME match guard not bool"),
                        }
                    }
                    return eval_tco(&arm.body, &mut env2, tracer, loader);
                }
            }
            bail!("ERROR_RUNTIME no match arm matched")
        }
        // All other expressions are not tail calls — evaluate normally
        other => Ok(TcoResult::Done(eval(other, env, tracer, loader)?)),
    }
}
fn call_builtin(
    b: Builtin,
    args: Vec<Val>,
    tracer: &mut Tracer,
    loader: &mut ModuleLoader,
) -> Result<Val> {
    match b {
        Builtin::PngRed1x1 => {
            if !args.is_empty() {
                bail!("ERROR_BADARG std/png.red_1x1 expects 0 args");
            }
            let bs = hex::decode("89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000f494441547801010400fbff00ff0000030101008d1de5820000000049454e44ae426082").map_err(|_| anyhow!("ERROR_BADARG std/png.red_1x1 invalid hex"))?;
            Ok(Val::Bytes(bs))
        }

        Builtin::IntAdd => {
            if args.len() != 2 {
                bail!("ERROR_BADARG int.add expects 2 args");
            }
            let a = match &args[0] {
                Val::Int(n) => *n,
                _ => bail!("ERROR_BADARG int.add arg0 must be int"),
            };
            let b = match &args[1] {
                Val::Int(n) => *n,
                _ => bail!("ERROR_BADARG int.add arg1 must be int"),
            };
            let out = a
                .checked_add(b)
                .ok_or_else(|| anyhow!("ERROR_OVERFLOW int.add overflow"))?;
            Ok(Val::Int(out))
        }

        Builtin::IntEq => {
            if args.len() != 2 {
                bail!("ERROR_BADARG int.eq expects 2 args");
            }
            let a = match &args[0] {
                Val::Int(n) => *n,
                _ => bail!("ERROR_BADARG int.eq arg0 must be int"),
            };
            let b = match &args[1] {
                Val::Int(n) => *n,
                _ => bail!("ERROR_BADARG int.eq arg1 must be int"),
            };
            Ok(Val::Bool(a == b))
        }

        Builtin::Unimplemented => bail!("ERROR_RUNTIME UNIMPLEMENTED_BUILTIN"),
        Builtin::ResultOk => {
            if args.len() != 1 {
                bail!("ERROR_BADARG result.ok expects 1 arg");
            }
            Ok(mk_result_ok(args[0].clone()))
        }
        Builtin::ResultAndThen => {
            if args.len() != 2 {
                bail!("ERROR_BADARG result.andThen expects 2 args");
            }
            let r = args[0].clone();
            let f = args[1].clone();

            let ok = result_is_ok(&r)?;
            if !ok {
                return Ok(r);
            }

            let v = result_unwrap_ok(&r)?;
            let out = call(f, vec![v], tracer, loader)?;

            let _ = result_is_ok(&out)?;
            Ok(out)
        }

        Builtin::ResultUnwrapOk => {
            if args.len() != 1 {
                bail!("ERROR_BADARG result.unwrap_ok expects 1 arg");
            }
            let r = args[0].clone();
            let v = result_unwrap_ok(&r)?;
            Ok(v)
        }
        Builtin::ResultUnwrapErr => {
            if args.len() != 1 {
                bail!("ERROR_BADARG result.unwrap_err expects 1 arg");
            }
            let r = args[0].clone();
            let e = result_unwrap_err(&r)?;
            Ok(e)
        }

        Builtin::ResultErr => {
            if args.len() != 1 {
                bail!("ERROR_BADARG result.err expects 1 arg");
            }
            Ok(mk_result_err(args[0].clone()))
        }
        Builtin::FlowPipe => {
            if args.len() != 2 {
                bail!("ERROR_BADARG flow.pipe expects 2 args");
            }
            let mut acc = args[0].clone();
            let fs = match &args[1] {
                Val::List(xs) => xs.clone(),
                _ => bail!("ERROR_BADARG flow.pipe arg1 must be list"),
            };
            for f in fs {
                acc = call(f, vec![acc], tracer, loader)?;
            }
            Ok(acc)
        }
        Builtin::FlowId => {
            if args.len() != 1 {
                bail!("ERROR_BADARG flow.id expects 1 arg");
            }
            Ok(args[0].clone())
        }
        Builtin::FlowTap => {
            if args.len() != 2 {
                bail!("ERROR_BADARG flow.tap expects 2 args");
            }
            let x = args[0].clone();
            let f = args[1].clone();
            let _ = call(f, vec![x.clone()], tracer, loader)?;
            Ok(x)
        }
        Builtin::GrowAppend => {
            if args.len() != 2 {
                bail!("ERROR_BADARG grow.append expects 2 args");
            }
            let mut xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG grow.append arg0 must be list"),
            };
            xs.push(args[1].clone());
            Ok(Val::List(xs))
        }
        Builtin::StrLen => {
            if args.len() != 1 {
                bail!("ERROR_RUNTIME arity");
            }
            match &args[0] {
                Val::Str(s) => Ok(Val::Int(s.len() as i64)),
                _ => bail!("ERROR_RUNTIME type"),
            }
        }
        Builtin::StrConcat => {
            if args.len() != 2 {
                bail!("ERROR_RUNTIME arity");
            }
            let a = match &args[0] {
                Val::Str(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let b = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            Ok(Val::Str(format!("{}{}", a, b)))
        }
        Builtin::MapGet => {
            if args.len() != 2 {
                bail!("ERROR_RUNTIME arity");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            Ok(m.get(k).cloned().unwrap_or(Val::Null))
        }
        Builtin::MapSet => {
            if args.len() != 3 {
                bail!("ERROR_RUNTIME arity");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let v = args[2].clone();
            let mut out = m.clone();
            out.insert(k.clone(), v);
            Ok(Val::Rec(out))
        }
        Builtin::JsonEncode => {
            if args.len() != 1 {
                bail!("ERROR_RUNTIME arity");
            }
            let j = args[0]
                .to_json()
                .ok_or_else(|| anyhow!("ERROR_RUNTIME json encode non-jsonable"))?;
            Ok(Val::Str(serde_json::to_string(&j)?))
        }
        Builtin::JsonDecode => {
            if args.len() != 1 {
                bail!("ERROR_RUNTIME arity");
            }
            let j: J = match &args[0] {
                Val::Str(ss) => serde_json::from_str(ss)?,
                Val::Bytes(bs) => serde_json::from_slice(bs)?,
                _ => bail!("ERROR_RUNTIME type"),
            };
            val_from_json(&j)
        }

        Builtin::JsonCanonicalize => {
            if args.len() != 1 { bail!("ERROR_RUNTIME arity"); }
            let j = args[0].to_json().ok_or_else(|| anyhow::anyhow!("ERROR_RUNTIME cannot canonicalize"))?;
            let canonical = serde_json::to_string(&j)?;
            Ok(Val::Str(canonical))
        }
        Builtin::CryptoEd25519Verify => {
            if args.len() != 3 { bail!("ERROR_RUNTIME ed25519_verify expects 3 args"); }
            let pk_hex  = match &args[0] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            let msg_hex = match &args[1] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            let sig_hex = match &args[2] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            let pk_bytes  = hex::decode(&pk_hex)?;
            let msg_bytes = hex::decode(&msg_hex)?;
            let sig_bytes = hex::decode(&sig_hex)?;
            use ed25519_dalek::{VerifyingKey, Signature, Verifier};
            let pk_arr: [u8;32] = pk_bytes.as_slice().try_into().map_err(|_| anyhow::anyhow!("bad pk length"))?;
            let sig_arr: [u8;64] = sig_bytes.as_slice().try_into().map_err(|_| anyhow::anyhow!("bad sig length"))?;
            let pk  = VerifyingKey::from_bytes(&pk_arr)?;
            let sig = Signature::from_bytes(&sig_arr);
            let ok  = pk.verify(&msg_bytes, &sig).is_ok();
            Ok(Val::Bool(ok))
        }
        Builtin::CryptoHmacSha256 => {
            if args.len() != 2 { bail!("ERROR_RUNTIME hmac_sha256 expects 2 args"); }
            let key_bytes = match &args[0] {
                Val::Str(ss) => hex::decode(ss)?,
                Val::Bytes(bs) => bs.clone(),
                _ => bail!("ERROR_RUNTIME hmac_sha256 key must be hex str or bytes"),
            };
            let msg_bytes = match &args[1] {
                Val::Str(ss) => ss.as_bytes().to_vec(),
                Val::Bytes(bs) => bs.clone(),
                _ => bail!("ERROR_RUNTIME hmac_sha256 msg must be str or bytes"),
            };
            use hmac::{Hmac, Mac};
            type HmacSha256 = Hmac<sha2::Sha256>;
            let mut mac = HmacSha256::new_from_slice(&key_bytes)?;
            mac.update(&msg_bytes);
            let result = mac.finalize();
            Ok(Val::Str(hex::encode(result.into_bytes())))
        }
        Builtin::CodecBase64UrlEncode => {
            if args.len() != 1 { bail!("ERROR_RUNTIME base64url_encode expects 1 arg"); }
            let input = match &args[0] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            Ok(Val::Str(URL_SAFE_NO_PAD.encode(input.as_bytes())))
        }
        Builtin::CodecBase64UrlDecode => {
            if args.len() != 1 { bail!("ERROR_RUNTIME base64url_decode expects 1 arg"); }
            let input = match &args[0] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            let bytes = URL_SAFE_NO_PAD.decode(input.as_bytes())?;
            Ok(Val::Bytes(bytes))
        }
        Builtin::RandUuidV4 => {
            if args.len() != 0 { bail!("ERROR_RUNTIME rand.uuid_v4 expects 0 args"); }
            Ok(Val::Str(uuid::Uuid::new_v4().to_string()))
        }
        Builtin::ListLen => {
            match args.first() {
                Some(Val::List(xs)) => Ok(Val::Int(xs.len() as i64)),
                Some(Val::Str(s)) => Ok(Val::Int(s.chars().count() as i64)),
                _ => bail!("ERROR_BADARG list.len expects list or str"),
            }
        }
        Builtin::ListHead => {
            match args.first() {
                Some(Val::List(xs)) if !xs.is_empty() => Ok(xs[0].clone()),
                Some(Val::List(_)) => bail!("ERROR_OOB list.head on empty list"),
                _ => bail!("ERROR_BADARG list.head expects list"),
            }
        }
        Builtin::ListTail => {
            match args.first() {
                Some(Val::List(xs)) if !xs.is_empty() => Ok(Val::List(xs[1..].to_vec())),
                Some(Val::List(_)) => bail!("ERROR_OOB list.tail on empty list"),
                _ => bail!("ERROR_BADARG list.tail expects list"),
            }
        }
        Builtin::ListAppend => {
            if args.len() != 2 { bail!("ERROR_BADARG list.append expects 2 args"); }
            match (&args[0], &args[1]) {
                (Val::List(xs), v) => {
                    let mut out = xs.clone();
                    out.push(v.clone());
                    Ok(Val::List(out))
                }
                _ => bail!("ERROR_BADARG list.append expects list, val"),
            }
        }
        Builtin::ListZip => {
            if args.len() != 2 { bail!("ERROR_BADARG list.zip expects 2 args"); }
            match (&args[0], &args[1]) {
                (Val::List(xs), Val::List(ys)) => {
                    let out: Vec<Val> = xs.iter().zip(ys.iter()).map(|(x, y)| {
                        Val::List(vec![x.clone(), y.clone()])
                    }).collect();
                    Ok(Val::List(out))
                }
                _ => bail!("ERROR_BADARG list.zip expects two lists"),
            }
        }
        Builtin::ListReverse => {
            match args.first() {
                Some(Val::List(xs)) => {
                    let mut out = xs.clone();
                    out.reverse();
                    Ok(Val::List(out))
                }
                _ => bail!("ERROR_BADARG list.reverse expects list"),
            }
        }
        Builtin::ListFlatten => {
            match args.first() {
                Some(Val::List(xs)) => {
                    let mut out = Vec::new();
                    for x in xs {
                        match x {
                            Val::List(inner) => out.extend(inner.clone()),
                            other => out.push(other.clone()),
                        }
                    }
                    Ok(Val::List(out))
                }
                _ => bail!("ERROR_BADARG list.flatten expects list"),
            }
        }
        Builtin::ListGet => {
            if args.len() != 2 {
                bail!("ERROR_BADARG list.get expects 2 args");
            }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG list.get arg0 must be list"),
            };
            let i = match &args[1] {
                Val::Int(n) => *n,
                _ => bail!("ERROR_BADARG list.get arg1 must be int"),
            };
            if i < 0 {
                bail!("ERROR_OOB list index out of bounds");
            }
            let iu = i as usize;
            if iu >= xs.len() {
                bail!("ERROR_OOB list index out of bounds");
            }
            return Ok(xs[iu].clone());
        }
        Builtin::ListSortByIntKey => {
            if args.len() != 2 {
                bail!("ERROR_BADARG sort_by_int_key expects 2 args");
            }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG sort_by_int_key arg0 must be list"),
            };
            let mut keyed: Vec<(i64, usize, Val)> = Vec::new();
            for (idx, it) in xs.into_iter().enumerate() {
                let k = match &it {
                    Val::Rec(m) => match m.get("k") {
                        Some(Val::Int(n)) => *n,
                        _ => bail!("ERROR_BADARG sort_by_int_key expects rec.k int"),
                    },
                    _ => bail!("ERROR_BADARG sort_by_int_key expects records"),
                };
                keyed.push((k, idx, it));
            }
            keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            let out: Vec<Val> = keyed.into_iter().map(|t| t.2).collect();
            return Ok(Val::List(out));
        }
        Builtin::RecEmpty => {
            if args.len() != 0 {
                bail!("ERROR_BADARG rec.empty expects 0 args");
            }
            Ok(Val::Rec(BTreeMap::new()))
        }
        Builtin::RecKeys => {
            if args.len() != 1 {
                bail!("ERROR_BADARG rec.keys expects 1 arg");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.keys arg0 must be record"),
            };
            let mut out: Vec<Val> = Vec::new();
            for k in m.keys() {
                out.push(Val::Str(k.clone()));
            }
            Ok(Val::List(out))
        }
        Builtin::RecValues => {
            if args.len() != 1 {
                bail!("ERROR_BADARG rec.values expects 1 arg");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.values arg0 must be record"),
            };
            let mut out: Vec<Val> = Vec::new();
            for v in m.values() {
                out.push(v.clone());
            }
            Ok(Val::List(out))
        }
        Builtin::RecHas => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.has expects 2 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.has arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.has arg1 must be string"),
            };
            Ok(Val::Bool(m.contains_key(k)))
        }
        Builtin::RecGet => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.get expects 2 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.get arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.get arg1 must be string"),
            };
            Ok(m.get(k).cloned().unwrap_or(Val::Null))
        }
        Builtin::RecGetOr => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.getOr expects 3 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.getOr arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.getOr arg1 must be string"),
            };
            let d = args[2].clone();
            Ok(m.get(k).cloned().unwrap_or(d))
        }
        Builtin::RecGetOrErr => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.getOrErr expects 3 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.getOrErr arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.getOrErr arg1 must be string"),
            };
            let msg = args[2].clone();
            match m.get(k) {
                Some(v) => Ok(mk_result_ok(v.clone())),
                None => Ok(mk_result_err(msg)),
            }
        }
        Builtin::RecSet => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.set expects 3 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.set arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.set arg1 must be string"),
            };
            let v = args[2].clone();
            let mut out = m.clone();
            out.insert(k.clone(), v);
            Ok(Val::Rec(out))
        }
        Builtin::RecRemove => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.remove expects 2 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.remove arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.remove arg1 must be string"),
            };
            let mut out = m.clone();
            out.remove(k);
            Ok(Val::Rec(out))
        }
        Builtin::RecMerge => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.merge expects 2 args");
            }
            let a = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.merge arg0 must be record"),
            };
            let b = match &args[1] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.merge arg1 must be record"),
            };
            let mut out = a.clone();
            for (k, v) in b.iter() {
                out.insert(k.clone(), v.clone());
            }
            Ok(Val::Rec(out))
        }
        Builtin::RecSelect => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.select expects 2 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.select arg0 must be record"),
            };
            let ks = match &args[1] {
                Val::List(v) => v,
                _ => bail!("ERROR_BADARG rec.select arg1 must be list"),
            };
            let mut out: BTreeMap<String, Val> = BTreeMap::new();
            for x in ks.iter() {
                let k = match x {
                    Val::Str(s) => s,
                    _ => bail!("ERROR_BADARG rec.select keys must be strings"),
                };
                if let Some(v) = m.get(k) {
                    out.insert(k.clone(), v.clone());
                }
            }
            Ok(Val::Rec(out))
        }
        Builtin::RecRename => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.rename expects 3 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.rename arg0 must be record"),
            };
            let a = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.rename arg1 must be string"),
            };
            let b = match &args[2] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.rename arg2 must be string"),
            };
            let mut out = m.clone();
            if let Some(v) = out.remove(a) {
                out.insert(b.clone(), v);
            }
            Ok(Val::Rec(out))
        }
        Builtin::RecUpdate => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.update expects 3 args");
            }
            let m = match &args[0] {
                Val::Rec(mm) => mm,
                _ => bail!("ERROR_BADARG rec.update arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Str(s) => s,
                _ => bail!("ERROR_BADARG rec.update arg1 must be string"),
            };
            let f = args[2].clone();
            let old = m.get(k).cloned().unwrap_or(Val::Null);
            let newv = call(f, vec![old], tracer, loader)?;
            let mut out = m.clone();
            out.insert(k.clone(), newv);
            Ok(Val::Rec(out))
        }
        Builtin::GrowUnfoldTree => {
            if args.len() < 2 {
                bail!("ERROR_BADARG unfold_tree expects at least 2 args");
            }
            let seed = args[0].clone();
            let depth = match &args[1] {
                Val::Rec(m) => match m.get("depth") {
                    Some(Val::Int(n)) => *n,
                    _ => 2,
                },
                _ => 2,
            };
            let mut q: std::collections::VecDeque<(Val, i64)> = std::collections::VecDeque::new();
            q.push_back((seed.clone(), 0));
            while let Some((node, d)) = q.pop_front() {
                tracer.grow_node(&node)?;
                if d >= depth {
                    continue;
                }
                let n = match &node {
                    Val::Rec(m) => match m.get("n") {
                        Some(Val::Int(x)) => *x,
                        _ => 0,
                    },
                    _ => 0,
                };
                let c1 = Val::Rec({
                    let mut m = BTreeMap::new();
                    m.insert("n".to_string(), Val::Int(n + 1));
                    m
                });
                let c2 = Val::Rec({
                    let mut m = BTreeMap::new();
                    m.insert("n".to_string(), Val::Int(n + 2));
                    m
                });
                q.push_back((c1, d + 1));
                q.push_back((c2, d + 1));
            }
            return Ok(Val::Null);
        }
        Builtin::StrTrim => {
            if args.len() != 1 {
                bail!("ERROR_BADARG str.trim expects 1 arg");
            }
            let s = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG str.trim arg0 must be string"),
            };
            Ok(Val::Str(s.trim().to_string()))
        }
        Builtin::StrToLower => {
            if args.len() != 1 {
                bail!("ERROR_BADARG str.toLower expects 1 arg");
            }
            let s = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG str.toLower arg0 must be string"),
            };
            Ok(Val::Str(s.to_ascii_lowercase()))
        }
        Builtin::IntMul => {
            if args.len() != 2 { bail!("ERROR_BADARG int.mul expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Int(a.checked_mul(b).ok_or_else(|| anyhow::anyhow!("ERROR_RUNTIME int overflow"))?))
        }
        Builtin::IntDiv => {
            if args.len() != 2 { bail!("ERROR_BADARG int.div expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            if b == 0 { bail!("ERROR_RUNTIME int divide by zero"); }
            Ok(Val::Int(a / b))
        }
        Builtin::IntSub => {
            if args.len() != 2 { bail!("ERROR_BADARG int.sub expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Int(a.checked_sub(b).ok_or_else(|| anyhow::anyhow!("ERROR_RUNTIME int underflow"))?))
        }
        Builtin::IntMod => {
            if args.len() != 2 { bail!("ERROR_BADARG int.mod expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            if b == 0 { bail!("ERROR_RUNTIME int mod by zero"); }
            Ok(Val::Int(a % b))
        }
        Builtin::IntLt => {
            if args.len() != 2 { bail!("ERROR_BADARG int.lt expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Bool(a < b))
        }
        Builtin::IntGt => {
            if args.len() != 2 { bail!("ERROR_BADARG int.gt expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Bool(a > b))
        }
        Builtin::IntLe => {
            if args.len() != 2 { bail!("ERROR_BADARG int.le expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Bool(a <= b))
        }
        Builtin::IntGe => {
            if args.len() != 2 { bail!("ERROR_BADARG int.ge expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Bool(a >= b))
        }
        Builtin::HashSha256Text => {
            if args.len() != 1 { bail!("ERROR_BADARG hash.sha256_text expects 1 arg"); }
            let s = match &args[0] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_BADARG type") };
            Ok(Val::Str(format!("sha256:{}", sha256_bytes_hex(s.as_bytes()))))
        }
        Builtin::HashSha256Bytes => {
            if args.len() != 1 { bail!("ERROR_BADARG hash.sha256_bytes expects 1 arg"); }
            match &args[0] {
                Val::Str(ss) => Ok(Val::Str(format!("sha256:{}", sha256_bytes_hex(ss.as_bytes())))),
                Val::Bytes(bs) => Ok(Val::Str(format!("sha256:{}", sha256_bytes_hex(bs)))),
                _ => bail!("ERROR_BADARG hash.sha256_bytes expects str or bytes"),
            }
        }
        Builtin::CodecHexEncode => {
            if args.len() != 1 { bail!("ERROR_BADARG codec.hex_encode expects 1 arg"); }
            match &args[0] {
                Val::Bytes(bs) => Ok(Val::Str(hex::encode(bs))),
                Val::Str(ss) => Ok(Val::Str(hex::encode(ss.as_bytes()))),
                _ => bail!("ERROR_BADARG codec.hex_encode expects str or bytes"),
            }
        }
        Builtin::CodecHexDecode => {
            if args.len() != 1 { bail!("ERROR_BADARG codec.hex_decode expects 1 arg"); }
            let s = match &args[0] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_BADARG type") };
            let bytes = hex::decode(s.as_str())?;
            Ok(Val::Str(String::from_utf8(bytes)?))
        }
        Builtin::StrSplit => {
            if args.len() != 2 { bail!("ERROR_BADARG str.split expects 2 args"); }
            let s = match &args[0] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_BADARG str.split arg0 must be string") };
            let delim = match &args[1] { Val::Str(ss) => ss.clone(), _ => bail!("ERROR_BADARG str.split arg1 must be string") };
            let parts: Vec<Val> = s.split(delim.as_str()).map(|p| Val::Str(p.to_string())).collect();
            Ok(Val::List(parts))
        }
        Builtin::StrSplitLines => {
            if args.len() != 1 {
                bail!("ERROR_BADARG str.split_lines expects 1 arg");
            }
            let s = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG str.split_lines arg0 must be string"),
            };
            // .lines() drops trailing empty line and handles \r\n
            let parts: Vec<Val> = s.lines().map(|x| Val::Str(x.to_string())).collect();
            Ok(Val::List(parts))
        }
        Builtin::ListMap => {
            if args.len() != 2 {
                bail!("ERROR_BADARG list.map expects 2 args");
            }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG list.map arg0 must be list"),
            };
            let f = args[1].clone();
            let mut out: Vec<Val> = Vec::with_capacity(xs.len());
            for x in xs {
                out.push(call(f.clone(), vec![x], tracer, loader)?);
            }
            Ok(Val::List(out))
        }
        Builtin::ListFilter => {
            if args.len() != 2 {
                bail!("ERROR_BADARG list.filter expects 2 args");
            }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG list.filter arg0 must be list"),
            };
            let pred = args[1].clone();
            let mut out: Vec<Val> = Vec::new();
            for x in xs {
                let keep = call(pred.clone(), vec![x.clone()], tracer, loader)?;
                match keep {
                    Val::Bool(true) => out.push(x),
                    Val::Bool(false) => {}
                    Val::Int(n) => {
                        if n != 0 {
                            out.push(x)
                        }
                    }
                    _ => bail!("ERROR_BADARG list.filter predicate must return bool"),
                }
            }
            Ok(Val::List(out))
        }
        Builtin::ListRange => {
            if args.len() != 1 && args.len() != 2 {
                bail!("ERROR_BADARG list.range expects 1 or 2 args");
            }
            let (start, end) = if args.len() == 1 {
                (
                    0i64,
                    match &args[0] {
                        Val::Int(n) => *n,
                        _ => bail!("ERROR_BADARG list.range arg0 must be int"),
                    },
                )
            } else {
                let a = match &args[0] {
                    Val::Int(n) => *n,
                    _ => bail!("ERROR_BADARG list.range arg0 must be int"),
                };
                let b = match &args[1] {
                    Val::Int(n) => *n,
                    _ => bail!("ERROR_BADARG list.range arg1 must be int"),
                };
                (a, b)
            };
            if end < start {
                bail!("ERROR_BADARG list.range requires end >= start");
            }
            let mut out: Vec<Val> = Vec::new();
            let mut i = start;
            while i < end {
                out.push(Val::Int(i));
                i += 1;
            }
            Ok(Val::List(out))
        }
        Builtin::ListRepeat => {
            if args.len() != 2 {
                bail!("ERROR_BADARG list.repeat expects 2 args");
            }
            let v = args[0].clone();
            let n = match &args[1] {
                Val::Int(k) => *k,
                _ => bail!("ERROR_BADARG list.repeat arg1 must be int"),
            };
            if n < 0 {
                bail!("ERROR_BADARG list.repeat requires n >= 0");
            }
            let mut out: Vec<Val> = Vec::new();
            let mut i = 0i64;
            while i < n {
                out.push(v.clone());
                i += 1;
            }
            Ok(Val::List(out))
        }
        Builtin::ListConcat => {
            if args.len() != 1 {
                bail!("ERROR_BADARG list.concat expects 1 arg");
            }
            let xss = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG list.concat arg0 must be list"),
            };
            let mut out: Vec<Val> = Vec::new();
            for xs in xss {
                match xs {
                    Val::List(v) => out.extend(v),
                    _ => bail!("ERROR_BADARG list.concat expects list[list[_]]"),
                }
            }
            Ok(Val::List(out))
        }
        Builtin::ListFold => {
            if args.len() != 3 {
                bail!("ERROR_BADARG list.fold expects 3 args");
            }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG list.fold arg0 must be list"),
            };
            let mut acc = args[1].clone();
            let f = args[2].clone();
            for x in xs {
                acc = call(f.clone(), vec![acc, x], tracer, loader)?;
            }
            Ok(acc)
        }
        Builtin::ImportArtifact => {
            if args.len() != 1 {
                bail!("ERROR_BADARG import_artifact expects 1 arg");
            }
            let p = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG import_artifact arg must be string"),
            };
            let disk = tracer.out_dir.join("artifacts").join(&p);
            let bytes = match fs::read(&disk) {
                Ok(b) => b,
                Err(e) => {
                    return Ok(mk_result_err(Val::Str(format!(
                        "ERROR_IO cannot read artifact: {p} ({e})"
                    ))));
                }
            };
            let cid = sha256_bytes(&bytes);
            tracer.artifact_in(&p, &cid)?;
            let text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    return Ok(mk_result_err(Val::Str(format!(
                        "ERROR_UTF8 invalid utf8 in artifact: {p} ({e})"
                    ))));
                }
            };
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("text".to_string(), Val::Str(text));
            rec.insert("cid".to_string(), Val::Str(cid));
            Ok(mk_result_ok(Val::Rec(rec)))
        }

        Builtin::ImportArtifactNamed => {
            if args.len() != 2 {
                bail!("ERROR_BADARG import_artifact_named expects 2 args");
            }
            let name = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG import_artifact_named name must be string"),
            };
            let p = match &args[1] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG import_artifact_named path must be string"),
            };

            let bytes = match fs::read(&p) {
                Ok(b) => b,
                Err(e) => {
                    return Ok(mk_result_err(Val::Str(format!(
                        "ERROR_IO cannot read artifact: {p} ({e})"
                    ))));
                }
            };
            let cid = sha256_bytes(&bytes);
            tracer.artifact_in_named(&name, &p, &cid)?;

            let text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    return Ok(mk_result_err(Val::Str(format!(
                        "ERROR_UTF8 invalid utf8 in artifact: {p} ({e})"
                    ))));
                }
            };

            let mut rec = std::collections::BTreeMap::new();
            rec.insert("text".to_string(), Val::Str(text));
            rec.insert("cid".to_string(), Val::Str(cid));
            Ok(mk_result_ok(Val::Rec(rec)))
        }
        Builtin::EmitArtifact => {
            if args.len() != 2 {
                bail!("ERROR_BADARG emit_artifact expects 2 args");
            }
            let name = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG emit_artifact name must be string"),
            };
            // Accept:
            //  - list[int] bytes
            //  - {text: string} (gate convenience)
            let bytes: Vec<u8> = match &args[1] {
                Val::List(vs) => {
                    let mut out: Vec<u8> = Vec::with_capacity(vs.len());
                    for v in vs {
                        let n = match v {
                            Val::Int(i) => *i,
                            _ => {
                                return Ok(mk_result_err(Val::Str(
                                    "ERROR_BADARG emit_artifact bytes must be ints".to_string(),
                                )));
                            }
                        };
                        if n < 0 || n > 255 {
                            return Ok(mk_result_err(Val::Str(
                                "ERROR_BADARG emit_artifact byte out of range".to_string(),
                            )));
                        }
                        out.push(n as u8);
                    }
                    out
                }
                Val::Rec(m) => match m.get("text") {
                    Some(Val::Str(s)) => s.as_bytes().to_vec(),
                    _ => {
                        return Ok(mk_result_err(Val::Str(
                            "ERROR_BADARG emit_artifact expects bytes:list[int] or {text:string}"
                                .to_string(),
                        )));
                    }
                },
                _ => {
                    return Ok(mk_result_err(Val::Str(
                        "ERROR_BADARG emit_artifact expects bytes:list[int] or {text:string}"
                            .to_string(),
                    )));
                }
            };
            let cid = sha256_bytes(&bytes);
            // tracer.artifact_out writes to out_dir/artifacts/<name> and traces it
            if let Err(e) = tracer.artifact_out(&name, &cid, &bytes) {
                return Ok(mk_result_err(Val::Str(format!(
                    "ERROR_IO cannot write artifact: {name} ({e})"
                ))));
            }
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("name".to_string(), Val::Str(name));
            rec.insert("cid".to_string(), Val::Str(cid));
            Ok(mk_result_ok(Val::Rec(rec)))
        }
        Builtin::EmitArtifactDerived => {
            if args.len() != 4 {
                bail!("ERROR_BADARG emit_artifact_derived expects 4 args");
            }

            let name = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG emit_artifact_derived name must be string"),
            };

            let filename = match &args[1] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG emit_artifact_derived filename must be string"),
            };

            // Payload encoding:
            // - list[int] => raw bytes
            // - {text:string} => utf8 bytes
            // - otherwise any jsonable => compact JSON bytes
            let bytes: Vec<u8> = match &args[2] {
                Val::List(vs) => {
                    let mut out: Vec<u8> = Vec::with_capacity(vs.len());
                    for v in vs {
                        let n = match v {
                            Val::Int(i) => *i,
                            _ => {
                                return Ok(mk_result_err(Val::Str(
                                    "ERROR_BADARG emit_artifact_derived bytes must be ints"
                                        .to_string(),
                                )));
                            }
                        };
                        if n < 0 || n > 255 {
                            return Ok(mk_result_err(Val::Str(
                                "ERROR_BADARG emit_artifact_derived byte out of range".to_string(),
                            )));
                        }
                        out.push(n as u8);
                    }
                    out
                }
                Val::Rec(m) => {
                    if let Some(Val::Str(s)) = m.get("text") {
                        s.as_bytes().to_vec()
                    } else {
                        let j = match args[2].to_json() {
                            Some(j) => j,
                            None => {
                                return Ok(mk_result_err(Val::Str(
                                    "ERROR_BADARG emit_artifact_derived value must be jsonable"
                                        .to_string(),
                                )));
                            }
                        };
                        match serde_json::to_vec(&j) {
                            Ok(b) => b,
                            Err(e) => {
                                return Ok(mk_result_err(Val::Str(format!(
                                    "ERROR_JSON emit_artifact_derived cannot encode json: {e}"
                                ))));
                            }
                        }
                    }
                }
                _ => {
                    let j = match args[2].to_json() {
                        Some(j) => j,
                        None => {
                            return Ok(mk_result_err(Val::Str(
                                "ERROR_BADARG emit_artifact_derived value must be jsonable"
                                    .to_string(),
                            )));
                        }
                    };
                    match serde_json::to_vec(&j) {
                        Ok(b) => b,
                        Err(e) => {
                            return Ok(mk_result_err(Val::Str(format!(
                                "ERROR_JSON emit_artifact_derived cannot encode json: {e}"
                            ))));
                        }
                    }
                }
            };

            let parent_names: Vec<String> = match &args[3] {
                Val::List(xs) => {
                    let mut out: Vec<String> = Vec::new();
                    for x in xs {
                        match x {
                            Val::Str(s) => out.push(s.clone()),
                            _ => {
                                return Ok(mk_result_err(Val::Str(
                "ERROR_BADARG emit_artifact_derived parents must be list[string]".to_string(),
              )));
                            }
                        }
                    }
                    out
                }
                _ => {
                    return Ok(mk_result_err(Val::Str(
                        "ERROR_BADARG emit_artifact_derived parents must be list[string]"
                            .to_string(),
                    )));
                }
            };

            if parent_names.is_empty() {
                return Ok(mk_result_err(Val::Str(
                    "ERROR_BADARG emit_artifact_derived parents must be non-empty".to_string(),
                )));
            }

            let mut parents: Vec<(String, String)> = Vec::new();
            for pn in parent_names {
                let pcid = match tracer.artifact_cids.get(&pn) {
                    Some(s) => s.clone(),
                    None => {
                        bail!("ERROR_M3_PARENT_NOT_DECLARED {pn}");
                    }
                };
                parents.push((pn, pcid));
            }

            let cid = sha256_bytes(&bytes);
            tracer
                .artifact_out_derived(&name, &filename, &cid, &bytes, &parents)
                .map_err(|e| anyhow!("ERROR_IO cannot write artifact: {filename} ({e})"))?;
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("name".to_string(), Val::Str(name));
            rec.insert("cid".to_string(), Val::Str(cid));
            Ok(mk_result_ok(Val::Rec(rec)))
        }

        Builtin::Emit => {
            if args.len() != 1 {
                bail!("emit arity");
            }
            let j = args[0]
                .to_json()
                .ok_or_else(|| anyhow!("emit arg must be jsonable"))?;
            tracer.emit(&j)?;
            Ok(Val::Null)
        }
        Builtin::Len => {
            if args.len() != 1 {
                bail!("len arity");
            }
            match &args[0] {
                Val::List(xs) => Ok(Val::Int(xs.len() as i64)),
                Val::Str(s) => Ok(Val::Int(s.as_bytes().len() as i64)),
                _ => bail!("len expects list or string"),
            }
        }
        Builtin::IntParse => {
            if args.len() != 1 {
                bail!("ERROR_BADARG int.parse expects 1 arg");
            }
            let s = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG int.parse arg0 must be string"),
            };
            match s.trim().parse::<i64>() {
                Ok(n) => Ok(mk_result_ok(Val::Int(n))),
                Err(e) => Ok(mk_result_err(Val::Str(format!(
                    "ERROR_PARSE int.parse ({e})"
                )))),
            }
        }

        Builtin::IntPow => {
            if args.len() != 2 {
                bail!("ERROR_BADARG int.pow expects 2 args");
            }

            let base = match &args[0] {
                Val::Int(n) => *n,

                _ => bail!("ERROR_BADARG int.pow arg0 must be int"),
            };

            let exp = match &args[1] {
                Val::Int(n) => *n,

                _ => bail!("ERROR_BADARG int.pow arg1 must be int"),
            };

            if exp < 0 {
                bail!("ERROR_BADARG int.pow requires exp >= 0");
            }

            let e: u32 = match u32::try_from(exp) {
                Ok(x) => x,

                Err(_) => bail!("ERROR_BADARG int.pow exp too large"),
            };

            let mut acc: i128 = 1;

            let mut i: u32 = 0;

            while i < e {
                acc = match acc.checked_mul(base as i128) {
                    Some(v) => v,

                    None => bail!("ERROR_OVERFLOW int.pow overflow"),
                };

                i += 1;
            }

            if acc < (i64::MIN as i128) || acc > (i64::MAX as i128) {
                bail!("ERROR_OVERFLOW int.pow overflow");
            }

            Ok(Val::Int(acc as i64))
        }

        Builtin::SortInt => {
            if args.len() != 1 {
                bail!("sort_int arity");
            }
            let mut xs = match args[0].clone() {
                Val::List(v) => v,
                _ => bail!("sort_int expects list"),
            };
            let mut ns: Vec<i64> = Vec::new();
            for v in xs.drain(..) {
                match v {
                    Val::Int(n) => ns.push(n),
                    _ => bail!("sort_int expects ints"),
                }
            }
            insertion_sort(&mut ns);
            Ok(Val::List(ns.into_iter().map(Val::Int).collect()))
        }
        Builtin::DedupeSortedInt => {
            if args.len() != 1 {
                bail!("dedupe_sorted_int arity");
            }
            let xs = match args[0].clone() {
                Val::List(v) => v,
                _ => bail!("dedupe_sorted_int expects list"),
            };
            let mut out: Vec<i64> = Vec::new();
            let mut last: Option<i64> = None;
            for v in xs {
                let n = match v {
                    Val::Int(n) => n,
                    _ => bail!("dedupe_sorted_int expects ints"),
                };
                if last.map(|x| x == n).unwrap_or(false) {
                    continue;
                }
                last = Some(n);
                out.push(n);
            }
            Ok(Val::List(out.into_iter().map(Val::Int).collect()))
        }
        Builtin::HistInt => {
            if args.len() != 1 {
                bail!("hist_int arity");
            }
            let xs = match args[0].clone() {
                Val::List(v) => v,
                _ => bail!("hist_int expects list"),
            };
            let mut m: BTreeMap<i64, i64> = BTreeMap::new();
            for v in xs {
                let n = match v {
                    Val::Int(n) => n,
                    _ => bail!("hist_int expects ints"),
                };
                *m.entry(n).or_insert(0) += 1;
            }
            let mut out_list: Vec<Val> = Vec::new();
            for (v, c) in m {
                let mut rec = BTreeMap::new();
                rec.insert("v".to_string(), Val::Int(v));
                rec.insert("count".to_string(), Val::Int(c));
                out_list.push(Val::Rec(rec));
            }
            Ok(Val::List(out_list))
        }
        Builtin::Unfold => {
            if args.len() != 3 {
                bail!("unfold arity");
            }
            let mut seed = args[0].clone();
            let fuel = match &args[1] {
                Val::Int(n) => *n,
                Val::Rec(m) => {
                    if let Some(Val::Int(n)) = m.get("fuel") {
                        *n
                    } else if let Some(Val::Int(n)) = m.get("steps") {
                        *n
                    } else {
                        bail!("unfold opts must include fuel:int or steps:int");
                    }
                }
                _ => bail!("unfold opts must be record or int"),
            };
            let step = args[2].clone();
            let mut out = Vec::new();
            let mut k = 0i64;
            while k < fuel {
                let r = call(step.clone(), vec![seed.clone()], tracer, loader)?;
                match r {
                    Val::Null => break,
                    Val::Rec(m) => {
                        let next_seed = if let Some(v) = m.get("seed").cloned() {
                            v
                        } else if let Some(v) = m.get("i").cloned() {
                            let mut mm = BTreeMap::new();
                            mm.insert("i".to_string(), v);
                            Val::Rec(mm)
                        } else {
                            bail!("unfold step missing seed/i");
                        };
                        let val = m
                            .get("value")
                            .or_else(|| m.get("out"))
                            .cloned()
                            .ok_or_else(|| anyhow!("unfold step missing value/out"))?;
                        out.push(val);
                        seed = next_seed;
                    }
                    Val::List(xs) => {
                        if xs.is_empty() {
                            break;
                        }
                        let m = match &xs[0] {
                            Val::Rec(m) => m,
                            _ => bail!("unfold step list must contain record"),
                        };
                        let next_seed = if let Some(v) = m.get("seed").cloned() {
                            v
                        } else if let Some(v) = m.get("i").cloned() {
                            let mut mm = BTreeMap::new();
                            mm.insert("i".to_string(), v);
                            Val::Rec(mm)
                        } else {
                            bail!("unfold step missing seed/i");
                        };
                        let val = m
                            .get("value")
                            .or_else(|| m.get("out"))
                            .cloned()
                            .ok_or_else(|| anyhow!("unfold step missing value/out"))?;
                        out.push(val);
                        seed = next_seed;
                    }
                    _ => bail!("unfold step must return record, list, or null"),
                }
                k += 1;
            }
            Ok(Val::List(out))
        }
        Builtin::FloatFromInt => {
            match args.first() {
                Some(Val::Int(n)) => Ok(Val::Bytes((*n as f64).to_le_bytes().to_vec())),
                _ => bail!("ERROR_BADARG float.from_int"),
            }
        }
        Builtin::FloatToInt => {
            let f = fb64_1(&args)?;
            Ok(Val::Int(f as i64))
        }
        Builtin::FloatFromText => {
            match args.first() {
                Some(Val::Str(s)) => match s.parse::<f64>() {
                    Ok(v) => Ok(Val::Bytes(v.to_le_bytes().to_vec())),
                    Err(_) => bail!("ERROR_PARSE float.from_text: {}", s),
                },
                _ => bail!("ERROR_BADARG float.from_text"),
            }
        }
        Builtin::FloatToText => {
            let f = fb64_1(&args)?;
            Ok(Val::Str(format!("{}", f)))
        }
        Builtin::FloatAdd => { let (a,b) = fb64_2(&args)?; Ok(fv(a+b)) }
        Builtin::FloatSub => { let (a,b) = fb64_2(&args)?; Ok(fv(a-b)) }
        Builtin::FloatMul => { let (a,b) = fb64_2(&args)?; Ok(fv(a*b)) }
        Builtin::FloatDiv => { let (a,b) = fb64_2(&args)?; Ok(fv(a/b)) }
        Builtin::FloatExp  => { let a = fb64_1(&args)?; Ok(fv(a.exp())) }
        Builtin::FloatLn   => { let a = fb64_1(&args)?; Ok(fv(a.ln())) }
        Builtin::FloatSqrt => { let a = fb64_1(&args)?; Ok(fv(a.sqrt())) }
        Builtin::FloatAbs  => { let a = fb64_1(&args)?; Ok(fv(a.abs())) }
        Builtin::FloatNeg  => { let a = fb64_1(&args)?; Ok(fv(-a)) }
        Builtin::FloatFloor=> { let a = fb64_1(&args)?; Ok(fv(a.floor())) }
        Builtin::FloatCeil => { let a = fb64_1(&args)?; Ok(fv(a.ceil())) }
        Builtin::FloatRound=> { let a = fb64_1(&args)?; Ok(fv(a.round())) }
        Builtin::FloatPow  => { let (a,b) = fb64_2(&args)?; Ok(fv(a.powf(b))) }
        Builtin::FloatLt   => { let (a,b) = fb64_2(&args)?; Ok(Val::Bool(a<b)) }
        Builtin::FloatGt   => { let (a,b) = fb64_2(&args)?; Ok(Val::Bool(a>b)) }
        Builtin::FloatLe   => { let (a,b) = fb64_2(&args)?; Ok(Val::Bool(a<=b)) }
        Builtin::FloatGe   => { let (a,b) = fb64_2(&args)?; Ok(Val::Bool(a>=b)) }
        Builtin::FloatEq   => { let (a,b) = fb64_2(&args)?; Ok(Val::Bool(a==b)) }
        Builtin::FloatNan  => Ok(fv(f64::NAN)),
        Builtin::FloatInf  => Ok(fv(f64::INFINITY)),
        Builtin::FloatIsNan    => { let a = fb64_1(&args)?; Ok(Val::Bool(a.is_nan())) }
        Builtin::FloatIsFinite => { let a = fb64_1(&args)?; Ok(Val::Bool(a.is_finite())) }
        Builtin::FloatMin => { let (a,b) = fb64_2(&args)?; Ok(fv(a.min(b))) }
        Builtin::FloatMax => { let (a,b) = fb64_2(&args)?; Ok(fv(a.max(b))) }
        Builtin::LinalgZeros => {
            match args.first() {
                Some(Val::Int(n)) => Ok(Val::List(vec![fv(0.0); *n as usize])),
                _ => bail!("ERROR_BADARG linalg.zeros"),
            }
        }
        Builtin::LinalgEye => {
            match args.first() {
                Some(Val::Int(n)) => {
                    let n = *n as usize;
                    Ok(Val::List((0..n).map(|i| Val::List((0..n).map(|j| fv(if i==j {1.0} else {0.0})).collect())).collect()))
                }
                _ => bail!("ERROR_BADARG linalg.eye"),
            }
        }
        Builtin::LinalgDot => {
            if args.len() != 2 { bail!("ERROR_ARITY linalg.dot"); }
            let a = vl_to_f64(&args[0])?;
            let b = vl_to_f64(&args[1])?;
            if a.len() != b.len() { bail!("ERROR_BADARG linalg.dot length mismatch"); }
            Ok(fv(a.iter().zip(b.iter()).map(|(x,y)| x*y).sum()))
        }
        Builtin::LinalgNorm => {
            let a = vl_to_f64(&args[0])?;
            Ok(fv(a.iter().map(|x| x*x).sum::<f64>().sqrt()))
        }
        Builtin::LinalgVecAdd => {
            let a = vl_to_f64(&args[0])?;
            let b = vl_to_f64(&args[1])?;
            Ok(Val::List(a.iter().zip(b.iter()).map(|(x,y)| fv(x+y)).collect()))
        }
        Builtin::LinalgVecSub => {
            let a = vl_to_f64(&args[0])?;
            let b = vl_to_f64(&args[1])?;
            Ok(Val::List(a.iter().zip(b.iter()).map(|(x,y)| fv(x-y)).collect()))
        }
        Builtin::LinalgVecScale => {
            let a = vl_to_f64(&args[0])?;
            let s = fb64_1(&args[1..])?;
            Ok(Val::List(a.iter().map(|x| fv(x*s)).collect()))
        }
        Builtin::LinalgTranspose => {
            let m = vl_to_mat(&args[0])?;
            if m.is_empty() { return Ok(Val::List(vec![])); }
            let rows = m.len(); let cols = m[0].len();
            Ok(Val::List((0..cols).map(|j| Val::List((0..rows).map(|i| fv(m[i][j])).collect())).collect()))
        }
        Builtin::LinalgMatvec => {
            let m = vl_to_mat(&args[0])?;
            let x = vl_to_f64(&args[1])?;
            Ok(Val::List(m.iter().map(|row| fv(row.iter().zip(x.iter()).map(|(a,b)| a*b).sum())).collect()))
        }
        Builtin::LinalgMatmul => {
            let a = vl_to_mat(&args[0])?;
            let b = vl_to_mat(&args[1])?;
            if a.is_empty() || b.is_empty() { return Ok(Val::List(vec![])); }
            let (rm, k, rn) = (a.len(), a[0].len(), b[0].len());
            Ok(Val::List((0..rm).map(|i| Val::List((0..rn).map(|j| fv((0..k).map(|l| a[i][l]*b[l][j]).sum())).collect())).collect()))
        }
        Builtin::LinalgMatAdd => {
            let a = vl_to_mat(&args[0])?;
            let b = vl_to_mat(&args[1])?;
            Ok(Val::List(a.iter().zip(b.iter()).map(|(ra,rb)| Val::List(ra.iter().zip(rb.iter()).map(|(x,y)| fv(x+y)).collect())).collect()))
        }
        Builtin::LinalgMatScale => {
            let m = vl_to_mat(&args[0])?;
            let s = fb64_1(&args[1..])?;
            Ok(Val::List(m.iter().map(|r| Val::List(r.iter().map(|x| fv(x*s)).collect())).collect()))
        }
        Builtin::LinalgEigh => {
            use nalgebra::{DMatrix, SymmetricEigen};
            let m = vl_to_mat(&args[0])?;
            let n = m.len();
            if n == 0 { let mut m = BTreeMap::new(); m.insert("vals".into(), Val::List(vec![])); m.insert("vecs".into(), Val::List(vec![])); return Ok(Val::Rec(m)); }
            let flat: Vec<f64> = m.iter().flat_map(|r| r.iter().cloned()).collect();
            let na_mat = DMatrix::from_row_slice(n, n, &flat);
            let eig = SymmetricEigen::new(na_mat);
            let vals: Vec<Val> = eig.eigenvalues.iter().map(|&v| fv(v)).collect();
            // Store as rows: vecs[i] = i-th eigenvector (row), so matvec(vecs, x) = V^T x
            let vecs: Vec<Val> = (0..n).map(|i| Val::List((0..n).map(|j| fv(eig.eigenvectors[(j,i)])).collect())).collect();
            { let mut m = BTreeMap::new(); m.insert("vals".into(), Val::List(vals)); m.insert("vecs".into(), Val::List(vecs)); Ok(Val::Rec(m)) }
        }
    }
}

fn fv(f: f64) -> Val { Val::Bytes(f.to_le_bytes().to_vec()) }

fn fb64_1(args: &[Val]) -> anyhow::Result<f64> {
    match args.first() {
        Some(Val::Bytes(b)) if b.len() == 8 => {
            let arr: [u8;8] = b.as_slice().try_into().unwrap();
            Ok(f64::from_le_bytes(arr))
        }
        Some(Val::Int(n)) => Ok(*n as f64),
        _ => anyhow::bail!("ERROR_BADARG expected float (8-byte Bytes)"),
    }
}

fn fb64_2(args: &[Val]) -> anyhow::Result<(f64,f64)> {
    if args.len() < 2 { anyhow::bail!("ERROR_ARITY expected 2 float args"); }
    Ok((fb64_1(&args[0..1])?, fb64_1(&args[1..2])?))
}

fn vl_to_f64(v: &Val) -> anyhow::Result<Vec<f64>> {
    match v {
        Val::List(items) => items.iter().map(|x| fb64_1(std::slice::from_ref(x))).collect(),
        _ => anyhow::bail!("ERROR_BADARG expected list of floats"),
    }
}

fn vl_to_mat(v: &Val) -> anyhow::Result<Vec<Vec<f64>>> {
    match v {
        Val::List(rows) => rows.iter().map(vl_to_f64).collect(),
        _ => anyhow::bail!("ERROR_BADARG expected list of list of floats"),
    }
}

fn insertion_sort(xs: &mut [i64]) {
    for i in 1..xs.len() {
        let key = xs[i];
        let mut j = i;
        while j > 0 && xs[j - 1] > key {
            xs[j] = xs[j - 1];
            j -= 1;
        }
        xs[j] = key;
    }
}
struct Lockfile {
    modules: HashMap<String, String>,
}
impl Lockfile {
    fn load(p: &Path) -> Result<Self> {
        let bytes = match fs::read(p) {
            Ok(b) => b,
            Err(e) => {
                // If the program has chdir’d (e.g., into --out), a relative lock path
                // like "fard.lock.json" will fail. Retry against the shell’s original PWD.
                if e.kind() == std::io::ErrorKind::NotFound && !p.is_absolute() {
                    if let Ok(pwd) = std::env::var("PWD") {
                        let alt = std::path::Path::new(&pwd).join(p);
                        fs::read(&alt).with_context(|| {
                            format!(
                                "missing lock file: {} (also tried {})",
                                p.display(),
                                alt.display()
                            )
                        })?
                    } else {
                        return Err(e)
                            .with_context(|| format!("missing lock file: {}", p.display()));
                    }
                } else {
                    return Err(e).with_context(|| format!("missing lock file: {}", p.display()));
                }
            }
        };
        let v: J = serde_json::from_slice(&bytes)?;
        let mut modules: HashMap<String, String> = HashMap::new();
        // Accept either:
        //  (A) object map:
        //      "modules": { "spec": "sha256:..." }
        //      "modules": { "spec": { "digest": "sha256:..." } }
        //  (B) array of entries (fixture format):
        //      "modules": [ { "spec": "...", "digest": "...", "path": "..." }, ... ]
        if let Some(ms) = v.get("modules").and_then(|x| x.as_object()) {
            for (k, vv) in ms {
                let dig = if let Some(s) = vv.as_str() {
                    s.to_string()
                } else {
                    vv.get("digest")
                        .and_then(|x| x.as_str())
                        .ok_or_else(|| {
                            anyhow!(
"ERROR_LOCK modules values must be string. expected string or object with digest key"
)
                        })?
                        .to_string()
                };
                if dig.is_empty() {
                    bail!("ERROR_LOCK modules digest empty");
                }
                modules.insert(k.clone(), dig);
            }
        } else if let Some(arr) = v.get("modules").and_then(|x| x.as_array()) {
            for it in arr {
                let spec = it
                    .get("spec")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| anyhow!("ERROR_LOCK modules array entry missing spec"))?;
                let dig = it
                    .get("digest")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| anyhow!("ERROR_LOCK modules array entry missing digest"))?;
                if dig.is_empty() {
                    bail!("ERROR_LOCK modules digest empty");
                }
                modules.insert(spec.to_string(), dig.to_string());
            }
        }
        Ok(Self { modules })
    }
    fn expected(&self, k: &str) -> Option<&str> {
        self.modules.get(k).map(|s| s.as_str())
    }
}
#[derive(Copy, Clone, Debug)]
enum ModKind {
    Std,
    Pkg,
    Rel,
}
#[derive(Clone, Debug)]
struct ModNode {
    id: usize,
    spec: String,
    kind: ModKind,
    path: Option<String>,
    digest: Option<String>,
}
#[derive(Clone, Debug)]
struct ModEdge {
    from: usize,
    to: usize,
    kind: String,
}
#[derive(Clone, Debug)]
struct ModuleGraph {
    nodes: Vec<ModNode>,
    edges: Vec<ModEdge>,
    index: HashMap<String, usize>,
}
impl ModuleGraph {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            index: HashMap::new(),
        }
    }
    fn intern_node(
        &mut self,
        spec: &str,
        kind: ModKind,
        path: Option<String>,
        digest: Option<String>,
    ) -> usize {
        if let Some(id) = self.index.get(spec) {
            let i = *id;
            if self.nodes[i].path.is_none() {
                self.nodes[i].path = path;
            }
            if self.nodes[i].digest.is_none() {
                self.nodes[i].digest = digest;
            }
            return i;
        }
        let id = self.nodes.len();
        self.index.insert(spec.to_string(), id);
        self.nodes.push(ModNode {
            id,
            spec: spec.to_string(),
            kind,
            path,
            digest,
        });
        id
    }
    fn add_edge(&mut self, from: usize, to: usize) {
        self.edges.push(ModEdge {
            from,
            to,
            kind: "import".to_string(),
        });
    }
    fn to_json(&self) -> J {
        let mut root = Map::new();
        let mut ns: Vec<J> = Vec::new();
        for n in &self.nodes {
            let mut m = Map::new();
            m.insert("id".to_string(), J::Number((n.id as u64).into()));
            m.insert("spec".to_string(), J::String(n.spec.clone()));
            m.insert(
                "kind".to_string(),
                J::String(
                    match n.kind {
                        ModKind::Std => "std",
                        ModKind::Pkg => "pkg",
                        ModKind::Rel => "rel",
                    }
                    .to_string(),
                ),
            );
            if let Some(p) = &n.path {
                m.insert("path".to_string(), J::String(p.clone()));
            }
            if let Some(d) = &n.digest {
                m.insert("digest".to_string(), J::String(d.clone()));
            }
            ns.push(J::Object(m));
        }
        let mut es: Vec<J> = Vec::new();
        for e in &self.edges {
            let mut m = Map::new();
            m.insert("from".to_string(), J::Number((e.from as u64).into()));
            m.insert("to".to_string(), J::Number((e.to as u64).into()));
            m.insert("kind".to_string(), J::String(e.kind.clone()));
            es.push(J::Object(m));
        }
        root.insert("nodes".to_string(), J::Array(ns));
        root.insert("edges".to_string(), J::Array(es));
        J::Object(root)
    }
}
struct ModuleLoader {
    root_dir: PathBuf,
    registry_dir: Option<PathBuf>,
    cache: HashMap<String, BTreeMap<String, Val>>,
    stack: Vec<String>,
    lock: Option<Lockfile>,
    graph: ModuleGraph,
    current: Option<usize>,
}
impl ModuleLoader {
    fn new(root: &Path) -> Self {
        Self {
            root_dir: root.to_path_buf(),
            registry_dir: None,
            cache: HashMap::new(),
            stack: Vec::new(),
            lock: None,
            graph: ModuleGraph::new(),
            current: None,
        }
    }
    fn graph_note_import(
        &mut self,
        callee_spec: &str,
        callee_kind: ModKind,
        callee_path: Option<String>,
        callee_digest: Option<String>,
    ) -> usize {
        let callee_id =
            self.graph
                .intern_node(callee_spec, callee_kind, callee_path, callee_digest);
        if let Some(from) = self.current {
            self.graph.add_edge(from, callee_id);
        }
        callee_id
    }

    fn graph_note_current(&mut self, callee_path: Option<String>, callee_digest: Option<String>) {
        if let Some(id) = self.current {
            if self.graph.nodes[id].path.is_none() {
                self.graph.nodes[id].path = callee_path;
            }
            if self.graph.nodes[id].digest.is_none() {
                self.graph.nodes[id].digest = callee_digest;
            }
        }
    }

    fn with_current<T>(&mut self, id: usize, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let prev = self.current;
        self.current = Some(id);
        let out = f(self);
        self.current = prev;
        out
    }
    fn eval_main(&mut self, main_path: &Path, tracer: &mut Tracer) -> Result<Val> {
        let src = fs::read_to_string(main_path)
            .with_context(|| format!("missing main program file: {}", main_path.display()))?;
        let file = main_path.to_string_lossy().to_string();
        let main_spec = file.clone();
        let main_digest = file_digest(main_path).ok();
        let main_id = self.graph.intern_node(
            &main_spec,
            ModKind::Rel,
            Some(main_spec.clone()),
            main_digest,
        );

        let mut p = Parser::from_src(&src, &file)?;
        let items = p.parse_module()?;
        let mut env = base_env();
        let here_dir = main_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.root_dir.clone());

        let v = self.with_current(main_id, |slf| {
            slf.eval_items(items, &mut env, tracer, &here_dir)
        })?;
        Ok(v)
    }
    fn eval_items(
        &mut self,
        items: Vec<Item>,
        env: &mut Env,
        tracer: &mut Tracer,
        here: &Path,
    ) -> Result<Val> {
        let mut exports: Option<Vec<String>> = None;
        let mut last: Val = Val::Null;
        for it in items {
            match it {
                Item::Import(path, alias) => {
                    let ex = self.load_module(&path, here, tracer)?;
                    env.set(alias, Val::Rec(ex));
                }
                Item::Let(name, rhs, span) => {
                    let v = eval(&rhs, env, tracer, self).map_err(|e| {
                        if let Some(sp) = &span {
                            e.context(format!("  --> {}:{}:{}", sp.file, sp.line, sp.col))
                        } else { e }
                    })?;
                    env.set(name, v);
                }
                Item::Fn(name, params, _ret, body) => {
                    let f = Val::Func(Func {
                        params: params.into_iter().map(|(p, _)| p).collect(),
                        body,
                        env: env.clone(),
                    });
                    env.set(name, f);
                }
                Item::Export(ns) => exports = Some(ns),
                Item::Expr(e, span) => {
                    last = eval(&e, env, tracer, self).map_err(|e| {
                        if let Some(sp) = &span {
                            e.context(format!("  --> {}:{}:{}", sp.file, sp.line, sp.col))
                        } else { e }
                    })?;
                }
            }
        }
        if let Some(ns) = exports {
            let mut out = BTreeMap::new();
            for n in ns {
                let v = env
                    .get(&n)
                    .ok_or_else(|| anyhow!("export missing name: {n}"))?;
                out.insert(n, v);
            }
            return Ok(Val::Rec(out));
        }
        Ok(last)
    }
    fn load_module(
        &mut self,
        name: &str,
        here: &Path,
        tracer: &mut Tracer,
    ) -> Result<BTreeMap<String, Val>> {
        if let Some(c) = self.cache.get(name) {
            return Ok(c.clone());
        }
        if self.stack.contains(&name.to_string()) {
            bail!("IMPORT_CYCLE cycle detected in imports at {name}");
        }
        self.stack.push(name.to_string());
        let (kind, digest0) = if name.starts_with("std/") {
            (ModKind::Std, Some(self.builtin_digest(name)))
        } else if name.starts_with("pkg:") || name.starts_with("pkg/") {
            (ModKind::Pkg, None)
        } else {
            (ModKind::Rel, None)
        };
        let callee_id = self.graph_note_import(name, kind, None, digest0);
        let exports = self.with_current(callee_id, |slf| {
            let exports = if name.starts_with("std/") {
                let ex = slf.builtin_std(name)?;
                slf.check_lock(name, &slf.builtin_digest(name))?;
                tracer.module_resolve(name, "std", &slf.builtin_digest(name))?;
                ex
            } else if name.starts_with("pkg:") || name.starts_with("pkg/") {
                if slf.lock.is_none() {
                    eprintln!("IMPORT_PKG_REQUIRES_LOCK");
                    bail!("ERROR_LOCK missing lock for pkg import: {name}");
                }
                let reg = slf
                    .registry_dir
                    .as_ref()
                    .ok_or_else(|| anyhow!("ERROR_REGISTRY missing --registry"))?;
                let spec = if let Some(s) = name.strip_prefix("pkg:") {
                    s
                } else if let Some(s) = name.strip_prefix("pkg/") {
                    s
                } else {
                    name
                };
                let (pkg, rest) = spec
                    .split_once("@")
                    .ok_or_else(|| anyhow!("ERROR_RUNTIME bad pkg import: {name}"))?;
                let (ver, mod_id) = rest
                    .split_once("/")
                    .ok_or_else(|| anyhow!("ERROR_RUNTIME bad pkg import: {name}"))?;
                let base = reg.join("pkgs").join(pkg).join(ver);
                let pkg_json_path = base.join("package.json");
                let rel: String = if let Ok(pkg_json_bytes) = fs::read(&pkg_json_path) {
                    let pkg_json: J = serde_json::from_slice(&pkg_json_bytes)
                        .with_context(|| format!("bad json: {}", pkg_json_path.display()))?;
                    let entrypoints = pkg_json
                        .get("entrypoints")
                        .and_then(|x| x.as_object())
                        .ok_or_else(|| anyhow!("ERROR_RUNTIME package.json missing entrypoints"))?;
                    entrypoints
                        .get(mod_id)
                        .and_then(|x| x.as_str())
                        .ok_or_else(|| {
                            anyhow!("ERROR_RUNTIME missing entrypoint {mod_id} in package.json")
                        })?
                        .to_string()
                } else {
                    format!("{mod_id}.fard")
                };
                let path = base.join("files").join(&rel);

                let p = path.to_string_lossy().to_string();
                let exp = slf
                    .lock
                    .as_ref()
                    .and_then(|lk| lk.expected(name))
                    .ok_or_else(|| anyhow!("ERROR_LOCK missing lock for pkg import: {name}"))?;
                let got = file_digest(&path).unwrap_or_else(|_| exp.to_string());
                slf.check_lock(name, &got)?;
                slf.graph_note_current(Some(p), Some(got.clone()));
                tracer.module_resolve(name, "pkg", &got)?;
                let src = fs::read_to_string(&path)
                    .with_context(|| format!("missing module file: {}", path.display()))?;
                let file = path.to_string_lossy().to_string();
                let mut p = Parser::from_src(&src, &file)?;
                let items = p.parse_module()?;
                let mut env = base_env();
                let v = slf.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
                match v {
                    Val::Rec(m) => m,
                    _ => bail!("module must export a record"),
                }
            } else if name.starts_with("registry/") {
                let reg = slf
                    .registry_dir
                    .as_ref()
                    .ok_or_else(|| anyhow!("ERROR_REGISTRY missing --registry"))?;
                let rest = name.strip_prefix("registry/").unwrap_or(name);
                let path = reg.join(format!("{rest}.fard"));
                let src = fs::read_to_string(&path).with_context(|| {
                    if path.to_string_lossy().contains("/pkg/") {
                        eprintln!("IMPORT_PKG_REQUIRES_LOCK");
                    }
                    format!("missing module file: {}", path.display())
                })?;
                slf.check_lock(name, &file_digest(&path)?)?;
                tracer.module_resolve(name, "registry", &file_digest(&path)?)?;
                let file = path.to_string_lossy().to_string();
                let mut p = Parser::from_src(&src, &file)?;
                let items = p.parse_module()?;
                let mut env = base_env();
                let v = slf.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
                match v {
                    Val::Rec(m) => m,
                    _ => bail!("module must export a record"),
                }
            } else {
                let base: &Path = if name.starts_with("lib/") {
                    slf.root_dir.as_path()
                } else {
                    here
                };
                let path = base.join(format!("{name}.fard"));
                let src = fs::read_to_string(&path).with_context(|| {
                    if path.to_string_lossy().contains("/pkg/") {
                        eprintln!("IMPORT_PKG_REQUIRES_LOCK");
                    }
                    format!("missing module file: {}", path.display())
                })?;
                slf.check_lock(name, &file_digest(&path)?)?;
                tracer.module_resolve(name, "rel", &file_digest(&path)?)?;
                let file = path.to_string_lossy().to_string();
                let mut p = Parser::from_src(&src, &file)?;
                let items = p.parse_module()?;
                let mut env = base_env();
                let v = slf.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
                match v {
                    Val::Rec(m) => m,
                    _ => bail!("module must export a record"),
                }
            };
            Ok(exports)
        })?;
        self.stack.pop();
        self.cache.insert(name.to_string(), exports.clone());
        Ok(exports)
    }
    fn check_lock(&self, module: &str, got: &str) -> Result<()> {
        if let Some(lk) = &self.lock {
            if let Some(exp) = lk.expected(module) {
                if exp != got {
                    bail!("LOCK_MISMATCH lock mismatch for module {module}: expected {exp}, got {got}");
                }
            }
        }
        Ok(())
    }
    fn builtin_std(&self, name: &str) -> Result<BTreeMap<String, Val>> {
        if name == "std/record" {
            return self.builtin_std("std/rec");
        }

        match name {
            "std/list" => {
                let mut m = BTreeMap::new();
                m.insert("len".to_string(), Val::Builtin(Builtin::Len));
                m.insert("range".to_string(), Val::Builtin(Builtin::ListRange));
                m.insert("repeat".to_string(), Val::Builtin(Builtin::ListRepeat));
                m.insert("concat".to_string(), Val::Builtin(Builtin::ListConcat));
                m.insert("fold".to_string(), Val::Builtin(Builtin::ListFold));
                m.insert("map".to_string(), Val::Builtin(Builtin::ListMap));
                m.insert("filter".to_string(), Val::Builtin(Builtin::ListFilter));
                m.insert("get".to_string(), Val::Builtin(Builtin::ListGet));
                m.insert("len".to_string(), Val::Builtin(Builtin::ListLen));
                m.insert("head".to_string(), Val::Builtin(Builtin::ListHead));
                m.insert("tail".to_string(), Val::Builtin(Builtin::ListTail));
                m.insert("append".to_string(), Val::Builtin(Builtin::ListAppend));
                m.insert("zip".to_string(), Val::Builtin(Builtin::ListZip));
                m.insert("reverse".to_string(), Val::Builtin(Builtin::ListReverse));
                m.insert("flatten".to_string(), Val::Builtin(Builtin::ListFlatten));
                m.insert(
                    "sort_by_int_key".to_string(),
                    Val::Builtin(Builtin::ListSortByIntKey),
                );
                m.insert("sort_int".to_string(), Val::Builtin(Builtin::SortInt));
                m.insert(
                    "dedupe_sorted_int".to_string(),
                    Val::Builtin(Builtin::DedupeSortedInt),
                );
                m.insert("hist_int".to_string(), Val::Builtin(Builtin::HistInt));
                Ok(m)
            }
            "std/result" => {
                let mut m = BTreeMap::new();
                m.insert("ok".to_string(), Val::Builtin(Builtin::ResultOk));
                m.insert("err".to_string(), Val::Builtin(Builtin::ResultErr));
                m.insert("andThen".to_string(), Val::Builtin(Builtin::ResultAndThen));
                m.insert(
                    "unwrap_ok".to_string(),
                    Val::Builtin(Builtin::ResultUnwrapOk),
                );
                m.insert(
                    "unwrap_err".to_string(),
                    Val::Builtin(Builtin::ResultUnwrapErr),
                );
                Ok(m)
            }
            "std/grow" => {
                let mut m = BTreeMap::new();
                m.insert("append".to_string(), Val::Builtin(Builtin::GrowAppend));
                m.insert("merge".to_string(), Val::Builtin(Builtin::RecMerge));
                m.insert(
                    "unfold_tree".to_string(),
                    Val::Builtin(Builtin::GrowUnfoldTree),
                );
                m.insert("unfold".to_string(), Val::Builtin(Builtin::Unfold));
                Ok(m)
            }
            "std/flow" => {
                let mut m = BTreeMap::new();
                m.insert("id".to_string(), Val::Builtin(Builtin::FlowId));
                m.insert("pipe".to_string(), Val::Builtin(Builtin::FlowPipe));
                m.insert("tap".to_string(), Val::Builtin(Builtin::FlowTap));
                Ok(m)
            }
            "std/str" => {
                let mut m = BTreeMap::new();
                m.insert("len".to_string(), Val::Builtin(Builtin::StrLen));
                m.insert("trim".to_string(), Val::Builtin(Builtin::StrTrim));
                m.insert(
                    "split_lines".to_string(),
                    Val::Builtin(Builtin::StrSplitLines),
                );
                m.insert("toLower".to_string(), Val::Builtin(Builtin::StrToLower));
                m.insert("lower".to_string(), Val::Builtin(Builtin::StrToLower));
                m.insert("concat".to_string(), Val::Builtin(Builtin::StrConcat));
                m.insert("split".to_string(), Val::Builtin(Builtin::StrSplit));
                Ok(m)
            }
            "std/map" => {
                let mut m = BTreeMap::new();
                m.insert("get".to_string(), Val::Builtin(Builtin::MapGet));
                m.insert("set".to_string(), Val::Builtin(Builtin::MapSet));
                m.insert("keys".to_string(), Val::Builtin(Builtin::RecKeys));
                m.insert("values".to_string(), Val::Builtin(Builtin::RecValues));
                m.insert("has".to_string(), Val::Builtin(Builtin::RecHas));
                Ok(m)
            }
            "std/rec" => {
                let mut m = BTreeMap::new();
                m.insert("empty".to_string(), Val::Builtin(Builtin::RecEmpty));
                m.insert("keys".to_string(), Val::Builtin(Builtin::RecKeys));
                m.insert("values".to_string(), Val::Builtin(Builtin::RecValues));
                m.insert("has".to_string(), Val::Builtin(Builtin::RecHas));
                m.insert("get".to_string(), Val::Builtin(Builtin::RecGet));
                m.insert("getOr".to_string(), Val::Builtin(Builtin::RecGetOr));
                m.insert("getOrErr".to_string(), Val::Builtin(Builtin::RecGetOrErr));
                m.insert("set".to_string(), Val::Builtin(Builtin::RecSet));
                m.insert("remove".to_string(), Val::Builtin(Builtin::RecRemove));
                m.insert("merge".to_string(), Val::Builtin(Builtin::RecMerge));
                m.insert("select".to_string(), Val::Builtin(Builtin::RecSelect));
                m.insert("rename".to_string(), Val::Builtin(Builtin::RecRename));
                m.insert("update".to_string(), Val::Builtin(Builtin::RecUpdate));
                Ok(m)
            }
            "std/json" => {
                let mut m = BTreeMap::new();
                m.insert("encode".to_string(), Val::Builtin(Builtin::JsonEncode));
                m.insert("decode".to_string(), Val::Builtin(Builtin::JsonDecode));
                m.insert("canonicalize".to_string(), Val::Builtin(Builtin::JsonCanonicalize));
                Ok(m)
            }

            "std/int" => {
                let mut m = BTreeMap::new();
                m.insert("add".to_string(), Val::Builtin(Builtin::IntAdd));
                m.insert("eq".to_string(), Val::Builtin(Builtin::IntEq));
                m.insert("parse".to_string(), Val::Builtin(Builtin::IntParse));
                m.insert("pow".to_string(), Val::Builtin(Builtin::IntPow));
                m.insert("mul".to_string(), Val::Builtin(Builtin::IntMul));
                m.insert("div".to_string(), Val::Builtin(Builtin::IntDiv));
                m.insert("sub".to_string(), Val::Builtin(Builtin::IntSub));
                m.insert("mod".to_string(), Val::Builtin(Builtin::IntMod));
                m.insert("lt".to_string(), Val::Builtin(Builtin::IntLt));
                m.insert("gt".to_string(), Val::Builtin(Builtin::IntGt));
                m.insert("le".to_string(), Val::Builtin(Builtin::IntLe));
                m.insert("ge".to_string(), Val::Builtin(Builtin::IntGe));
                Ok(m)
            }
            "std/option" => {
                let mut m = BTreeMap::new();
                m.insert("None".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("Some".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("isNone".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("isSome".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert(
                    "fromNullable".to_string(),
                    Val::Builtin(Builtin::Unimplemented),
                );
                m.insert(
                    "toNullable".to_string(),
                    Val::Builtin(Builtin::Unimplemented),
                );
                m.insert("map".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("andThen".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("unwrapOr".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert(
                    "unwrapOrElse".to_string(),
                    Val::Builtin(Builtin::Unimplemented),
                );
                m.insert("toResult".to_string(), Val::Builtin(Builtin::Unimplemented));
                Ok(m)
            }
            "std/null" => {
                let mut m = BTreeMap::new();
                m.insert("isNull".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("coalesce".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert(
                    "guardNotNull".to_string(),
                    Val::Builtin(Builtin::Unimplemented),
                );
                Ok(m)
            }
            "std/path" => {
                let mut m = BTreeMap::new();
                m.insert("base".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("dir".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("ext".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("isAbs".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("join".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("joinAll".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert(
                    "normalize".to_string(),
                    Val::Builtin(Builtin::Unimplemented),
                );
                Ok(m)
            }
            "std/time" => {
                let mut m = BTreeMap::new();
                m.insert("add".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("sub".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("format".to_string(), Val::Builtin(Builtin::Unimplemented));
                let mut d = BTreeMap::new();
                d.insert("ms".to_string(), Val::Builtin(Builtin::Unimplemented));
                d.insert("sec".to_string(), Val::Builtin(Builtin::Unimplemented));
                d.insert("min".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("Duration".to_string(), Val::Rec(d));
                Ok(m)
            }
            "std/trace" => {
                let mut m = BTreeMap::new();
                m.insert("emit".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("info".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("warn".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("error".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("span".to_string(), Val::Builtin(Builtin::Unimplemented));
                Ok(m)
            }
            "std/artifact" => {
                let mut m = BTreeMap::new();
                m.insert("import".to_string(), Val::Builtin(Builtin::ImportArtifact));
                m.insert("emit".to_string(), Val::Builtin(Builtin::EmitArtifact));
                m.insert("ref".to_string(), Val::Builtin(Builtin::Unimplemented));
                m.insert("derive".to_string(), Val::Builtin(Builtin::Unimplemented));
                Ok(m)
            }
            "std/bytes" => {
                let m: BTreeMap<String, Val> = BTreeMap::new();
                Ok(m)
            }

            "std/codec" => {
                let mut m = BTreeMap::new();
                m.insert("base64url_encode".to_string(), Val::Builtin(Builtin::CodecBase64UrlEncode));
                m.insert("base64url_decode".to_string(), Val::Builtin(Builtin::CodecBase64UrlDecode));
                m.insert("hex_encode".to_string(), Val::Builtin(Builtin::CodecHexEncode));
                m.insert("hex_decode".to_string(), Val::Builtin(Builtin::CodecHexDecode));
                Ok(m)
            }
            "std/env" => {
                let m: BTreeMap<String, Val> = BTreeMap::new();
                Ok(m)
            }
            "std/fs" => {
                let m: BTreeMap<String, Val> = BTreeMap::new();
                Ok(m)
            }
            "std/hash" => {
                let mut m = BTreeMap::new();
                m.insert("sha256_bytes".to_string(), Val::Builtin(Builtin::HashSha256Bytes));
                m.insert("sha256_text".to_string(), Val::Builtin(Builtin::HashSha256Text));
                Ok(m)
            }
            "std/http" => {
                let m: BTreeMap<String, Val> = BTreeMap::new();
                Ok(m)
            }
            "std/record" => {
                let m: BTreeMap<String, Val> = BTreeMap::new();
                Ok(m)
            }

            "std/png" => {
                let mut m = BTreeMap::new();

                m.insert("red_1x1".to_string(), Val::Builtin(Builtin::PngRed1x1));

                Ok(m)
            }

            "std/rand" => {
                let mut m = BTreeMap::new();
                m.insert("uuid_v4".to_string(), Val::Builtin(Builtin::RandUuidV4));
                Ok(m)
            }
            "std/crypto" => {
                let mut m = BTreeMap::new();
                m.insert("ed25519_verify".to_string(), Val::Builtin(Builtin::CryptoEd25519Verify));
                m.insert("hmac_sha256".to_string(), Val::Builtin(Builtin::CryptoHmacSha256));
                Ok(m)
            }
            "std/float" => {
                let mut m = BTreeMap::new();
                m.insert("from_int".to_string(), Val::Builtin(Builtin::FloatFromInt));
                m.insert("to_int".to_string(), Val::Builtin(Builtin::FloatToInt));
                m.insert("from_text".to_string(), Val::Builtin(Builtin::FloatFromText));
                m.insert("to_text".to_string(), Val::Builtin(Builtin::FloatToText));
                m.insert("add".to_string(), Val::Builtin(Builtin::FloatAdd));
                m.insert("sub".to_string(), Val::Builtin(Builtin::FloatSub));
                m.insert("mul".to_string(), Val::Builtin(Builtin::FloatMul));
                m.insert("div".to_string(), Val::Builtin(Builtin::FloatDiv));
                m.insert("exp".to_string(), Val::Builtin(Builtin::FloatExp));
                m.insert("ln".to_string(), Val::Builtin(Builtin::FloatLn));
                m.insert("sqrt".to_string(), Val::Builtin(Builtin::FloatSqrt));
                m.insert("pow".to_string(), Val::Builtin(Builtin::FloatPow));
                m.insert("abs".to_string(), Val::Builtin(Builtin::FloatAbs));
                m.insert("neg".to_string(), Val::Builtin(Builtin::FloatNeg));
                m.insert("floor".to_string(), Val::Builtin(Builtin::FloatFloor));
                m.insert("ceil".to_string(), Val::Builtin(Builtin::FloatCeil));
                m.insert("round".to_string(), Val::Builtin(Builtin::FloatRound));
                m.insert("lt".to_string(), Val::Builtin(Builtin::FloatLt));
                m.insert("gt".to_string(), Val::Builtin(Builtin::FloatGt));
                m.insert("le".to_string(), Val::Builtin(Builtin::FloatLe));
                m.insert("ge".to_string(), Val::Builtin(Builtin::FloatGe));
                m.insert("eq".to_string(), Val::Builtin(Builtin::FloatEq));
                m.insert("nan".to_string(), Val::Builtin(Builtin::FloatNan));
                m.insert("inf".to_string(), Val::Builtin(Builtin::FloatInf));
                m.insert("is_nan".to_string(), Val::Builtin(Builtin::FloatIsNan));
                m.insert("is_finite".to_string(), Val::Builtin(Builtin::FloatIsFinite));
                m.insert("min".to_string(), Val::Builtin(Builtin::FloatMin));
                m.insert("max".to_string(), Val::Builtin(Builtin::FloatMax));
                Ok(m)
            }
            "std/linalg" => {
                let mut m = BTreeMap::new();
                m.insert("dot".to_string(), Val::Builtin(Builtin::LinalgDot));
                m.insert("norm".to_string(), Val::Builtin(Builtin::LinalgNorm));
                m.insert("zeros".to_string(), Val::Builtin(Builtin::LinalgZeros));
                m.insert("eye".to_string(), Val::Builtin(Builtin::LinalgEye));
                m.insert("matvec".to_string(), Val::Builtin(Builtin::LinalgMatvec));
                m.insert("matmul".to_string(), Val::Builtin(Builtin::LinalgMatmul));
                m.insert("transpose".to_string(), Val::Builtin(Builtin::LinalgTranspose));
                m.insert("eigh".to_string(), Val::Builtin(Builtin::LinalgEigh));
                m.insert("vec_add".to_string(), Val::Builtin(Builtin::LinalgVecAdd));
                m.insert("vec_sub".to_string(), Val::Builtin(Builtin::LinalgVecSub));
                m.insert("vec_scale".to_string(), Val::Builtin(Builtin::LinalgVecScale));
                m.insert("mat_add".to_string(), Val::Builtin(Builtin::LinalgMatAdd));
                m.insert("mat_scale".to_string(), Val::Builtin(Builtin::LinalgMatScale));
                Ok(m)
            }
            _ => bail!("unknown std module: {name}"),
        }
    }
    fn builtin_digest(&self, name: &str) -> String {
        if name == "std/record" {
            return self.builtin_digest("std/rec");
        }

        let mut h = Sha256::new();
        h.update(format!("builtin:{name}:v0.5").as_bytes());
        format!("sha256:{:x}", h.finalize())
    }

    fn stdlib_root_digest(&self) -> String {
        let names: [&str; 23] = [
            "std/artifact",
            "std/bytes",
            "std/codec",
            "std/env",
            "std/flow",
            "std/fs",
            "std/grow",
            "std/hash",
            "std/http",
            "std/int",
            "std/json",
            "std/list",
            "std/map",
            "std/option",
            "std/record",
            "std/result",
            "std/str",
            "std/time",
            "std/trace",
            "std/png",
            "std/rec",
            "std/crypto",
            "std/rand",
        ];

        let mut pairs: Vec<(String, String)> = names
            .into_iter()
            .map(|n| (n.to_string(), self.builtin_digest(n)))
            .collect();

        pairs.sort_by(|a, b| a.0.cmp(&b.0));

        let mut pre = String::new();
        pre.push_str("stdlib_root_v0\n");
        for (n, d) in pairs {
            pre.push_str(&n);
            pre.push_str("=");
            pre.push_str(&d);
            pre.push_str("\n");
        }

        format!("sha256:{}", sha256_bytes_hex(pre.as_bytes()))
    }
}
fn base_env() -> Env {
    let mut e = Env::new();
    e.set("emit".to_string(), Val::Builtin(Builtin::Emit));
    e.set("len".to_string(), Val::Builtin(Builtin::Len));
    e.set(
        "import_artifact".to_string(),
        Val::Builtin(Builtin::ImportArtifact),
    );

    e.set(
        "import_artifact_named".to_string(),
        Val::Builtin(Builtin::ImportArtifactNamed),
    );
    e.set(
        "emit_artifact".to_string(),
        Val::Builtin(Builtin::EmitArtifact),
    );

    e.set(
        "emit_artifact_derived".to_string(),
        Val::Builtin(Builtin::EmitArtifactDerived),
    );
    e
}
fn sha256_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}
fn file_digest(p: &Path) -> Result<String> {
    let b = fs::read(p)?;
    let mut h = Sha256::new();
    h.update(&b);
    Ok(format!("sha256:{:x}", h.finalize()))
}

fn canonical_json_bytes(v: &serde_json::Value) -> Vec<u8> {
    fn emit(out: &mut String, v: &serde_json::Value) {
        match v {
            serde_json::Value::Null => out.push_str("null"),
            serde_json::Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            serde_json::Value::Number(n) => out.push_str(&n.to_string()),
            serde_json::Value::String(s) => {
                out.push_str(&serde_json::to_string(s).expect("JSON_STRING_ENC_FAIL"))
            }
            serde_json::Value::Array(xs) => {
                out.push_str("[");
                for (i, x) in xs.iter().enumerate() {
                    if i != 0 {
                        out.push_str(",");
                    }
                    emit(out, x);
                }
                out.push_str("]");
            }
            serde_json::Value::Object(m) => {
                out.push_str("{");
                let mut keys: Vec<&String> = m.keys().collect();

                keys.sort_by(|a, b| {
                    if a.as_str() == "t" && b.as_str() != "t" {
                        return std::cmp::Ordering::Less;
                    }
                    if a.as_str() != "t" && b.as_str() == "t" {
                        return std::cmp::Ordering::Greater;
                    }
                    a.cmp(b)
                });
                for (i, k) in keys.iter().enumerate() {
                    if i != 0 {
                        out.push_str(",");
                    }
                    out.push_str(&serde_json::to_string(k).expect("JSON_KEY_ENC_FAIL"));
                    out.push_str(":");
                    emit(out, m.get(*k).expect("JSON_KEY_MISSING"));
                }
                out.push_str("}");
            }
        }
    }

    let mut s = String::new();
    emit(&mut s, v);
    s.into_bytes()
}
