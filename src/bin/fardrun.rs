use sha2::Digest as Sha2Digest;
use valuecore::Sha256 as NativeSha256;
use anyhow::{anyhow, bail, Context, Result};
// fardlang dialect support (module header detection)
use fardlang::parse::parse_module as fardlang_parse_module;
use fardlang::check::check_module as fardlang_check_module;
use fardlang::eval::{eval_block, apply_imports, Env as FardlangEnv};
#[derive(Debug, Clone)]
enum TypeField {
    Named(String, String), // field_name, type_name
}

#[derive(Debug, Clone)]
enum TypeDefKind {
    Record(Vec<TypeField>),
    Sum(Vec<(String, Vec<TypeField>)>), // variant_name, fields
}

#[derive(Debug, Clone)]
enum StrPart {
    Lit(String),
    Expr(Expr),
}

const QMARK_EXPECT_RESULT: &str = "QMARK_EXPECT_RESULT";
const QMARK_PROPAGATE_ERR: &str = "QMARK_PROPAGATE_ERR";
const RESULT_OK_TAG: &str = "ok";
const RESULT_ERR_TAG: &str = "err";
const RESULT_TAG_KEY: &str = "t";
const RESULT_OK_VAL_KEY: &str = "v";
const RESULT_ERR_VAL_KEY: &str = "e";
const ERROR_PAT_MISMATCH: &str = "ERROR_PAT_MISMATCH";
const ERROR_MATCH_NO_ARM: &str = "ERROR_MATCH_NO_ARM";
use valuecore::int::{i64_sub, i64_mul, i64_div, i64_rem};
use valuecore::json::{JsonVal as J, escape_string, from_slice as json_from_slice, from_str as json_from_str, to_string as json_to_string};
type Map = std::collections::BTreeMap<String, J>;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
thread_local! {
    static VM_FNS: std::cell::RefCell<Vec<VmCompiledFn>> = std::cell::RefCell::new(Vec::new());
    static VM_SELF_SLOTS: std::cell::RefCell<std::collections::HashMap<usize, (usize, Val)>> = std::cell::RefCell::new(std::collections::HashMap::<usize, (usize, Val)>::new());
    /// Raw pointer to VM_FNS vec — valid for entire program execution, avoids per-call clone
    static VM_FNS_PTR: std::cell::Cell<*const Vec<VmCompiledFn>> = std::cell::Cell::new(std::ptr::null());
    /// Region-based slot arena — reusable Vec<Val> frames, avoids per-call heap allocation
    static SLOT_ARENA: std::cell::RefCell<Vec<Vec<Val>>> = std::cell::RefCell::new(Vec::new());

    static PROGRAM_ARGS: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(vec![]);
    static CALL_DEPTH: std::cell::RefCell<usize> = std::cell::RefCell::new(0);
}
fn set_program_args(args: Vec<String>) {
    PROGRAM_ARGS.with(|a| *a.borrow_mut() = args);
}
fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let mut h = NativeSha256::new();
    h.update(bytes);
    hex_lower(&h.finalize())
}
fn sha256_raw(bytes: &[u8]) -> Vec<u8> {
    let mut h = NativeSha256::new();
    h.update(bytes);
    h.finalize().to_vec()
}
fn merkle_root_bytes(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        let r = sha256_raw(b"");
        return r.as_slice().try_into().unwrap_or([0u8; 32]);
    }
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            let mut buf = [0u8; 64];
            buf[0..32].copy_from_slice(&left);
            buf[32..64].copy_from_slice(&right);
            let h = sha256_raw(&buf);
            next.push(h.as_slice().try_into().unwrap_or([0u8; 32]));
            i += 2;
        }
        level = next;
    }
    level[0]
}
fn canon_json(v: &J) -> Result<String> {
    fn canon_value(v: &J, out: &mut String) -> Result<()> {
        match v {
            J::Null => {
                out.push_str("null");
                Ok(())
            }
            J::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
                Ok(())
            }
            J::Int(n) => {
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
            J::Float(f) => {
                let s = format!("{}", f);
                if s.contains('+') { bail!("M5_CANON_NUM_PLUS"); }
                if s.ends_with(".0") { bail!("M5_CANON_NUM_DOT0"); }
                out.push_str(&s);
                Ok(())
            }
            J::Str(s) => {
                out.push_str(&escape_string(s));
                Ok(())
            }
            J::Array(a) => {
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
            J::Object(m) => {
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
                    out.push_str(&escape_string(k));
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
                    out.push_str(&escape_string(k));
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

const REGISTRY_URL: &str = "https://github.com/mauludsadiq/FARD/releases/latest/download/registry.json";

fn fard_cache_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        PathBuf::from(h).join(".fard").join("cache")
    } else {
        PathBuf::from("/tmp/.fard_cache")
    }
}


// ── Semver range resolution ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SemVer { major: u32, minor: u32, patch: u32 }

impl SemVer {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim_start_matches(|c: char| !c.is_ascii_digit());
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        if parts.len() < 2 { return None; }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts.get(2)
            .and_then(|p| p.split('-').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        Some(SemVer { major, minor, patch })
    }

    fn as_tuple(&self) -> (u32, u32, u32) { (self.major, self.minor, self.patch) }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Check if `version` satisfies `range`.
/// Supported: exact "1.2.3", caret "^1.2.3" (>=1.2.3 <2.0.0),
///            tilde "~1.2.3" (>=1.2.3 <1.3.0), ">=1.2.3", "*"/"latest"
fn semver_matches(range: &str, version: &str) -> bool {
    let range = range.trim();
    if range == "*" || range == "latest" || range.is_empty() { return true; }
    let ver = match SemVer::parse(version) { Some(v) => v, None => return false };
    if let Some(req) = range.strip_prefix('^') {
        // ^1.2.3: >=1.2.3 <2.0.0 (or <0.2.0 if major=0, etc.)
        let req = match SemVer::parse(req) { Some(v) => v, None => return false };
        if req.major > 0 {
            ver.as_tuple() >= req.as_tuple() && ver.major == req.major
        } else if req.minor > 0 {
            ver.as_tuple() >= req.as_tuple() && ver.major == 0 && ver.minor == req.minor
        } else {
            ver.as_tuple() >= req.as_tuple() && ver.major == 0 && ver.minor == 0
        }
    } else if let Some(req) = range.strip_prefix('~') {
        // ~1.2.3: >=1.2.3 <1.3.0
        let req = match SemVer::parse(req) { Some(v) => v, None => return false };
        ver.as_tuple() >= req.as_tuple() && ver.major == req.major && ver.minor == req.minor
    } else if let Some(req) = range.strip_prefix(">=") {
        let req = match SemVer::parse(req) { Some(v) => v, None => return false };
        ver.as_tuple() >= req.as_tuple()
    } else if let Some(req) = range.strip_prefix('>') {
        let req = match SemVer::parse(req) { Some(v) => v, None => return false };
        ver.as_tuple() > req.as_tuple()
    } else if let Some(req) = range.strip_prefix("<=") {
        let req = match SemVer::parse(req) { Some(v) => v, None => return false };
        ver.as_tuple() <= req.as_tuple()
    } else if let Some(req) = range.strip_prefix('<') {
        let req = match SemVer::parse(req) { Some(v) => v, None => return false };
        ver.as_tuple() < req.as_tuple()
    } else {
        // Exact match or prefix match (1.6 matches 1.6.0)
        if let Some(req) = SemVer::parse(range) {
            if range.matches('.').count() == 1 {
                // Minor-level: 1.6 matches 1.6.x
                ver.major == req.major && ver.minor == req.minor
            } else {
                ver.as_tuple() == req.as_tuple()
            }
        } else {
            version == range // exact string match
        }
    }
}

/// Resolve a semver range to the best matching version from a list.
/// Returns the highest version satisfying the range.
fn resolve_version<'a>(range: &str, versions: &[&'a str]) -> Option<&'a str> {
    let mut best: Option<(SemVer, &str)> = None;
    for &ver in versions {
        if semver_matches(range, ver) {
            if let Some(sv) = SemVer::parse(ver) {
                match &best {
                    None => best = Some((sv, ver)),
                    Some((best_sv, _)) if sv.as_tuple() > best_sv.as_tuple() => {
                        best = Some((sv, ver));
                    }
                    _ => {}
                }
            }
        }
    }
    best.map(|(_, v)| v)
}

/// Search packages in registry by query string.
fn search_packages(query: &str) -> Result<Vec<(String, String, String)>> {
    let registry_body = ureq::get(REGISTRY_URL)
        .call()
        .map_err(|e| anyhow!("ERROR_REGISTRY failed to fetch registry: {e}"))?
        .into_string()?;
    let registry: J = json_from_slice(registry_body.as_bytes())?;
    let packages = registry.get("packages")
        .and_then(|p| p.as_object())
        .ok_or_else(|| anyhow!("ERROR_REGISTRY missing packages"))?;
    let query_lower = query.to_lowercase();
    let mut results: Vec<(String, String, String)> = Vec::new();
    for (key, entry) in packages {
        let key_lower = key.to_lowercase();
        if query_lower.is_empty() || key_lower.contains(&query_lower) {
            let desc = entry.get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            // key is "name@version"
            if let Some((name, ver)) = key.split_once('@') {
                results.push((name.to_string(), ver.to_string(), desc));
            }
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

fn fetch_package(pkg_name: &str, version: &str) -> Result<PathBuf> {
    let cache_dir = fard_cache_dir();
    let pkg_dir = cache_dir.join(format!("{}@{}", pkg_name, version));
    let marker = pkg_dir.join(".fetched");
    if marker.exists() {
        let inner = pkg_dir.join(pkg_name);
        return Ok(if inner.exists() { inner } else { pkg_dir });
    }
    // Fetch registry.json
    eprintln!("[fard] fetching registry...");
    let registry_body = ureq::get(REGISTRY_URL)
        .call()
        .map_err(|e| anyhow!("ERROR_REGISTRY failed to fetch registry: {e}"))?
        .into_string()?;
    let registry: J = json_from_slice(registry_body.as_bytes())?;
    // Resolve semver range to exact version
    let resolved_version = if let Some(pkgs) = registry.get("packages").and_then(|p| p.as_object()) {
        let versions: Vec<&str> = pkgs.keys()
            .filter_map(|k| k.strip_prefix(&format!("{}@", pkg_name)))
            .collect();
        if versions.is_empty() {
            bail!("ERROR_REGISTRY package not found: {}", pkg_name);
        }
        resolve_version(version, &versions)
            .map(|v| v.to_string())
            .unwrap_or_else(|| version.to_string())
    } else {
        version.to_string()
    };
    if resolved_version != version {
        eprintln!("[fard] resolved {}@{} -> {}@{}", pkg_name, version, pkg_name, resolved_version);
    }
    let key = format!("{}@{}", pkg_name, resolved_version);
    let pkg_entry = registry
        .get("packages")
        .and_then(|p| p.get(&key))
        .ok_or_else(|| anyhow!("ERROR_REGISTRY package not found: {key}"))?;
    let url = pkg_entry
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow!("ERROR_REGISTRY missing url for {key}"))?;
    let expected_sha256 = pkg_entry
        .get("sha256")
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow!("ERROR_REGISTRY missing sha256 for {key}"))?;
    // Download tar.gz
    eprintln!("[fard] downloading {}@{}...", pkg_name, version);
    let mut reader = ureq::get(url)
        .call()
        .map_err(|e| anyhow!("ERROR_REGISTRY download failed: {e}"))?
        .into_reader();
    let mut tar_bytes = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut tar_bytes)?;
    // Verify sha256
    let got_sha256 = sha256_bytes_hex(&tar_bytes);
    if got_sha256 != expected_sha256 {
        bail!("ERROR_REGISTRY sha256 mismatch for {key}: expected {expected_sha256}, got {got_sha256}");
    }
    // Extract tar.gz
    fs::create_dir_all(&pkg_dir)?;
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(&tar_bytes));
    let mut archive = tar::Archive::new(gz);
    archive.unpack(&pkg_dir)?;
    // Write marker
    fs::write(&marker, b"")?;
    eprintln!("[fard] installed {}@{}", pkg_name, version);
    // Return the inner directory (tar extracts to pkg_name/ subdir)
    let inner = pkg_dir.join(pkg_name);
    if inner.exists() { Ok(inner) } else { Ok(pkg_dir) }
}

fn write_m5_digests(
    out_dir: &std::path::Path,
    runtime_version: &str,
    trace_format_version: &str,
    stdlib_root_digest: &str,
    ok: bool,
    self_digest_subst: Option<&str>,
    no_trace: bool,
) -> Result<()> {
    let trace_path = out_dir.join("trace.ndjson");
    let modg_path = out_dir.join("module_graph.json");

    let trace_h = if no_trace {
        "sha256:no-trace".to_string()
    } else {
        format!("sha256:{}", sha256_file_hex(&trace_path)?)
    };
    let modg_h = format!("sha256:{}", sha256_file_hex(&modg_path)?);

    let (leaf_name, leaf_path) = if ok {
        ("result.json", out_dir.join("result.json"))
    } else {
        ("error.json", out_dir.join("error.json"))
    };
    // Normalize self_digest placeholder in leaf file before hashing
    // so the digest is stable across fixed-point iterations
    let leaf_h = if let Some(subst) = self_digest_subst {
        let raw = std::fs::read(&leaf_path).unwrap_or_default();
        let normalized = String::from_utf8_lossy(&raw)
            .replace(subst, "sha256:self")
            .into_bytes();
        format!("sha256:{}", sha256_bytes_hex(&normalized))
    } else {
        format!("sha256:{}", sha256_file_hex(&leaf_path)?)
    };

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    files.insert("trace.ndjson".to_string(), trace_h.clone());
    files.insert("module_graph.json".to_string(), modg_h.clone());
    files.insert(leaf_name.to_string(), leaf_h.clone());
    let preimage = {
        let mut m = Map::new();
        m.insert("files".to_string(), J::Object(files.iter().map(|(k,v)| (k.clone(), J::Str(v.clone()))).collect()));
        m.insert("ok".to_string(), J::Bool(ok));
        m.insert("runtime_version".to_string(), J::Str(runtime_version.to_string()));
        m.insert("stdlib_root_digest".to_string(), J::Str(stdlib_root_digest.to_string()));
        m.insert("trace_format_version".to_string(), J::Str(trace_format_version.to_string()));
        J::Object(m)
    };
    let canon = canon_json(&preimage)?;
    let preimage_sha256 = format!("sha256:{}", sha256_bytes_hex(canon.as_bytes()));
    println!("fard_run_digest={}", preimage_sha256);
    let dig = {
        let mut m = Map::new();
        m.insert("files".to_string(), J::Object(files.into_iter().map(|(k,v)| (k, J::Str(v))).collect()));
        m.insert("ok".to_string(), J::Bool(ok));
        m.insert("preimage_sha256".to_string(), J::Str(preimage_sha256.to_string()));
        m.insert("runtime_version".to_string(), J::Str(runtime_version.to_string()));
        m.insert("stdlib_root_digest".to_string(), J::Str(stdlib_root_digest.to_string()));
        m.insert("trace_format_version".to_string(), J::Str(trace_format_version.to_string()));
        J::Object(m)
    };

    let out = canonical_json_bytes(&dig);
    std::fs::write(out_dir.join("digests.json"), out).with_context(|| "write digests.json")?;
    Ok(())
}

thread_local! {
    static RETURN_VAL: std::cell::RefCell<Option<Val>> = std::cell::RefCell::new(None);
    static WITNESS_DEPS: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    static SELF_DIGEST: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
    static SELF_DIGEST_ACCESSED: std::cell::RefCell<bool> = std::cell::RefCell::new(false);
    static FFI_LIBS: std::cell::RefCell<std::collections::HashMap<String, libloading::Library>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i-1] == b[j-1] {
                dp[i-1][j-1]
            } else {
                1 + dp[i-1][j].min(dp[i][j-1]).min(dp[i-1][j-1])
            };
        }
    }
    dp[m][n]
}

fn cmd_new(args: fard_v0_5_language_gate::cli::fardrun_cli::NewArgs) -> Result<()> {
    let name = &args.name;
    let template = args.template.as_str();
    if name.is_empty() || name.contains("/") || name.contains("\\") {
        bail!("invalid project name: {}", name);
    }
    let dir = std::path::Path::new(name);
    if dir.exists() { bail!("directory already exists: {}", name); }
    fs::create_dir_all(dir)?;
    fs::create_dir_all(dir.join(".vscode"))?;

    fs::write(dir.join("fard.toml"), format!("name = \"{}\"
version = \"2026-03-15\"
entry = \"main.fard\"\n", name))?;

    let main_src = match template {
        "server" => format!("// {} -- HTTP server\nimport(\"std/net\") as net\nimport(\"std/json\") as json\n\nfn handle(req) {{\n  if req.path == \"/health\" then\n    {{ status: 200, body: \"ok\", headers: {{}} }}\n  else\n    {{ status: 404, body: \"not found\", headers: {{}} }}\n}}\n\nlet _ = net.serve(8080, fn(req) {{ handle(req) }})\nnull\n", name),
        "ci" => format!("// {} -- CI runner\nimport(\"std/io\") as io\nimport(\"std/json\") as json\nimport(\"std/list\") as list\nimport(\"std/process\") as process\n\nfn run_step(step) {{\n  let r = process.spawn(\"fardrun\", [\"test\", \"--program\", step.program], \"\")\n  {{ name: step.name, passed: r.code == 0 }}\n}}\n\nlet spec = json.decode(io.read_file(\"pipeline.json\").ok)\nlet results = list.map(spec.steps, fn(s) {{ run_step(s) }})\n{{ total: list.len(results) }}\n", name),
        _ => format!("// {} -- FARD program\n// Every run produces a SHA-256 receipt in out/\n\nimport(\"std/str\") as str\n\nfn greet(name) {{\n  str.concat(\"Hello, \", str.concat(name, \"!\"))\n}}\n\ngreet(\"world\")\n", name),
    };
    fs::write(dir.join("main.fard"), main_src)?;

    let lsp = std::env::var("HOME").unwrap_or_else(|_| ".".into()) + "/bin/fard-lsp";
    let vsc = format!("{{\n  \"fard.lspPath\": \"{}\",\n  \"[fard]\": {{\n    \"editor.formatOnSave\": false\n  }}\n}}\n", lsp);
    fs::write(dir.join(".vscode").join("settings.json"), vsc)?;
    fs::write(dir.join(".gitignore"), "out/\nreceipts/\n*.lock.json\n")?;
    fs::write(dir.join("README.md"), format!("# {}\n\nA FARD project.\n\n## Run\n\n    fardrun run --program main.fard --out ./out\n    cat out/result.json\n", name))?;

    println!("Created project: {}/", name);
    println!("  {}/main.fard", name);
    println!("  {}/fard.toml", name);
    println!("  {}/.vscode/settings.json", name);
    println!("");
    println!("To run:");
    println!("  cd {}", name);
    println!("  fardrun run --program main.fard --out ./out");
    Ok(())
}



fn expr_contains_while(expr: &Expr) -> bool {
    match expr {
        Expr::While(..) => true,
        Expr::Let(_, e1, e2) => expr_contains_while(e1) || expr_contains_while(e2),
        Expr::LetPat(_, e1, e2) => expr_contains_while(e1) || expr_contains_while(e2),
        Expr::If(c, t, f) => expr_contains_while(c) || expr_contains_while(t) || expr_contains_while(f),
        Expr::Bin(_, a, b) => expr_contains_while(a) || expr_contains_while(b),
        Expr::Unary(_, e) => expr_contains_while(e),
        Expr::Call(f, args) => expr_contains_while(f) || args.iter().any(|a| expr_contains_while(a)),
        Expr::Fn(_, body) | Expr::Lambda(_, body) => expr_contains_while(body),
        Expr::Get(e, _) => expr_contains_while(e),
        Expr::List(xs) => xs.iter().any(|x| expr_contains_while(x)),
        Expr::Rec(kvs) => kvs.iter().any(|(_, v)| expr_contains_while(v)),
        Expr::Try(e) | Expr::Return(e) => expr_contains_while(e),
        Expr::Match(e, arms) => expr_contains_while(e) || arms.iter().any(|a| expr_contains_while(&a.body)),
        Expr::Using(_, e1, e2) => expr_contains_while(e1) || expr_contains_while(e2),
        Expr::Index(a, b) => expr_contains_while(a) || expr_contains_while(b),
        Expr::NamedCall(f, args) => expr_contains_while(f) || args.iter().any(|(_, v)| expr_contains_while(v)),
        _ => false,
    }
}

fn expr_contains_var(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Var(n) => n == name,
        Expr::Int(_) | Expr::FloatLit(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Null => false,
        Expr::Let(_, e1, e2) => expr_contains_var(e1, name) || expr_contains_var(e2, name),
        Expr::LetPat(_, e1, e2) => expr_contains_var(e1, name) || expr_contains_var(e2, name),
        Expr::If(c, t, f) => expr_contains_var(c, name) || expr_contains_var(t, name) || expr_contains_var(f, name),
        Expr::Bin(_, a, b) => expr_contains_var(a, name) || expr_contains_var(b, name),
        Expr::Unary(_, e) => expr_contains_var(e, name),
        Expr::Call(f, args) => expr_contains_var(f, name) || args.iter().any(|a| expr_contains_var(a, name)),
        Expr::Fn(_, body) | Expr::Lambda(_, body) => expr_contains_var(body, name),
        Expr::Get(e, _) => expr_contains_var(e, name),
        Expr::List(xs) => xs.iter().any(|x| expr_contains_var(x, name)),
        Expr::Rec(kvs) => kvs.iter().any(|(_, v)| expr_contains_var(v, name)),
        Expr::Try(e) | Expr::Return(e) => expr_contains_var(e, name),
        Expr::Match(e, arms) => expr_contains_var(e, name) || arms.iter().any(|a| expr_contains_var(&a.body, name)),
        Expr::While(a, b, c) => expr_contains_var(a, name) || expr_contains_var(b, name) || expr_contains_var(c, name),
        Expr::Using(_, e1, e2) => expr_contains_var(e1, name) || expr_contains_var(e2, name),
        Expr::Index(a, b) => expr_contains_var(a, name) || expr_contains_var(b, name),
        Expr::NamedCall(f, args) => expr_contains_var(f, name) || args.iter().any(|(_, v)| expr_contains_var(v, name)),
        Expr::StrInterp(_) => false,
    }
}




fn main() -> Result<()> {
    let (run, want_version, want_repl, test_args, publish_args, install_args, new_args) = fard_v0_5_language_gate::cli::fardrun_cli::Cli::parse_compat();

    // Handle search subcommand
    if std::env::var("FARD_SEARCH_MODE").is_ok() {
        let query = std::env::var("FARD_SEARCH_QUERY").unwrap_or_default();
        match search_packages(&query) {
            Ok(results) => {
                if results.is_empty() {
                    println!("No packages found{}", if query.is_empty() { String::new() } else { format!(" matching {:?}", query) });
                } else {
                    println!("{} package(s) found:\n", results.len());
                    for (name, ver, desc) in &results {
                        if desc.is_empty() {
                            println!("  {}@{}", name, ver);
                        } else {
                            println!("  {}@{}  —  {}", name, ver, desc);
                        }
                    }
                }
            }
            Err(e) => eprintln!("search error: {}", e),
        }
        return Ok(());
    }
    if want_version {
        println!("fard_runtime_version={}", env!("CARGO_PKG_VERSION"));
        println!("trace_format_version=0.1.0");
        println!("stdlib_root_cid=sha256:dev");
        return Ok(());
    }
    // fardrun new <name> [--template minimal|server|ci]
    if let Some(new_args) = new_args {
        use fard_v0_5_language_gate::cli::fardrun_cli::NewArgs;
        return cmd_new(new_args);
    }

fn pretty_print_val(v: &Val, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let pad1 = "  ".repeat(indent + 1);
    match v {
        Val::Unit => "null".to_string(),
        Val::Bool(b) => b.to_string(),
        Val::Int(n) => n.to_string(),
        Val::Float(f) => format!("{}", f),
        Val::Text(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        Val::Bytes(b) => format!("<bytes:{}>", b.len()),
        Val::Big(n) => n.to_string(),
        Val::List(items) => {
            if items.is_empty() { return "[]".to_string(); }
            // Short lists on one line
            if items.len() <= 4 {
                let all_simple = items.iter().all(|v| matches!(v,
                    Val::Int(_) | Val::Float(_) | Val::Bool(_) | Val::Text(_) | Val::Unit));
                if all_simple {
                    let inner: Vec<String> = items.iter().map(|v| pretty_print_val(v, 0)).collect();
                    let line = format!("[{}]", inner.join(", "));
                    if line.len() < 60 { return line; }
                }
            }
            let mut out = "[\n".to_string();
            for (i, item) in items.iter().enumerate() {
                out.push_str(&format!("{}{}", pad1, pretty_print_val(item, indent + 1)));
                if i + 1 < items.len() { out.push(','); }
                out.push('\n');
            }
            out.push_str(&format!("{}]", pad));
            out
        }
        Val::Record(m) => {
            if m.is_empty() { return "{}".to_string(); }
            // Short records on one line
            if m.len() <= 3 {
                let all_simple = m.values().all(|v| matches!(v,
                    Val::Int(_) | Val::Float(_) | Val::Bool(_) | Val::Text(_) | Val::Unit));
                if all_simple {
                    let inner: Vec<String> = m.iter()
                        .map(|(k, v)| format!("{}: {}", k, pretty_print_val(v, 0)))
                        .collect();
                    let line = format!("{{ {} }}", inner.join(", "));
                    if line.len() < 60 { return line; }
                }
            }
            let mut out = "{\n".to_string();
            let keys: Vec<&String> = m.keys().collect();
            for (i, k) in keys.iter().enumerate() {
                let val = &m[*k];
                out.push_str(&format!("{}{}: {}", pad1, k, pretty_print_val(val, indent + 1)));
                if i + 1 < keys.len() { out.push(','); }
                out.push('\n');
            }
            out.push_str(&format!("{}}}", pad));
            out
        }
        Val::Func(_) | Val::VmFunc(_) => "<fn>".to_string(),
        Val::Builtin(_) => "<builtin>".to_string(),
        Val::Chan(..) => "<chan>".to_string(),
        Val::Mtx(..) => "<mutex>".to_string(),
        Val::Promise(..) => "<promise>".to_string(),
        Val::BoundMethod(..) => "<method>".to_string(),
        Val::Err { code, .. } => format!("<err:{}>", code),
    }
}

    if want_repl {
        use rustyline::error::ReadlineError;
        use rustyline::DefaultEditor;

        println!("FARD v{} REPL", env!("CARGO_PKG_VERSION"));
        println!("  :quit / :q      exit");
        println!("  :help           show commands");
        println!("  :reset          clear environment");
        println!("  :vars           show defined names");
        println!("  :time <expr>    time an expression");
        println!("  {{...}}          multi-line auto-continuation");
        println!("");

        let mut rl = DefaultEditor::new().expect("readline init failed");
        let history_path = std::env::var("HOME")
            .map(|h| format!("{}/.fard_history", h))
            .unwrap_or_else(|_| "/tmp/.fard_history".to_string());
        let _ = rl.load_history(&history_path);

        let mut env = Env::new();
        let mut loader = ModuleLoader::new(Path::new("."));
        let devnull = std::path::PathBuf::from("/dev/null");
        let mut tracer = Tracer::new(&devnull, &devnull).unwrap_or_else(|_| {
            let t = std::env::temp_dir();
            Tracer::new(&t, &t.join("repl_trace.ndjson")).expect("tracer")
        });

        let mut pending = String::new();

        loop {
            let prompt = if pending.is_empty() { "fard> " } else { "....  " };
            let readline = rl.readline(prompt);
            let line = match readline {
                Ok(l) => {
                    if !l.trim().is_empty() && pending.is_empty() {
                        let _ = rl.add_history_entry(l.as_str());
                    }
                    l
                }
                Err(ReadlineError::Interrupted) => {
                    if !pending.is_empty() {
                        pending.clear();
                        println!("^C (input cleared)");
                    } else {
                        println!("^C");
                    }
                    continue;
                }
                Err(ReadlineError::Eof) => { println!(""); break; }
                Err(_) => break,
            };
            let line = line.trim_end_matches(|c: char| c == '\n' || c == '\r');

            // REPL commands (only when not in continuation)
            if pending.is_empty() {
                if line == ":quit" || line == ":q" { break; }
                if line.trim().is_empty() { continue; }
                if line == ":help" {
                    println!("Commands:");
                    println!("  :quit / :q          exit the REPL");
                    println!("  :reset              clear all bindings");
                    println!("  :vars               list defined names");
                    println!("  :time <expr>        time expression evaluation");
                    println!("");
                    println!("Language:");
                    println!("  let x = expr        bind a name");
                    println!("  fn f(x) {{ expr }}    define a function");
                    println!("  import(\"std/list\") as list");
                    println!("  Open {{ and press Enter for multi-line input");
                    continue;
                }
                if line == ":reset" {
                    env = Env::new();
                    loader = ModuleLoader::new(Path::new("."));
                    println!("environment reset");
                    continue;
                }
                if line == ":vars" {
                    let mut keys = env.keys();
                    if keys.is_empty() {
                        println!("(no bindings)");
                    } else {
                        keys.sort();
                        for k in keys { println!("  {k}"); }
                    }
                    continue;
                }
            }

            // Accumulate multi-line input
            if !pending.is_empty() { pending.push('\n'); }
            pending.push_str(line);

            // Check brace/paren balance
            let open_braces  = pending.chars().filter(|&c| c == '{').count();
            let close_braces = pending.chars().filter(|&c| c == '}').count();
            let open_parens  = pending.chars().filter(|&c| c == '(').count();
            let close_parens = pending.chars().filter(|&c| c == ')').count();
            if open_braces > close_braces || open_parens > close_parens {
                continue; // wait for more input
            }

            let input = pending.clone();
            pending.clear();

            // :time prefix
            let (timed, eval_input) = if input.trim_start().starts_with(":time ") {
                (true, input.trim_start()[6..].to_string())
            } else {
                (false, input.clone())
            };

            let file = "<repl>".to_string();
            let t_start = std::time::Instant::now();
            match Parser::from_src(&eval_input, &file) {
                Err(e) => { eprintln!("parse error: {e}"); continue; }
                Ok(mut p) => {
                    match p.parse_module() {
                        Err(e) => { eprintln!("parse error: {e}"); continue; }
                        Ok(items) => {
                            match loader.eval_items(items, &mut env, &mut tracer, Path::new(".")) {
                                Err(e) => { eprintln!("error: {}", e.root_cause()); }
                                Ok(v) => {
                                    let elapsed = t_start.elapsed();
                                    if timed {
                                        println!("time: {:.3}ms", elapsed.as_secs_f64() * 1000.0);
                                    }
                                    if !matches!(v, Val::Unit) {
                                        println!("{}", pretty_print_val(&v, 0));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let _ = rl.save_history(&history_path);
        return Ok(());
    }
    // Test runner
    if let Some(targs) = test_args {
        let program = targs.program;
        let src = fs::read_to_string(&program)
            .with_context(|| format!("cannot read {}", program.display()))?;
        let file = program.to_string_lossy().to_string();
        let mut parser = Parser::from_src(&src, &file)?;
        let items = parser.parse_module()?;
        let mut loader = ModuleLoader::new(program.parent().unwrap_or(Path::new(".")));
        let t = std::env::temp_dir();
        let tp = t.join("fard_test_trace.ndjson");
        let mut tracer = Tracer::new(&t, &tp).expect("tracer");
        let mut env = base_env();
        // Set SELF_DIGEST to a stable value for test mode (hash of program source)
        {
            let test_digest = format!("sha256:{}", sha256_bytes_hex(src.as_bytes()));
            SELF_DIGEST.with(|d| *d.borrow_mut() = test_digest);
            SELF_DIGEST_ACCESSED.with(|a| *a.borrow_mut() = false);
        }
        // First pass: register all non-test items
        let non_test: Vec<Item> = items.iter().filter(|i| !matches!(i, Item::Test(..)))
            .cloned().collect();
        loader.eval_items(non_test, &mut env, &mut tracer, program.parent().unwrap_or(Path::new(".")))?;
        // Set VM_FNS_PTR after all fns compiled — avoids per-call clone
        ModuleLoader::set_vm_fns_ptr();
        // Second pass: run tests
        let tests: Vec<(String, Expr, ErrorSpan)> = items.into_iter().filter_map(|i| {
            if let Item::Test(label, body, span) = i { Some((label, body, span)) } else { None }
        }).collect();
        let total = tests.len();
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut results: Vec<(String, bool, Option<String>)> = Vec::new();
        for (label, body, span) in tests {
            let mut test_env = env.clone();
            match eval(&body, &mut test_env, &mut tracer, &mut loader) {
                Ok(Val::Bool(true)) => {
                    passed += 1;
                    println!("  [32m✓[0m {}", label);
                    results.push((label, true, None));
                }
                Ok(Val::Bool(false)) => {
                    failed += 1;
                    println!("  [31m✗[0m {} [2m→ false[0m", label);
                    results.push((label, false, Some("assertion returned false".to_string())));
                }
                Ok(other) => {
                    failed += 1;
                    let msg = format!("expected bool, got {:?}", other);
                    println!("  [31m✗[0m {} [2m→ {}[0m", label, msg);
                    results.push((label, false, Some(msg)));
                }
                Err(e) => {
                    failed += 1;
                    let msg = e.root_cause().to_string();
                    println!("  [31m✗[0m {} [2m→ error: {}[0m  --> {}:{}:{}",
                        label, msg, span.file, span.line, span.col);
                    results.push((label, false, Some(msg)));
                }
            }
        }
        println!();
        if failed == 0 {
            println!("[32m  {} passed[0m", passed);
        } else {
            println!("[32m  {} passed[0m  [31m{} failed[0m", passed, failed);
        }
        if targs.json {
            let mut out = Map::new();
            out.insert("passed".to_string(), J::Int(passed as i64));
            out.insert("failed".to_string(), J::Int(failed as i64));
            out.insert("total".to_string(), J::Int(total as i64));
            let arr: Vec<J> = results.into_iter().map(|(label, ok, msg)| {
                let mut m = Map::new();
                m.insert("label".to_string(), J::Str(label));
                m.insert("ok".to_string(), J::Bool(ok));
                if let Some(e) = msg { m.insert("error".to_string(), J::Str(e)); }
                J::Object(m)
            }).collect();
            out.insert("tests".to_string(), J::Array(arr));
            println!("{}", json_to_string(&J::Object(out)));
        }
        std::process::exit(if failed > 0 { 1 } else { 0 });
    }
    // Publish
    if let Some(pargs) = publish_args {
        let pkg_dir = &pargs.package;
        let toml_path = pkg_dir.join("fard.toml");
        let toml_src = fs::read_to_string(&toml_path)
            .with_context(|| format!("cannot read fard.toml in {}", pkg_dir.display()))?;
        // Parse name and version from fard.toml
        let get_field = |key: &str| -> Option<String> {
            toml_src.lines()
                .find(|l| l.trim_start().starts_with(key))
                .and_then(|l| l.split_once('='))
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
        };
        let pkg_name = get_field("name")
            .ok_or_else(|| anyhow!("fard.toml missing 'name'"))?;
        let pkg_version = get_field("version")
            .ok_or_else(|| anyhow!("fard.toml missing 'version'"))?;
        let tag = format!("pkg-{}-{}", pkg_name, pkg_version);
        let tarball_name = format!("{}@{}.tar.gz", pkg_name, pkg_version);
        eprintln!("[fard publish] packaging {}@{}...", pkg_name, pkg_version);
        // Build tar.gz in temp dir
        let tmp = std::env::temp_dir();
        let tarball_path = tmp.join(&tarball_name);
        let tar_gz = fs::File::create(&tarball_path)?;
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut ar = tar::Builder::new(enc);
        ar.append_dir_all(&pkg_name, pkg_dir)?;
        ar.finish()?;
        drop(ar);
        // Compute sha256
        let tar_bytes = fs::read(&tarball_path)?;
        let sha = sha256_bytes_hex(&tar_bytes);
        eprintln!("[fard publish] sha256: {}", sha);
        // GitHub API: create release + upload asset
        let repo = &pargs.repo;
        let token = &pargs.token;
        let auth = format!("token {}", token);
        // Get or create release
        eprintln!("[fard publish] creating release {}...", tag);
        let check_url = format!("https://api.github.com/repos/{}/releases/tags/{}", repo, tag);
        let rel_json: J = match ureq::get(&check_url)
            .set("Authorization", &auth)
            .set("User-Agent", "fardrun")
            .call()
        {
            Ok(resp) => {
                eprintln!("[fard publish] release already exists, updating...");
                json_from_slice(resp.into_string()?.as_bytes())?
            }
            Err(_) => {
                let rel_url = format!("https://api.github.com/repos/{}/releases", repo);
                let rel_body = format!(r#"{{"tag_name":"{}","name":"Package: {}@{}","body":"FARD package","draft":false,"prerelease":false}}"#,
                    tag, pkg_name, pkg_version);
                let rel_resp = ureq::post(&rel_url)
                    .set("Authorization", &auth)
                    .set("Content-Type", "application/json")
                    .set("User-Agent", "fardrun")
                    .send_string(&rel_body)?;
                json_from_slice(rel_resp.into_string()?.as_bytes())?
            }
        };
        let upload_url_tmpl = rel_json.get("upload_url")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow!("GitHub API: missing upload_url — check token permissions"))?;
        let upload_url = upload_url_tmpl.split('{').next().unwrap_or(upload_url_tmpl);
        let upload_url = format!("{}?name={}", upload_url, tarball_name);
        // Delete existing tarball asset if present
        let rel_assets = rel_json.get("assets").and_then(|a| a.as_array()).cloned().unwrap_or_default();
        if let Some(old_asset) = rel_assets.iter().find(|a| {
            a.get("name").and_then(|n| n.as_str()) == Some(tarball_name.as_str())
        }) {
            let aid = old_asset.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
            ureq::delete(&format!("https://api.github.com/repos/{}/releases/assets/{}", repo, aid))
                .set("Authorization", &auth).set("User-Agent", "fardrun").call().ok();
        }
        // Upload tarball
        eprintln!("[fard publish] uploading {}...", tarball_name);
        ureq::post(&upload_url)
            .set("Authorization", &auth)
            .set("Content-Type", "application/gzip")
            .set("User-Agent", "fardrun")
            .send_bytes(&tar_bytes)?;
        // Update registry.json
        eprintln!("[fard publish] updating registry.json...");
        let registry_url = format!(
            "https://api.github.com/repos/{}/releases/tags/registry",
            repo
        );
        let reg_rel: J = json_from_slice(
            ureq::get(&registry_url)
                .set("Authorization", &auth)
                .set("User-Agent", "fardrun")
                .call()?.into_string()?.as_bytes()
        )?;
        // Download existing registry.json asset
        let assets = reg_rel.get("assets").and_then(|a| a.as_array()).cloned().unwrap_or_default();
        let registry: J = if let Some(asset) = assets.iter().find(|a| {
            a.get("name").and_then(|n| n.as_str()) == Some("registry.json")
        }) {
            let dl_url = asset.get("browser_download_url").and_then(|u| u.as_str()).unwrap_or("");
            let body = ureq::get(dl_url).call()?.into_string()?;
            json_from_slice(body.as_bytes()).unwrap_or_else(|_| {
                let mut m = std::collections::BTreeMap::new();
                m.insert("packages".to_string(), J::Object(std::collections::BTreeMap::new()));
                J::Object(m)
            })
        } else {
            let mut m = std::collections::BTreeMap::new();
            m.insert("packages".to_string(), J::Object(std::collections::BTreeMap::new()));
            J::Object(m)
        };
        // Add new entry
        let dl_url = format!(
            "https://github.com/{}/releases/download/{}/{}",
            repo, tag, tarball_name
        );
        let key = format!("{}@{}", pkg_name, pkg_version);
        let mut entry = BTreeMap::new();
        entry.insert("url".to_string(), J::Str(dl_url));
        entry.insert("sha256".to_string(), J::Str(sha.clone()));
        // Rebuild packages map
        let mut pkgs: BTreeMap<String, J> = registry
            .get("packages")
            .and_then(|p| p.as_object())
            .map(|m| m.iter().map(|(k,v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        pkgs.insert(key, J::Object(entry));
        let mut new_registry = BTreeMap::new();
        new_registry.insert("packages".to_string(), J::Object(pkgs));
        let registry = J::Object(new_registry);
        // Delete old registry.json asset if exists
        if let Some(asset) = assets.iter().find(|a| {
            a.get("name").and_then(|n| n.as_str()) == Some("registry.json")
        }) {
            let asset_id = asset.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
            let del_url = format!("https://api.github.com/repos/{}/releases/assets/{}", repo, asset_id);
            ureq::delete(&del_url)
                .set("Authorization", &auth)
                .set("User-Agent", "fardrun")
                .call().ok();
        }
        // Upload new registry.json
        let reg_rel_id = reg_rel.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
        let reg_upload_url = format!(
            "https://uploads.github.com/repos/{}/releases/{}/assets?name=registry.json",
            repo, reg_rel_id
        );
        let reg_bytes = canonical_json_bytes(&registry);
        ureq::post(&reg_upload_url)
            .set("Authorization", &auth)
            .set("Content-Type", "application/json")
            .set("User-Agent", "fardrun")
            .send_bytes(&reg_bytes)?;
        eprintln!("[fard publish] published {}@{} ✓", pkg_name, pkg_version);
        println!("{}@{}", pkg_name, pkg_version);
        return Ok(());
    }
    if let Some(iargs) = install_args {
        use fard_v0_5_language_gate::cli::fardrun_cli::InstallArgs;
        // Install a specific dep or all deps from fard.toml
        let deps_to_install: Vec<(String, String)> = if let Some(dep) = &iargs.dep {
            // Single dep: name@version
            let (pkg, ver) = dep.split_once('@')
                .ok_or_else(|| anyhow!("--dep must be name@version, got: {}", dep))?;
            vec![(pkg.to_string(), ver.to_string())]
        } else {
            // Read fard.toml [deps]
            let toml_src = fs::read_to_string(&iargs.manifest)
                .with_context(|| format!("cannot read {}", iargs.manifest.display()))?;
            let mut deps = Vec::new();
            let mut in_deps = false;
            for line in toml_src.lines() {
                let line = line.trim();
                if line == "[deps]" { in_deps = true; continue; }
                if line.starts_with('[') { in_deps = false; continue; }
                if in_deps && line.contains('=') {
                    let mut parts = line.splitn(2, '=');
                    let _k = parts.next().unwrap_or("").trim();
                    let v = parts.next().unwrap_or("").trim().trim_matches('"').to_string();
                    if let Some((pkg, ver)) = v.split_once('@') {
                        deps.push((pkg.to_string(), ver.to_string()));
                    }
                }
            }
            deps
        };
        if deps_to_install.is_empty() {
            eprintln!("[fard install] no deps found in {}", iargs.manifest.display());
            return Ok(());
        }
        for (pkg, ver) in &deps_to_install {
            eprintln!("[fard install] installing {}@{}...", pkg, ver);
            // Local registry: copy into cache directly
            if let Some(reg) = &iargs.registry {
                let src_path = reg.join("pkgs").join(pkg).join(ver);
                let cache_dir = fard_cache_dir();
                let dst = cache_dir.join(format!("{}@{}", pkg, ver));
                if dst.join(".fetched").exists() {
                    eprintln!("[fard install] {}@{} already cached", pkg, ver);
                    continue;
                }
                if src_path.exists() {
                    fs::create_dir_all(&dst).ok();
                    // Copy all files
                    for entry in fs::read_dir(&src_path).into_iter().flatten().flatten() {
                        let dest_file = dst.join(entry.file_name());
                        fs::copy(entry.path(), &dest_file).ok();
                    }
                    fs::write(dst.join(".fetched"), b"").ok();
                    eprintln!("[fard install] {}@{} → {} (local)", pkg, ver, dst.display());
                } else {
                    eprintln!("[fard install] ERROR {}@{}: not found in local registry at {}", pkg, ver, src_path.display());
                }
                continue;
            }
            match fetch_package(pkg, ver) {
                Ok(path) => eprintln!("[fard install] {}@{} → {}", pkg, ver, path.display()),
                Err(e) => eprintln!("[fard install] ERROR {}@{}: {}", pkg, ver, e),
            }
        }
        eprintln!("[fard install] done ({} package(s))", deps_to_install.len());
        return Ok(());
    }



    // ── Notebook subcommand ───────────────────────────────────────────────────
    if let Some(fard_v0_5_language_gate::cli::fardrun_cli::Command::Notebook(nb)) = {
        use fard_v0_5_language_gate::cli::fardrun_cli::Cli;
        Cli::parse_compat_notebook()
    }
    {
        use std::io::Write;
        let input = std::fs::read_to_string(&nb.input)
            .with_context(|| format!("cannot read notebook: {}", nb.input.display()))?;
        let out_dir = std::path::PathBuf::from(&nb.out_dir);
        std::fs::create_dir_all(&out_dir)?;

        // Parse cells: extract ```fard ... ``` blocks
        let mut output_md = String::new();
        let mut cell_idx = 0usize;
        let mut i = 0;
        let lines: Vec<&str> = input.lines().collect();

        while i < lines.len() {
            let line = lines[i];
            if line.trim_start() == "```fard" {
                // Collect cell source
                let mut cell_src = String::new();
                i += 1;
                while i < lines.len() && lines[i].trim() != "```" {
                    cell_src.push_str(lines[i]);
                    cell_src.push('\n');
                    i += 1;
                }
                i += 1; // skip closing ```

                // Write cell to temp file
                let cell_path = out_dir.join(format!("cell_{}.fard", cell_idx));
                let cell_out = out_dir.join(format!("cell_{}_out", cell_idx));
                std::fs::write(&cell_path, &cell_src)?;
                std::fs::create_dir_all(&cell_out)?;

                // Run cell
                let exe = std::env::current_exe()?;
                let status = std::process::Command::new(&exe)
                    .args(["run", "--program",
                        cell_path.to_str().unwrap(),
                        "--out", cell_out.to_str().unwrap(),
                        "--no-trace"])
                    .output()?;

                // Read result
                let result = if let Ok(r) = std::fs::read_to_string(cell_out.join("result.json")) {
                    // Pretty print the result
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r) {
                        let inner = v.get("result").unwrap_or(&v);
                        serde_json::to_string_pretty(inner).unwrap_or(r)
                    } else { r }
                } else if !status.stderr.is_empty() {
                    String::from_utf8_lossy(&status.stderr).trim().to_string()
                } else {
                    "null".to_string()
                };

                // Append to output
                output_md.push_str("```fard\n");
                output_md.push_str(&cell_src);
                output_md.push_str("```\n\n");
                output_md.push_str("```output\n");
                output_md.push_str(&result);
                output_md.push('\n');
                output_md.push_str("```\n");

                eprintln!("[cell {}] ok", cell_idx);
                cell_idx += 1;
            } else {
                // Skip existing output blocks
                if line.trim_start() == "```output" {
                    while i < lines.len() && lines[i].trim() != "```" { i += 1; }
                    i += 1;
                } else {
                    output_md.push_str(line);
                    output_md.push('\n');
                    i += 1;
                }
            }
        }

        // Write output
        let out_path = nb.output.as_ref().unwrap_or(&nb.input);
        std::fs::write(out_path, &output_md)?;
        eprintln!("notebook: {} cell(s) executed → {}", cell_idx, out_path.display());
        return Ok(());
    }

    let program = run.program;
    let out_dir = run.out;

    // ── Strict type checking ──────────────────────────────────────────────────
    if run.strict_types {
        let src_text = match fs::read_to_string(&program) {
            Ok(s) => s,
            Err(e) => bail!("cannot read {}: {}", program.display(), e),
        };
        let errors = fard_v0_5_language_gate::type_check_strict(&src_text, &program.to_string_lossy());
        if !errors.is_empty() {
            fs::create_dir_all(&out_dir).ok();
            // Write error.json with type errors
            let type_errors: Vec<_> = errors.iter().map(|(line, col, msg)| {
                let mut m = Map::new();
                m.insert("line".to_string(), J::Int(*line as i64));
                m.insert("col".to_string(), J::Int(*col as i64));
                m.insert("message".to_string(), J::Str(msg.clone()));
                J::Object(m)
            }).collect();
            let mut em = Map::new();
            em.insert("code".to_string(), J::Str("ERROR_TYPE".to_string()));
            em.insert("message".to_string(), J::Str(format!("{} type error(s)", errors.len())));
            em.insert("type_errors".to_string(), J::Array(type_errors));
            em.insert("strict_types".to_string(), J::Bool(true));
            fs::write(
                out_dir.join("error.json"),
                json_to_string(&J::Object(em)).into_bytes(),
            )?;
            for (line, _col, msg) in &errors {
                eprintln!("TYPE ERROR line {}: {}", line, msg);
            }
            eprintln!("{} type error(s) — run aborted (--strict-types)", errors.len());
            std::process::exit(2);
        }
        eprintln!("[strict-types] ok — 0 errors");
    }


    let lockfile = run.lockfile;
    let registry_dir = run.registry;
    set_program_args(run.program_args.clone());
    fs::create_dir_all(&out_dir).ok();
    let trace_path = out_dir.join("trace.ndjson");
    let result_path = out_dir.join("result.json");
    let devnull_trace = std::env::temp_dir().join("fard_null_trace.ndjson");
    let effective_trace = if run.no_trace { &devnull_trace } else { &trace_path };
    let mut tracer = Tracer::new(&out_dir, effective_trace)?;
    let mut loader = ModuleLoader::new(program.parent().unwrap_or(Path::new(".")));
    // Load fard.toml from program directory for pkg dep resolution
    let fard_toml_path = program.parent().unwrap_or(Path::new(".")).join("fard.toml");
    loader.load_fard_toml(&fard_toml_path);
    let runtime_version = env!("CARGO_PKG_VERSION");
    let trace_format_version = "0.1.0";
    if let Some(rp) = registry_dir.clone() {
        loader.registry_dir = Some(rp);
    }
    if let Some(ref lockp) = lockfile {
        loader.lock = Some(Lockfile::load(lockp)?);
    } else if run.enforce_lockfile {
        bail!("ERROR_LOCK --enforce-lockfile requires --lockfile <path>");
    }
    loader.enforce_lockfile = run.enforce_lockfile;
    let mut _fp_prev_digest: Option<String> = None;
    let mut _fp_attempt = 0u32;
    SELF_DIGEST_ACCESSED.with(|a| *a.borrow_mut() = false);
    SELF_DIGEST.with(|d| *d.borrow_mut() = "sha256:pending".to_string());
    'fp: loop {
    _fp_attempt += 1;
    if _fp_attempt > 1 {
        let _ = fs::remove_dir_all(&out_dir);
        fs::create_dir_all(&out_dir)?;
        tracer = Tracer::new(&out_dir, &trace_path)?;
        loader = ModuleLoader::new(program.parent().unwrap_or(Path::new(".")));
        let fard_toml_path2 = program.parent().unwrap_or(Path::new(".")).join("fard.toml");
        loader.load_fard_toml(&fard_toml_path2);
        if let Some(rp) = registry_dir.clone() { loader.registry_dir = Some(rp); }
        if let Some(ref lockp) = lockfile { loader.lock = Some(Lockfile::load(lockp)?); }
        loader.enforce_lockfile = run.enforce_lockfile;
        SELF_DIGEST_ACCESSED.with(|a| *a.borrow_mut() = false);
        WITNESS_DEPS.with(|d| d.borrow_mut().clear());
    }
    let v = match loader.eval_main(&program, &mut tracer) {
        Ok(v) => v,
        Err(e) if e.downcast_ref::<QMarkUnwind>().is_some() => {
            // Top-level QMarkUnwind — the program's final expression used ?
            // and produced an err. Treat as witnessed failure.
            let q = e.downcast_ref::<QMarkUnwind>().unwrap();
            let err_val = q.err.clone();
            // Write witnessed failure to trace
            let partial_digest = if let Ok(trace_bytes) = std::fs::read(effective_trace) {
                format!("sha256:{}", {
                    use sha2::Digest;
                    let mut h = sha2::Sha256::new();
                    h.update(&trace_bytes);
                    hex::encode(h.finalize())
                })
            } else { "sha256:no-trace".to_string() };
            let _ = tracer.emit_raw(&format!(
                r#"{{"code":"QMARK_PROPAGATE_ERR","partial_trace_digest":"{}","t":"witnessed_failure"}}"#,
                partial_digest
            ));
            // Write error.json
            let clean_msg = match &err_val {
                Val::Text(s) => format!("error propagated: {}", s),
                Val::Record(m) => m.get("e")
                    .and_then(|v| if let Val::Text(s) = v { Some(format!("error propagated: {}", s)) } else { None })
                    .unwrap_or_else(|| "error propagated".to_string()),
                _ => "error propagated".to_string(),
            };
            let mut em = Map::new();
            em.insert("code".to_string(), J::Str("QMARK_PROPAGATE_ERR".to_string()));
            em.insert("message".to_string(), J::Str(clean_msg));
            em.insert("partial_trace_digest".to_string(), J::Str(partial_digest));
            em.insert("witnessed_failure".to_string(), J::Bool(true));
            if let Some(j) = err_val.to_json() { em.insert("e".to_string(), j.clone()); }
            fs::write(out_dir.join("error.json"), json_to_string(&J::Object(em)).into_bytes())?;
            // Emit error event to trace so existing tests see "t":"error"
            let err_j = err_val.to_json().unwrap_or(J::Null);
            tracer.error_event_with_e("QMARK_PROPAGATE_ERR",
                &format!("QMARK_PROPAGATE_ERR {:?}", err_val), &err_j).ok();
            // Write module_graph and digests before exit
            {
                let mg = loader.graph.to_json();
                let b = canonical_json_bytes(&mg);
                fs::write(out_dir.join("module_graph.json"), &b).ok();
                let cid = sha256_bytes(&b);
                let _ = tracer.module_graph_event(&cid);
            }
            let stdlib_root_digest = loader.stdlib_root_digest();
            drop(tracer);
            write_m5_digests(
                &out_dir,
                runtime_version,
                trace_format_version,
                &stdlib_root_digest,
                false,
                None,
                run.no_trace,
            ).ok();
            eprintln!("Error: QMARK_PROPAGATE_ERR {:?}", err_val);
            std::process::exit(1);
        }
        Err(e) => {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let v = loader.graph.to_json();
                let b = canonical_json_bytes(&v);
                let _ = fs::write(tracer.out_dir.join("module_graph.json"), &b);
            }));
            let msg0 = e.root_cause().to_string();
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
                    // Clean human-readable message
                    let clean_msg = match &q.err {
                        Val::Text(s) => format!("error propagated: {}", s),
                        Val::Record(m) => m.get("e")
                            .and_then(|v| if let Val::Text(s) = v { Some(format!("error propagated: {}", s)) } else { None })
                            .unwrap_or_else(|| "error propagated".to_string()),
                        _ => "error propagated".to_string(),
                    };
                    em.insert("message".to_string(), J::Str(clean_msg));
                }
            }

            em.insert("code".to_string(), J::Str(code.clone()));
            let final_msg = if code == QMARK_PROPAGATE_ERR {
                // Strip "QMARK_PROPAGATE_ERR Text(\"...\")" to clean message
                let raw = msg.trim_start_matches(QMARK_PROPAGATE_ERR).trim().to_string();
                // raw is like: Text("division by zero") or just the error text
                if raw.starts_with("Text(\"") && raw.ends_with("\")") {
                    format!("error propagated: {}", &raw[6..raw.len()-2])
                } else if raw.starts_with('"') && raw.ends_with('"') {
                    format!("error propagated: {}", &raw[1..raw.len()-1])
                } else {
                    format!("error propagated: {}", raw)
                }
            } else { msg.clone() };
            em.insert("message".to_string(), J::Str(final_msg));
            // Walk anyhow context chain for "  --> file:line:col" added by eval_items
            if !em.contains_key("span") {
                for cause in e.chain() {
                    let s = cause.to_string();
                    if let Some(rest) = s.strip_prefix("  --> ") {
                        // rest = "file:line:col"
                        let parts: Vec<&str> = rest.rsplitn(3, ':').collect();
                        if parts.len() == 3 {
                            if let (Ok(col), Ok(line)) = (parts[0].trim().parse::<i64>(), parts[1].trim().parse::<i64>()) {
                                let file = parts[2].to_string();
                                let mut sm = Map::new();
                                sm.insert("file".to_string(), J::Str(file));
                                sm.insert("line".to_string(), J::Int(line));
                                sm.insert("col".to_string(), J::Int(col));
                                em.insert("span".to_string(), J::Object(sm));
                            }
                        }
                        break;
                    }
                }
            }
            if let Some(se) = e.downcast_ref::<SpannedRuntimeError>() {
                let mut bs = se.span.byte_start;
                let mut be = se.span.byte_end;
                let mut ln = se.span.line;
                let mut cl = se.span.col;
                if let Ok(src) = fs::read_to_string(&se.span.file) {
                    let abs_s = {
                        let mut i = bs.min(src.len());
                        while i > 0 && !src.is_char_boundary(i) { i -= 1; }
                        i
                    };
                    let ls = src[..abs_s].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_s = abs_s.saturating_sub(ls);
                    let abs_e = {
                        let mut i = be.min(src.len());
                        while i > 0 && !src.is_char_boundary(i) { i -= 1; }
                        i
                    };
                    let le = src[..abs_e].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_e = abs_e.saturating_sub(le);
                    bs = rel_s;
                    be = rel_e;
                    cl = rel_s + 1;
                    ln = src[..ls].bytes().filter(|b| *b == b"\n"[0]).count() + 1;
                }
                let mut sm = Map::new();
                sm.insert("file".to_string(), J::Str(se.span.file.clone()));
                sm.insert("byte_start".to_string(), J::Int(bs as i64));
                sm.insert("byte_end".to_string(), J::Int(be as i64));
                sm.insert("line".to_string(), J::Int(ln as i64));
                sm.insert("col".to_string(), J::Int(cl as i64));
                em.insert("span".to_string(), J::Object(sm));
            } else if let Some(pe) = e.downcast_ref::<ParseError>() {
                // Stored spans are absolute offsets; G39 expects line-relative byte offsets.
                let mut bs = pe.span.byte_start;
                let mut be = pe.span.byte_end;
                let mut ln = pe.span.line;
                let mut cl = pe.span.col;
                if let Ok(src) = fs::read_to_string(&pe.span.file) {
                    // Snap byte offsets to valid char boundaries (source may contain multibyte chars)
                    let abs_s = {
                        let mut i = bs.min(src.len());
                        while i > 0 && !src.is_char_boundary(i) { i -= 1; }
                        i
                    };
                    let ls = src[..abs_s].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_s = abs_s.saturating_sub(ls);
                    let abs_e = {
                        let mut i = be.min(src.len());
                        while i > 0 && !src.is_char_boundary(i) { i -= 1; }
                        i
                    };
                    let le = src[..abs_e].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_e = abs_e.saturating_sub(le);
                    bs = rel_s;
                    be = rel_e;
                    cl = rel_s + 1;
                    ln = src[..ls].bytes().filter(|b| *b == b"\n"[0]).count() + 1;
                }
                let mut sm = Map::new();
                sm.insert("file".to_string(), J::Str(pe.span.file.clone()));
                sm.insert("byte_start".to_string(), J::Int(bs as i64));
                sm.insert("byte_end".to_string(), J::Int(be as i64));
                sm.insert("line".to_string(), J::Int(ln as i64));
                sm.insert("col".to_string(), J::Int(cl as i64));
                em.insert("span".to_string(), J::Object(sm));
            }
            // Witnessed failure: attach partial trace digest at failure point
            {
                let trace_path = effective_trace;
                if let Ok(trace_bytes) = fs::read(trace_path) {
                    let partial_digest = format!("sha256:{}", {
                        use sha2::Digest;
                        let mut h = sha2::Sha256::new();
                        h.update(&trace_bytes);
                        hex::encode(h.finalize())
                    });
                    em.insert("partial_trace_digest".to_string(), J::Str(partial_digest.clone()));
                    em.insert("witnessed_failure".to_string(), J::Bool(true));
                    // Emit witnessed_failure event to trace
                    let _ = tracer.emit_raw(&format!(
                        r#"{{"code":"{}","partial_trace_digest":"{}","t":"witnessed_failure"}}"#,
                        em.get("code").and_then(|v| if let J::Str(s) = v { Some(s.as_str()) } else { None }).unwrap_or("ERROR_RUNTIME"),
                        partial_digest
                    ));
                }
            }
            fs::write(
                out_dir.join("error.json"),
                json_to_string(&J::Object(em)).into_bytes(),
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
                    None,
                    run.no_trace,
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
            let _fp_subst = _fp_prev_digest.as_deref();
            write_m5_digests(
                &out_dir,
                runtime_version,
                trace_format_version,
                &stdlib_root_digest,
                true,
                _fp_subst,
                run.no_trace,
            )?;
            // Fixed-point check for w.self_digest()
            if let Ok(dig_bytes) = fs::read(out_dir.join("digests.json")) {
                if let Ok(dig_json) = json_from_slice(&dig_bytes) {
                    if let Some(J::Str(run_id)) = dig_json.get("preimage_sha256") {
                        let run_id = run_id.clone();
                        let accessed = SELF_DIGEST_ACCESSED.with(|a| *a.borrow());
                        let converged = _fp_prev_digest.as_deref() == Some(run_id.as_str());
                        if accessed && !converged && _fp_attempt < 3 {
                            _fp_prev_digest = Some(run_id.clone());
                            SELF_DIGEST.with(|d| *d.borrow_mut() = run_id.clone());
                            tracer = Tracer::new(&out_dir, &trace_path)?;
                            continue 'fp;
                        }
                        if accessed && !converged {
                            bail!("ERROR_SELF_DIGEST_DIVERGE w.self_digest() did not converge after 3 iterations");
                        }
                        SELF_DIGEST.with(|d| *d.borrow_mut() = run_id.clone());
                        let hex = run_id.strip_prefix("sha256:").unwrap_or(&run_id);
                        fs::create_dir_all("receipts").ok();
                        let receipt_path = format!("receipts/sha256_{}.json", hex);
                        let output: J = fs::read(out_dir.join("result.json"))
                            .ok()
                            .and_then(|b| json_from_slice(&b).ok())
                            .and_then(|j| j.get("result").cloned())
                            .unwrap_or(J::Null);
                        let deps = WITNESS_DEPS.with(|d| d.borrow().clone());
                        let mut receipt = BTreeMap::new();
                        receipt.insert("derived_from".to_string(),
                            J::Array(deps.into_iter().map(J::Str).collect()));
                        receipt.insert("output".to_string(), output);
                        receipt.insert("run_id".to_string(), J::Str(run_id));
                        let _ = fs::write(&receipt_path, canonical_json_bytes(&J::Object(receipt)));
                    }
                }
            }
        } // end inner block
    } // end success block
    break 'fp;
    } // end fp loop
    Ok(())
}
struct Tracer {
    first_event: bool,
    artifact_cids: std::collections::BTreeMap<String, String>,
    w: fs::File,
    out_dir: PathBuf,
}
impl Tracer {
    fn emit_raw(&mut self, line: &str) -> Result<()> {
        // Use same newline convention as write_ndjson:
        // newline BEFORE each event except the first
        if !self.first_event {
            std::io::Write::write_all(&mut self.w, b"\n")?;
        }
        std::io::Write::write_all(&mut self.w, line.as_bytes())?;
        self.first_event = false;
        Ok(())
    }

    fn module_graph_event(&mut self, cid: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("module_graph".to_string()));
        m.insert("cid".to_string(), J::Str(cid.to_string()));
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
        m.insert("t".to_string(), J::Str("emit".to_string()));
        m.insert("v".to_string(), v.clone());
        let line = json_to_string(&J::Object(m));
        self.write_ndjson(&line)?;
        Ok(())
    }

    fn emit_event(&mut self, ev: J) -> Result<()> {
        let line = json_to_string(&ev);
        self.write_ndjson(&line)?;
        Ok(())
    }
    fn grow_node(&mut self, v: &Val) -> Result<()> {
        let j = v.to_json().context("grow_node must be jsonable")?;
        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("grow_node".to_string()));
        m.insert("v".to_string(), j);
        let line = json_to_string(&J::Object(m));
        self.write_ndjson(&line)?;
        Ok(())
    }
    fn note_artifact_dep(&mut self, run_id: &str) -> Result<()> {
        let mut m = std::collections::BTreeMap::new();
        m.insert("run_id".to_string(), J::Str(run_id.to_string()));
        m.insert("t".to_string(), J::Str("artifact_dep".to_string()));
        let line = json_to_string(&J::Object(m)) + "
";
        std::io::Write::write_all(&mut self.w, line.as_bytes())?;
        WITNESS_DEPS.with(|d| d.borrow_mut().push(run_id.to_string()));
        Ok(())
    }

    fn child_spawn(&mut self, spawn_id: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("child_spawn".to_string()));
        m.insert("spawn_id".to_string(), J::Str(spawn_id.to_string()));
        self.emit_event(J::Object(m))
    }

    fn child_receipt(&mut self, spawn_id: &str, run_digest: &str, result_digest: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("child_receipt".to_string()));
        m.insert("spawn_id".to_string(), J::Str(spawn_id.to_string()));
        m.insert("run_digest".to_string(), J::Str(run_digest.to_string()));
        m.insert("result_digest".to_string(), J::Str(result_digest.to_string()));
        self.emit_event(J::Object(m))
    }

    fn artifact_in(&mut self, path: &str, cid: &str) -> Result<()> {
        // legacy import_artifact: treat path as the stable name
        self.artifact_cids.insert(path.to_string(), cid.to_string());

        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("artifact_in".to_string()));
        m.insert("name".to_string(), J::Str(path.to_string()));
        m.insert("path".to_string(), J::Str(path.to_string()));
        m.insert("cid".to_string(), J::Str(cid.to_string()));
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

        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("artifact_out".to_string()));
        m.insert("name".to_string(), J::Str(name.to_string()));
        m.insert("cid".to_string(), J::Str(cid.to_string()));
        m.insert("parents".to_string(), J::Array(vec![]));
        self.emit_event(J::Object(m))
    }

    fn artifact_in_named(&mut self, name: &str, path: &str, cid: &str) -> Result<()> {
        self.artifact_cids.insert(name.to_string(), cid.to_string());

        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("artifact_in".to_string()));
        m.insert("name".to_string(), J::Str(name.to_string()));
        m.insert("path".to_string(), J::Str(path.to_string()));
        m.insert("cid".to_string(), J::Str(cid.to_string()));
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
            let mut pm = Map::new();
            pm.insert("name".to_string(), J::Str(pname.clone()));
            pm.insert("cid".to_string(), J::Str(pcid.clone()));
            plist.push(J::Object(pm));
        }

        let mut m = Map::new();
        m.insert("cid".to_string(), J::Str(cid.to_string()));
        m.insert("name".to_string(), J::Str(name.to_string()));
        m.insert("parents".to_string(), J::Array(plist));
        m.insert("t".to_string(), J::Str("artifact_out".to_string()));
        self.emit_event(J::Object(m))
    }
    fn module_resolve(&mut self, name: &str, kind: &str, cid: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("module_resolve".to_string()));
        m.insert("name".to_string(), J::Str(name.to_string()));
        m.insert("kind".to_string(), J::Str(kind.to_string()));
        m.insert("cid".to_string(), J::Str(cid.to_string()));
        let line = json_to_string(&J::Object(m));
        self.write_ndjson(&line)?;
        Ok(())
    }

    fn error_event_with_e(&mut self, code: &str, message: &str, e: &J) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("error".to_string()));
        m.insert("code".to_string(), J::Str(code.to_string()));
        let mut s = message.to_string();
        if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
            s = rest.to_string();
        }
        if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
            s = rest.to_string();
        }
        m.insert("message".to_string(), J::Str(format!("{} {}", code, s)));
        m.insert("e".to_string(), e.clone());
        let line = json_to_string(&J::Object(m));
        self.write_ndjson(&line)?;
        Ok(())
    }

    fn error_event(&mut self, code: &str, message: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::Str("error".to_string()));
        m.insert("code".to_string(), J::Str(code.to_string()));
        let mut s = message.to_string();
        if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
            s = rest.to_string();
        }
        if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
            s = rest.to_string();
        }
        m.insert("message".to_string(), J::Str(format!("{} {}", code, s)));
        let line = json_to_string(&J::Object(m));
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
    StrInterp(Vec<StrPart>),
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
    fn peek_char(&self) -> Option<char> {
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
                    "let", "in", "fn", "if", "then", "else", "import", "as", "export", "match", "test", "while", "return",
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
                // Check for scientific notation: e/E [+-] digits
                let base = n as f64 + frac;
                if self.peek().map(|c| c == 'e' || c == 'E').unwrap_or(false) {
                    self.i += 1; // consume e/E
                    let neg_exp = if self.peek() == Some('-') { self.i += 1; true }
                                  else if self.peek() == Some('+') { self.i += 1; false }
                                  else { false };
                    let mut exp: i32 = 0;
                    while let Some(d) = self.peek() {
                        if d.is_ascii_digit() { exp = exp * 10 + (d as i32 - '0' as i32); self.i += 1; }
                        else { break; }
                    }
                    let factor = 10f64.powi(if neg_exp { -exp } else { exp });
                    return Ok(Tok::Float(base * factor));
                }
                return Ok(Tok::Float(base));
            }
            return Ok(Tok::Num(n));
        }
        if c == '`' {
            self.bump();
            let mut t = String::new();
            while let Some(d) = self.bump() {
                if d == '`' { break; }
                t.push(d);
            }
            return Ok(Tok::Str(t));
        }
        if c == '"' {
            self.bump();
            let mut t = String::new();
            let mut parts: Vec<StrPart> = Vec::new();
            let mut has_interp = false;
            while let Some(d) = self.bump() {
                if d == '"' {
                    break;
                }
                if d == '$' && self.peek_char() == Some('{') {
                    // string interpolation: collect what we have, then parse expr
                    has_interp = true;
                    self.bump(); // consume '{'
                    parts.push(StrPart::Lit(t.clone()));
                    t.clear();
                    // collect the inner expression source until matching '}'
                    let mut depth = 1usize;
                    let mut inner = String::new();
                    loop {
                        match self.bump() {
                            None => bail!("unterminated ${{}}"),
                            Some('{') => { depth += 1; inner.push('{'); }
                            Some('}') => {
                                depth -= 1;
                                if depth == 0 { break; }
                                inner.push('}');
                            }
                            Some(c) => inner.push(c),
                        }
                    }
                    // parse the inner expression
                    let file = "<interp>".to_string();
                    let mut ip = Parser::from_src(&inner, &file)?;
                    let e = ip.parse_expr()?;
                    parts.push(StrPart::Expr(e));
                    continue;
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
            if has_interp {
                parts.push(StrPart::Lit(t));
                return Ok(Tok::StrInterp(parts));
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
    StrInterp(Vec<StrPart>),
    Null,
    Bin(String, Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
    Try(Box<Expr>),
    Match(Box<Expr>, Vec<MatchArm>),
    Using(Pat, Box<Expr>, Box<Expr>),
    While(Box<Expr>, Box<Expr>, Box<Expr>),
    Return(Box<Expr>),
    NamedCall(Box<Expr>, Vec<(String, Expr)>),
}
#[derive(Clone, Debug)]
enum Item {
    Import(String, String),
    Artifact(String, String),   // name, run_id — witness composition
    Let(String, Expr, Option<ErrorSpan>),
    Fn(String, Vec<(Pat, Option<Type>)>, Option<Type>, Expr),
    Export(Vec<String>),
    TypeDef(String, TypeDefKind),
    Test(String, Expr, ErrorSpan),
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
    /// Returns true if there is a newline in the source between the previous
    /// token and the current token. Used to disambiguate `expr\n[...]` (new expr)
    /// from `expr[...]` (index) at statement boundaries.
    fn newline_before_current(&self) -> bool {
        if self.i == 0 { return false; }
        let prev_end = self.spans.get(self.i - 1).map(|s| s.1).unwrap_or(0);
        let cur_start = self.spans.get(self.i).map(|s| s.0).unwrap_or(0);
        self.src.get(prev_end..cur_start)
            .map(|gap| gap.contains('\n'))
            .unwrap_or(false)
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
            // Support destructuring: let {a, b} = expr or let [a, b] = expr
            let is_destructure = matches!(self.peek(), Tok::Sym(s) if s == "{" || s == "[");
            if is_destructure {
                let pat = self.parse_pat()?;
                self.expect_sym("=")?;
                let rhs = self.parse_expr()?;
                binds.push(("__destruct__".to_string(), rhs.clone()));
                // Desugar: bind temp var, then extract each field
                let tmp = "__destruct_tmp__".to_string();
                // We'll handle this via LetPat in the tail
                // Push a special marker — handled below
                // Actually: desugar directly into multiple binds
                match pat {
                    Pat::Obj { ref items, .. } => {
                        let tmp_expr = Expr::Var(tmp.clone());
                        // Remove the __destruct__ placeholder we just pushed
                        binds.pop();
                        binds.push((tmp.clone(), rhs));
                        for (field, sub_pat) in items {
                            let field_expr = Expr::Get(Box::new(Expr::Var(tmp.clone())), field.clone());
                            match sub_pat {
                                Pat::Bind(name) => binds.push((name.clone(), field_expr)),
                                _ => binds.push((field.clone(), field_expr)),
                            }
                        }
                    }
                    Pat::Bind(name) => binds.push((name, rhs)),
                    _ => binds.push(("_".to_string(), rhs)),
                }
                continue;
            }
            let name = self.expect_ident()?;
            self.expect_sym("=")?;
            let rhs = self.parse_expr()?;
            // If followed by `in`, this is a let-in expression
            if self.eat_kw("in") {
                let mut tail = self.parse_expr()?;
                tail = Expr::Let(name, Box::new(rhs), Box::new(tail));
                for (n, r) in binds.into_iter().rev() {
                    tail = Expr::Let(n, Box::new(r), Box::new(tail));
                }
                return Ok(tail);
            }
            // If followed by `|` (pipe/sequence), treat as: let name = rhs; next_expr
            if matches!(self.peek(), Tok::Sym(s) if s == "|")
                && !matches!(self.peek_n(1), Tok::Sym(s) if s == "|")
            {
                self.bump(); // consume |
                binds.push((name, rhs));
                // parse the continuation as a new block
                let mut tail = self.parse_fn_block_inner()?;
                // Note: parse_fn_block_inner will handle further lets and pipes
                for (n, r) in binds.into_iter().rev() {
                    tail = Expr::Let(n, Box::new(r), Box::new(tail));
                }
                return Ok(tail);
            }
            binds.push((name, rhs));
        }
        let mut tail = self.parse_expr()?;
        // Support `expr | expr` as sequencing: bind result of lhs as `_`, eval rhs
        while matches!(self.peek(), Tok::Sym(s) if s == "|") {
            // Only treat | as pipe if next token after | is not |  (avoid || operator)
            if matches!(self.peek_n(1), Tok::Sym(s) if s == "|") { break; }
            self.bump(); // consume |
            let rhs = self.parse_expr()?;
            tail = Expr::Let("_".to_string(), Box::new(tail), Box::new(rhs));
        }
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
            // test "name" { expr }
            if self.eat_kw("test") {
                let label = match self.bump() {
                    Tok::Str(s) => s,
                    other => bail!("ERROR_PARSE test expects a string label, got {:?}", other),
                };
                let span = self.cur_span();
                self.expect_sym("{")?;
                let body = self.parse_fn_block_inner()?;
                self.expect_sym("}")?;
                items.push(Item::Test(label, body, span));
                continue;
            }
            // "a Point is { x: Int, y: Int }" or "a Shape is Circle(r: Int) or Rect(w: Int, h: Int)"
            if matches!(self.peek(), Tok::Ident(s) if s == "a") { self.bump();
                let type_name = self.expect_ident()?;
                match self.bump() {
                    Tok::Ident(s) if s == "is" => {}
                    other => bail!("ERROR_PARSE expected 'is', got {:?}", other),
                }
                let kind = if matches!(self.peek(), Tok::Sym(s) if s == "{") {
                    // record type: a Point is { x: Int, y: Int }
                    self.expect_sym("{")?;
                    let mut fields = Vec::new();
                    while !matches!(self.peek(), Tok::Sym(s) if s == "}") {
                        let fname = self.expect_ident()?;
                        self.expect_sym(":")?;
                        let tname = self.expect_ident()?;
                        fields.push(TypeField::Named(fname, tname));
                        self.eat_sym(",");
                    }
                    self.expect_sym("}")?;
                    TypeDefKind::Record(fields)
                } else {
                    // sum type: a Shape is Circle(r: Int) or Rect(w: Int, h: Int)
                    let mut variants = Vec::new();
                    loop {
                        let vname = self.expect_ident()?;
                        let mut fields = Vec::new();
                        if matches!(self.peek(), Tok::Sym(s) if s == "(") {
                            self.expect_sym("(")?;
                            while !matches!(self.peek(), Tok::Sym(s) if s == ")") {
                                let fname = self.expect_ident()?;
                                self.expect_sym(":")?;
                                let tname = self.expect_ident()?;
                                fields.push(TypeField::Named(fname, tname));
                                self.eat_sym(",");
                            }
                            self.expect_sym(")")?;
                        }
                        variants.push((vname, fields));
                        if !matches!(self.peek(), Tok::Ident(s) if s == "or") { break; } else { self.bump(); }
                    }
                    TypeDefKind::Sum(variants)
                };
                items.push(Item::TypeDef(type_name, kind));
                continue;
            }
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
            if self.eat_kw("artifact") {
                let name = self.expect_ident()?;
                self.expect_sym("=")?;
                let run_id = match self.bump() {
                    Tok::Str(s) => s,
                    _ => bail!("ERROR_PARSE artifact requires run_id string"),
                };
                if !run_id.starts_with("sha256:") {
                    bail!("ERROR_PARSE artifact run_id must start with 'sha256:'");
                }
                items.push(Item::Artifact(name, run_id));
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
                let mut defaults: Vec<(String, Expr)> = Vec::new();
                if !self.eat_sym(")") {
                    loop {
                        let p = self.parse_pat()?;
                        // Check for default value: param = expr
                        let param_name = if let Pat::Bind(ref n) = p { Some(n.clone()) } else { None };
                        if let Some(pname) = param_name {
                            if self.eat_sym("=") {
                                let default_expr = self.parse_expr()?;
                                defaults.push((pname, default_expr));
                                params.push((p, None));
                                if self.eat_sym(")") { break; }
                                self.expect_sym(",")?;
                                continue;
                            }
                        }
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
                let mut body = self.parse_fn_block_body()?;
                // Desugar default args: prepend let bindings for defaulted params
                // fn f(a, b = expr) { body } ->
                // fn f(a, b) { body }  (full arity, used directly)
                // fn f__d(a) { f(a, expr) }  (short-arity helper, name__d suffix removed params count)
                if !defaults.is_empty() {
                    // Desugar default args by wrapping body with null-sentinel defaults.
                    // fn f(a, b = expr) { body }
                    // becomes: fn f(a, b) { let b = if b == null then expr else b in body }
                    // Callers use null explicitly for defaults: f(1, null) or f(1) via short helper.
                    let n_defaults = defaults.len();
                    let n_required = params.len() - n_defaults;
                    // Wrap body: for each default param, prepend: let param = if param == null then default else param
                    let mut wrapped = body;
                    for (pname, dexpr) in defaults.iter().rev() {
                        let check = Expr::If(
                            Box::new(Expr::Bin("==".to_string(),
                                Box::new(Expr::Var(pname.clone())),
                                Box::new(Expr::Null))),
                            Box::new(dexpr.clone()),
                            Box::new(Expr::Var(pname.clone())),
                        );
                        wrapped = Expr::Let(pname.clone(), Box::new(check), Box::new(wrapped));
                    }
                    // Emit full-arity fn with wrapped body (accepts null for defaults)
                    items.push(Item::Fn(name.clone(), params.clone(), ret, wrapped));
                    // Emit short-arity helper that passes null for defaulted params
                    let required_params: Vec<(Pat, Option<Type>)> = params[..n_required].to_vec();
                    let mut call_args: Vec<Expr> = required_params.iter().map(|(p, _)| {
                        if let Pat::Bind(n) = p { Expr::Var(n.clone()) } else { Expr::Null }
                    }).collect();
                    for _ in &defaults {
                        call_args.push(Expr::Null);
                    }
                    let call_body = Expr::Call(Box::new(Expr::Var(name.clone())), call_args);
                    let helper_name = format!("{}__d{}", name, n_required);
                    items.push(Item::Fn(helper_name, required_params, None, call_body));
                } else {
                    items.push(Item::Fn(name, params, ret, body));
                }
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
                // Support destructuring: let {a, b} = expr
                if matches!(self.peek(), Tok::Sym(s) if s == "{" || s == "[") {
                    let pat = self.parse_pat()?;
                    self.expect_sym("=")?;
                    let rhs = self.parse_expr()?;
                    // Desugar into a temp binding + field extractions
                    let tmp = "__destruct_top__".to_string();
                    items.push(Item::Let(tmp.clone(), rhs, None));
                    match pat {
                        Pat::Obj { items: fields, .. } => {
                            for (field, sub_pat) in fields {
                                let field_expr = Expr::Get(
                                    Box::new(Expr::Var(tmp.clone())),
                                    field.clone()
                                );
                                let bind_name = match sub_pat {
                                    Pat::Bind(n) => n,
                                    _ => field,
                                };
                                items.push(Item::Let(bind_name, field_expr, None));
                            }
                        }
                        Pat::Bind(name) => {
                            items.push(Item::Let(name, Expr::Var(tmp), None));
                        }
                        _ => {}
                    }
                    continue;
                }
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
                        // Shorthand: {name} means {name: name}
                        let sub = if self.eat_sym(":") {
                            self.parse_pat()?
                        } else {
                            Pat::Bind(k.clone())
                        };
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
        if self.eat_kw("while") {
            let init = self.parse_expr()?;
            let cond = self.parse_expr()?;
            let body = self.parse_expr()?;
            return Ok(Expr::While(Box::new(init), Box::new(cond), Box::new(body)));
        }
        // `for pat in xs do body` — desugar to list.map(xs, fn(pat) { body })
        if self.eat_kw("for") {
            let pat = self.parse_pat()?;
            self.expect_kw("in")?;
            let xs = self.parse_expr()?;
            self.expect_kw("do")?;
            let body = self.parse_expr()?;
            // Desugar: list.map(xs, fn(pat) { body })
            let list_var = Expr::Var("list".to_string());
            let map_fn = Expr::Get(Box::new(list_var), "map".to_string());
            let lambda = Expr::Fn(vec![pat], Box::new(body));
            return Ok(Expr::Call(Box::new(map_fn), vec![xs, lambda]));
        }
        if self.eat_kw("return") {
            let e = self.parse_expr()?;
            return Ok(Expr::Return(Box::new(e)));
        }
        if self.eat_kw("if") {
            let c = self.parse_expr()?;
            self.expect_kw("then")?;
            let t = if matches!(self.peek(), Tok::Sym(s) if s == "{") {
                // Disambiguate: `then { let ... }` block vs `then { k: v }` record.
                // A block starts with `let` or is empty. A record starts with ident/kw + colon.
                let is_block = matches!(self.peek_n(1), Tok::Kw(s) if s == "let")
                    || matches!(self.peek_n(1), Tok::Kw(s) if s == "return")
                    || matches!(self.peek_n(1), Tok::Sym(s) if s == "}");
                if is_block {
                    self.bump();
                    let body = self.parse_fn_block_inner()?;
                    self.expect_sym("}")?;
                    body
                } else {
                    self.parse_expr()?
                }
            } else {
                self.parse_expr()?
            };
            self.expect_kw("else")?;
            let f = self.parse_expr()?;
            return Ok(Expr::If(Box::new(c), Box::new(t), Box::new(f)));
        }
        self.parse_or()
    }
    /// Precedence table for infix operators (higher = tighter binding).
    /// All operators are left-associative.
    ///
    ///  1  ||
    ///  2  &&
    ///  3  == != < > <= >=
    ///  4  + -
    ///  5  * /
    fn infix_prec(tok: &Tok) -> Option<u8> {
        match tok {
            Tok::OrOr                                   => Some(1),
            Tok::Sym(s) if s == "&&"                    => Some(2),
            Tok::Sym(s) if matches!(s.as_str(),
                "==" | "!=" | "<" | ">" | "<=" | ">=") => Some(3),
            Tok::Sym(s) if s == "+" || s == "-"         => Some(4),
            Tok::Sym(s) if s == "*" || s == "/" || s == "%" => Some(5),
            _                                           => None,
        }
    }

    /// Precedence-climbing infix parser.
    /// Parses all infix operators with precedence >= `min_prec`.
    /// Call with `min_prec = 0` to parse any infix expression.
    fn parse_infix(&mut self, min_prec: u8) -> Result<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let prec = match Self::infix_prec(self.peek()) {
                Some(p) if p >= min_prec => p,
                _ => break,
            };
            // Consume the operator token and record its string form.
            let op = match self.peek().clone() {
                Tok::OrOr    => { self.bump(); "||".to_string() }
                Tok::Sym(s)  => { self.bump(); s }
                _            => unreachable!(),
            };
            // Left-associative: right side binds at prec + 1.
            let rhs = self.parse_infix(prec + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// Shim so all existing `parse_or()` call sites compile unchanged.
    #[inline(always)]
    fn parse_or(&mut self) -> Result<Expr> { self.parse_infix(0) }

    // parse_and, parse_eq, parse_add, parse_mul are replaced by parse_infix above.
    // Kept as dead-code shims if ever needed for debugging:
    #[allow(dead_code)] fn parse_and(&mut self) -> Result<Expr> { self.parse_infix(2) }
    #[allow(dead_code)] fn parse_eq(&mut self)  -> Result<Expr> { self.parse_infix(3) }
    #[allow(dead_code)] fn parse_add(&mut self) -> Result<Expr> { self.parse_infix(4) }
    #[allow(dead_code)] fn parse_mul(&mut self) -> Result<Expr> { self.parse_infix(5) }
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
                let mut named: Vec<(String, Expr)> = Vec::new();
                let mut is_named = false;
                if !self.eat_sym(")") {
                    loop {
                        // Check for named arg: ident ":"
                        let is_name = matches!(self.peek(), Tok::Ident(_))
                            && matches!(self.peek_n(1), Tok::Sym(s) if s == ":");
                        if is_name {
                            is_named = true;
                            let name = self.expect_ident()?;
                            self.expect_sym(":")?;
                            let v = self.parse_expr()?;
                            named.push((name, v));
                        } else {
                            let a = self.parse_expr()?;
                            args.push(a);
                        }
                        if self.eat_sym(")") { break; }
                        self.expect_sym(",")?;
                        if self.eat_sym(")") { break; }
                    }
                }
                if is_named {
                    e = Expr::NamedCall(Box::new(e), named);
                } else {
                    e = Expr::Call(Box::new(e), args);
                }
                continue;
            }
            if self.eat_sym("[") {
                // Do not treat [ as index if:
                // 1. The base is a bare literal (can never be indexed), or
                // 2. There is a newline between the base expression and [
                //    (statement-level disambiguation: `expr\n[...]` is a new expr)
                let is_literal = matches!(e,
                    Expr::Int(_) | Expr::FloatLit(_) | Expr::Bool(_) |
                    Expr::Str(_) | Expr::Null | Expr::List(_) | Expr::Rec(_)
                );
                // self.i was incremented by eat_sym("["), so self.i-1 is the [
                // and self.i-2 is the last token of the base expression.
                let has_newline = {
                    let bracket_idx = self.i - 1;
                    let prev_end = self.spans.get(bracket_idx - 1).map(|s| s.1).unwrap_or(0);
                    let cur_start = self.spans.get(bracket_idx).map(|s| s.0).unwrap_or(0);
                    self.src.get(prev_end..cur_start)
                        .map(|gap| gap.contains('\n'))
                        .unwrap_or(false)
                };
                if is_literal || has_newline {
                    self.i -= 1;
                    break;
                }
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
            Tok::StrInterp(parts) => Ok(Expr::StrInterp(parts)),
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
                    let first = self.parse_expr()?;
                    // Check for list comprehension: [expr for pat in xs] or [expr for pat in xs if cond]
                    if self.eat_kw("for") {
                        let pat = self.parse_pat()?;
                        self.expect_kw("in")?;
                        let iter = self.parse_expr()?;
                        // Optional `if cond`
                        let filtered = if self.eat_kw("if") {
                            let cond = self.parse_expr()?;
                            // list.filter(iter, fn(pat) { cond })
                            let list_var = Expr::Var("list".to_string());
                            let filter_fn = Expr::Get(Box::new(list_var), "filter".to_string());
                            let cond_lambda = Expr::Fn(vec![pat.clone()], Box::new(cond));
                            Expr::Call(Box::new(filter_fn), vec![iter, cond_lambda])
                        } else {
                            iter
                        };
                        self.expect_sym("]")?;
                        // list.map(filtered, fn(pat) { first })
                        let list_var2 = Expr::Var("list".to_string());
                        let map_fn = Expr::Get(Box::new(list_var2), "map".to_string());
                        let map_lambda = Expr::Fn(vec![pat], Box::new(first));
                        return Ok(Expr::Call(Box::new(map_fn), vec![filtered, map_lambda]));
                    }
                    xs.push(first);
                    loop {
                        if self.eat_sym("]") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym("]") {
                            break;
                        }
                        xs.push(self.parse_expr()?);
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
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<Val>),
    Record(BTreeMap<String, Val>),
    Err { code: String, data: Box<Val> },
    Func(Func),
    Builtin(Builtin),
    /// Method bound to a receiver. When called, receiver is prepended as first arg.
    BoundMethod(Box<Val>, Box<Val>),
    Chan(Arc<Mutex<std::collections::VecDeque<Val>>>, Arc<Mutex<bool>>),
    Mtx(Arc<Mutex<Val>>),
    Big(Box<BigInt>),
    Promise(Arc<Mutex<Option<Result<Val, String>>>>, String, PathBuf),  // slot, spawn_id, trace_path
    /// VM-compiled function — executed by the bytecode VM, not the tree-walker
    VmFunc(usize),  // index into a Vec<CompiledFn> held by the VM
}

impl Val {
    fn err(code: &str) -> Val {
        Val::Err { code: code.to_string(), data: Box::new(Val::Unit) }
    }
    fn err_data(code: &str, data: Val) -> Val {
        Val::Err { code: code.to_string(), data: Box::new(data) }
    }
    fn is_err(&self) -> bool { matches!(self, Val::Err { .. }) }
    fn type_name(&self) -> &'static str {
        match self {
            Val::Unit => "unit", Val::Bool(_) => "bool", Val::Int(_) => "int",
            Val::Float(_) => "float", Val::Text(_) => "text", Val::Bytes(_) => "bytes",
            Val::List(_) => "list", Val::Record(_) => "record",
            Val::BoundMethod(..) => "bound-method",
            Val::Err { .. } => "err", Val::Func(_) => "func", Val::Builtin(_) => "builtin",
            Val::Chan(..) => "chan",
            Val::Mtx(..) => "mutex",
            Val::Big(..) => "bigint",
            Val::Promise(..) => "promise",
            Val::VmFunc(_) => "vm-func",
        }
    }
}
#[derive(Debug)]
struct QMarkUnwind {
    err: Val,
}
impl std::fmt::Display for QMarkUnwind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match &self.err {
            Val::Text(s) => s.clone(),
            Val::Record(m) => m.get("e")
                .and_then(|v| if let Val::Text(s) = v { Some(s.clone()) } else { None })
                .unwrap_or_else(|| format!("{:?}", self.err)),
            other => format!("{:?}", other),
        };
        write!(f, "{} {}", QMARK_PROPAGATE_ERR, msg)
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
    Unimplemented(&'static str),
    // Type checking constructors
    TypeCheck(String, Vec<String>),   // type_name, required_fields
    // std/math
    MathAbs, MathMin, MathMax, MathPow, MathSqrt,
    MathFloor, MathCeil, MathRound, MathLog, MathLog2, MathExp,
    // std/bits
    BitAnd, BitOr, BitXor, BitNot, BitShl, BitShr, BitPopcount,
    // std/bytes
    BytesConcat, BytesLen, BytesGet, BytesOfList, BytesMerkleRoot, BytesOfStr, BytesToList, BytesToStr,
    // std/io
    IoReadFile, IoWriteFile, IoAppendFile, IoReadLines, IoFileExists, IoDeleteFile,
    IoReadStdin, IoListDir, IoMakeDir, IoReadStdinLines,
    ChanNew, ChanSend, ChanRecv, ChanTryRecv, ChanClose,
    MutexNew, MutexLock, MutexUnlock, MutexWithLock,
    // std/cli
    CliArgs, CliGet, CliGetInt, CliGetFloat, CliGetBool, CliHas,
    // std/null
    NullIsNull, NullCoalesce, NullGuard,
    // std/path
    PathBase, PathDir, PathExt, PathIsAbs, PathJoin, PathJoinAll, PathNormalize,
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
    ResultUnwrapOr,
    ResultIsOk,
    ResultIsErr,
    ResultMap,
    ResultMapErr,
    ResultOrElse,
    TraceInfo,
    TraceWarn,
    TraceError,
    TraceSpan,
    ListGroupBy,
    SembitPartition,
    HttpGet,
    HttpPost,
    HttpRequest,
    TimeNow,
    TimeParse,
    TimeFormat,
    TimeAdd,
    TimeSub,
    TimeDurationMs,
    TimeDurationSec,
    TimeDurationMin,
    OptionNone,
    OptionSome,
    OptionIsNone,
    OptionIsSome,
    OptionFromNullable,
    OptionToNullable,
    OptionMap,
    OptionAndThen,
    OptionUnwrapOr,
    OptionUnwrapOrElse,
    OptionToResult,
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
    CodecBase64UrlEncodeHex,
    RandUuidV4,
    StrSplit,
    StrUpper,
    StrContains,
    StrStartsWith,
    StrEndsWith,
    StrReplace,
    StrSlice,
    StrFormat,
    StrFromInt,
    StrFromFloat,
    StrPadLeft,
    StrPadRight,
    StrRepeat,
    StrIndexOf,
    StrChars,
    FsReadText,
    FsWriteText,
    FsExists,
    FsReadDir,
    FsStat,
    FsDelete,
    FsMakeDir,
    CodecHexEncode,
    CodecHexDecode,
    HashSha256Text,
    WitnessSelfDigest,   // witness.self_digest() -> Text
    WitnessDeps,         // witness.deps() -> List of run_id Text
    WitnessVerify,       // witness.verify(run_id) -> {ok: record} | {err: text}
    WitnessVerifyChain,  // witness.verify_chain(run_id) -> {ok: depth} | {err: {run_id, reason}}
    FfiOpen,   // ffi.open(path) -> {ok: handle_id} | {err: text}
    FfiCall,   // ffi.call(handle_id, symbol, args) -> {ok: val} | {err: text}
    FfiClose,  // ffi.close(handle_id) -> null
    FfiCallPure, // ffi.call_pure(handle_id, symbol, args) -> same as call but hashed into witness
    FfiCallStr,
    FfiCallChecked,  // ffi.call_str(handle_id, symbol, args) -> {ok: text} | {err: text}
    NetServe,   // net.serve(port, handler) -> never (blocking)
    NetRespond, // net.respond(req, status, headers, body) -> null (internal)
    CryptoSha512,         // crypto.sha512(bytes) -> text
    CryptoAesEncrypt,     // crypto.aes_encrypt(key_hex, nonce_hex, plaintext) -> {ok: hex} | {err: text}
    CryptoAesDecrypt,     // crypto.aes_decrypt(key_hex, nonce_hex, ciphertext_hex) -> {ok: text} | {err: text}
    CryptoMerkleRoot,     // crypto.merkle_root(list_of_hex) -> hex
    CompressGzip,         // compress.gzip(text) -> bytes_hex
    CompressGunzip,       // compress.gunzip(bytes_hex) -> {ok: text} | {err: text}
    GraphOf,       // graph.of(run_id) -> {nodes, edges} | {err: text}
    GraphAncestors, // graph.ancestors(run_id) -> list of run_ids
    GraphLeaves,    // graph.leaves(run_id) -> list of root run_ids
    GraphToDot,     // graph.to_dot(graph) -> dot string
    HashSha256Bytes,
    IntMul,
    IntDiv,
    IntSub,
    IntAbs,
    IntMin,
    IntMax,
    IntToText,
    IntFromText,
    IntNeg,
    IntClamp,
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
    LinalgRelu,
    LinalgSoftmax,
    LinalgArgmax,
    LinalgVecExp, LinalgVecLog, LinalgVecSum, LinalgVecMax, LinalgVecMul,
    LinalgVecRelu, LinalgVecReluGrad, LinalgSoftmaxGrad, LinalgCrossEntropy,
    LinalgOuter, LinalgMatMulVecGrad, LinalgVecScalarAdd, LinalgMatRowSum,
    ListSet,
    CastFloat, CastInt, CastText, StrJoin, ListAny, ListAll, ListFind, ListFindIndex, ListTake, ListDrop, ListFlatMap,
    MathSin, MathCos, MathTan, MathAtan2, IntToHex, IntToBin, FloatIsInf, TypeOf,
    EnvGet, EnvArgs, ProcessSpawn, ProcessExit,
    ReMatch, ReFind, ReFindAll, ReSplit, ReReplace, FardEval,
    Base64Encode, Base64Decode, CsvParse, CsvEncode,
    MapDelete, MapEntries,
    SetNew, SetAdd, SetRemove, SetHas, SetUnion, SetIntersect, SetDiff, SetToList, SetFromList, SetSize,
    ListZipWith, ListChunk, ListSortBy,
    MathAsin, MathAcos, MathAtan, MathLog10,
    FloatToStrFixed,
    UuidV4, UuidValidate,
    IntToStrPadded,
    BigFromInt, BigFromStr, BigAdd, BigSub, BigMul, BigDiv, BigPow, BigToStr, BigEq, BigLt, BigGt, BigMod,
    PromiseSpawn, PromiseAwait, PromiseSpawnOrdered,
    AstParse,
    DateTimeNow, DateTimeFormat, DateTimeParse, DateTimeAdd, DateTimeSub, DateTimeField,
    ListParMap,
    CellNew, CellGet, CellSet,
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
use num_bigint::BigInt;
use num_traits::Zero;

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
                // Pre-allocate for typical fn call scope (4 params)
                vars: Mutex::new(HashMap::with_capacity(4)),
                parent: Some(self.clone()),
            }),
        }
    }

    fn set(&mut self, k: String, v: Val) {
        let mut g = self.inner.vars.lock().unwrap();
        g.insert(k, v);
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.inner.vars.lock().unwrap().keys().cloned().collect();
        if let Some(ref parent) = self.inner.parent {
            keys.extend(parent.keys());
        }
        keys
    }

    fn get(&self, k: &str) -> Option<Val> {
        // Walk the scope chain without cloning Arc at each level
        let mut current: *const EnvInner = &*self.inner;
        loop {
            // SAFETY: current always points to a live EnvInner owned by an Arc
            let inner = unsafe { &*current };
            if let Some(v) = inner.vars.lock().unwrap().get(k).cloned() {
                return Some(v);
            }
            match &inner.parent {
                Some(p) => current = &*p.inner,
                None => return None,
            }
        }
    }
}
impl Val {
    #[allow(dead_code)]
    fn to_vc_json(&self) -> Option<J> {
        match self {
            Val::Int(n) => { let mut m = Map::new(); m.insert("t".to_string(), J::Str("int".to_string())); m.insert("v".to_string(), J::Int(*n)); Some(J::Object(m)) },
            Val::Bool(b) => { let mut m = Map::new(); m.insert("t".to_string(), J::Str("bool".to_string())); m.insert("v".to_string(), J::Bool(*b)); Some(J::Object(m)) },
            Val::Text(s) => { let mut m = Map::new(); m.insert("t".to_string(), J::Str("str".to_string())); m.insert("v".to_string(), J::Str(s.clone())); Some(J::Object(m)) },
            Val::Unit => { let mut m = Map::new(); m.insert("t".to_string(), J::Str("null".to_string())); m.insert("v".to_string(), J::Null); Some(J::Object(m)) },
            Val::List(xs) => {
                let mut out: Vec<J> = Vec::with_capacity(xs.len());
                for x in xs {
                    out.push(x.to_vc_json()?);
                }
                { let mut m = Map::new(); m.insert("t".to_string(), J::Str("list".to_string())); m.insert("v".to_string(), J::Array(out)); Some(J::Object(m)) }
            }
            Val::Record(m) => {
                let mut obj = Map::new();
                for (k, v) in m.iter() {
                    obj.insert(k.clone(), v.to_vc_json()?);
                }
                { let mut m = Map::new(); m.insert("t".to_string(), J::Str("rec".to_string())); m.insert("v".to_string(), J::Object(obj)); Some(J::Object(m)) }
            }
            _ => None,
        }
    }

    fn to_json(&self) -> Option<J> {
        match self {
            Val::Int(n) => Some(J::Int(*n)),
            Val::Bool(b) => Some(J::Bool(*b)),
            Val::Text(s) => Some(J::Str(s.clone())),
            Val::Unit => Some(J::Null),
            Val::List(xs) => Some(J::Array(
                xs.iter().map(|x| x.to_json()).collect::<Option<Vec<_>>>()?,
            )),
            Val::Record(m) => {
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
            Val::Float(f) => {
                Some(J::Float(*f))
            }
            Val::Bytes(bs) => {
                let mut obj = Map::new();
                obj.insert("t".to_string(), J::Str("bytes".to_string()));
                obj.insert("v".to_string(), J::Str(format!("hex:{}", hex_lower(bs))));
                Some(J::Object(obj))
            }
            Val::Err { code, .. } => Some(J::Str(format!("error:{}", code))),
            Val::Func(_) | Val::Builtin(_) | Val::BoundMethod(..) | Val::Chan(..) | Val::Mtx(..) | Val::Big(..) | Val::Promise(..) | Val::VmFunc(_) => None,
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

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        anyhow::bail!("hex_decode: odd length input");
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => anyhow::bail!("hex_decode: invalid nibble {:?}", b as char),
    }
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

fn vcore_to_fardrun(v: valuecore::Val) -> Val {
    match v {
        valuecore::Val::Unit    => Val::Unit,
        valuecore::Val::Bool(b) => Val::Bool(b),
        valuecore::Val::Int(n)  => Val::Int(n),
        valuecore::Val::Text(s) => Val::Text(s),
        valuecore::Val::Bytes(b) => Val::Bytes(b),
        valuecore::Val::List(xs) => Val::List(xs.into_iter().map(vcore_to_fardrun).collect()),
        valuecore::Val::Float(f) => Val::Float(f),
        valuecore::Val::Record(pairs) => {
            let mut m = BTreeMap::new();
            for (k, v) in pairs {
                m.insert(k, vcore_to_fardrun(v));
            }
            Val::Record(m)
        }
        valuecore::Val::Err { code, data } => Val::Err {
            code,
            data: Box::new(vcore_to_fardrun(*data)),
        },
    }
}

fn val_from_json(j: &J) -> Result<Val> {
    match j {
        J::Null => Ok(Val::Unit),
        J::Bool(b) => Ok(Val::Bool(*b)),
        J::Int(n) => Ok(Val::Int(*n)),
        J::Float(f) => {
            let f = *f;
            if true {
                Ok(Val::Float(f))
            } else {
                bail!("ERROR_RUNTIME json number out of range")
            }
        }
        J::Str(s) => Ok(Val::Text(s.clone())),
        J::Array(xs) => {
            let mut out = Vec::new();
            for x in xs {
                out.push(val_from_json(x)?);
            }
            Ok(Val::List(out))
        }
        J::Object(m) => {
            if m.len() == 2 {
                if let (Some(J::Str(t)), Some(J::Str(v))) = (m.get("t"), m.get("v")) {
                    if t == "bytes" {
                        return Ok(Val::Bytes(parse_hex_bytes(v)?));
                    }
                }
            }
            let mut out = BTreeMap::new();
            for (k, v) in m.iter() {
                out.insert(k.clone(), val_from_json(v)?);
            }
            Ok(Val::Record(out))
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
        Pat::LitStr(s) => Ok(matches!(v, Val::Text(t) if t == s)),
        Pat::LitBool(b) => Ok(matches!(v, Val::Bool(c) if c == b)),
        Pat::LitNull => Ok(matches!(v, Val::Unit)),
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
            Val::Record(m) => {
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
                    env.set(rn.clone(), Val::Record(rm));
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
        Expr::FloatLit(f) => Ok(Val::Float(*f)),
        Expr::Bool(b) => Ok(Val::Bool(*b)),
        Expr::Str(s) => Ok(Val::Text(s.clone())),
        Expr::StrInterp(parts) => {
            let mut result = String::new();
            for part in parts {
                match part {
                    StrPart::Lit(s) => result.push_str(s),
                    StrPart::Expr(e) => {
                        let v = eval(e, env, tracer, loader)?;
                        let s = match v {
                            Val::Text(s) => s,
                            Val::Int(n) => n.to_string(),
                            Val::Float(f) => f.to_string(),
                            Val::Bool(b) => b.to_string(),
                            other => other.to_json().map(|j| canon_json(&j).unwrap_or_default()).unwrap_or_else(|| "?".to_string()),
                        };
                        result.push_str(&s);
                    }
                }
            }
            Ok(Val::Text(result))
        },
        Expr::Null => Ok(Val::Unit),
        Expr::Var(n) => env.get(n).ok_or_else(|| {
            // Suggest stdlib import if name matches a known module
            let stdlib_modules = [
                "str","list","math","io","json","hash","http","re","map","set",
                "result","option","ffi","witness","process","env","net","trace",
                "float","bigint","bits","path","datetime","uuid","base64","csv",
                "compress","crypto","graph","linalg","type","promise","chan",
                "mutex","ast","eval","cell","grow","flow","cast","rec","record",
            ];
            if stdlib_modules.contains(&n.as_str()) {
                anyhow!("unbound var: {n} -- did you forget to import? Try: import(\"std/{n}\") as {n}")
            } else {
                // Find similar names in env using edit distance
                let env_keys: Vec<String> = env.keys();
                let suggestion = env_keys.iter()
                    .filter(|k| edit_distance(k, n) <= 2 && !k.is_empty())
                    .min_by_key(|k| edit_distance(k, n))
                    .cloned();
                if let Some(s) = suggestion {
                    anyhow!("unbound var: {n} -- did you mean '{s}'?")
                } else {
                    anyhow!("unbound var: {n}")
                }
            }
        }),
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
            Ok(Val::Record(m))
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
                (Val::Record(m), Val::Text(k)) => {
                    m.get(&k).cloned().ok_or_else(|| anyhow!("ERROR_KEY key {:?} not found", k))
                }
                _ => bail!("ERROR_BADARG index operator requires list[int] or rec[str]"),
            }
        }
        Expr::Get(obj, k) => {
            let o = eval(obj, env, tracer, loader)?;
            match &o {
                Val::Record(m) => m
                    .get(k)
                    .cloned()
                    .ok_or_else(|| {
                        // Suggest similar field names
                        let keys: Vec<&String> = m.keys().collect();
                        let suggestion = keys.iter()
                            .filter(|candidate| edit_distance(candidate, k) <= 2)
                            .min_by_key(|candidate| edit_distance(candidate, k))
                            .map(|s| s.as_str());
                        if let Some(s) = suggestion {
                            anyhow!("no member '{k}' -- did you mean '{s}'?")
                        } else {
                            let available: Vec<&str> = keys.iter().map(|s| s.as_str()).take(8).collect();
                            anyhow!("no member '{k}' -- available: {}", available.join(", "))
                        }
                    }),
                // Method-style dispatch: val.method looks up k in the stdlib
                // module for that value type and returns a BoundMethod that
                // prepends the receiver when called. xs.map(f) -> map(xs, f).
                v => {
                    let type_mod = match v {
                        Val::List(_)  => Some("std/list"),
                        Val::Text(_)  => Some("std/str"),
                        Val::Int(_)   => Some("std/int"),
                        Val::Bytes(_) => Some("std/bytes"),
                        Val::Float(_) => Some("std/float"),
                        _             => None,
                    };
                    if let Some(mod_name) = type_mod {
                        let here = loader.root_dir.clone();
                        let m = loader.load_module(mod_name, &here, tracer)
                            .map_err(|e| anyhow!("method dispatch: {mod_name}: {e}"))?;
                        if let Some(f) = m.get(k) {
                            return Ok(Val::BoundMethod(Box::new(o.clone()), Box::new(f.clone())));
                        }
                        bail!("method not found: {k} on type {mod_name}");
                    }
                    bail!("no methods on type: {}", match v {
                        Val::Unit => "unit", Val::Bool(_) => "bool",
                        Val::Int(_) => "int", Val::Float(_) => "float",
                        Val::Text(_) => "text", Val::Bytes(_) => "bytes",
                        Val::List(_) => "list", Val::Func(_) | Val::Builtin(_) => "function",
                        Val::BoundMethod(..) => "bound-method",
                        Val::Err{..} => "err", Val::Record(_) => "record",
                        Val::Chan(..) => "chan",
                        Val::Mtx(..) => "mutex",
                        Val::Big(..) => "bigint",
                        Val::Promise(..) => "promise",
                        Val::VmFunc(_) => "vm-func",
                    })
                }
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
        Expr::NamedCall(f_expr, named_args) => {
            let fv = eval(f_expr, env, tracer, loader)?;
            // Extract param names from function
            let params = match &fv {
                Val::Func(f) => f.params.iter().map(|p| match p {
                    Pat::Bind(n) => n.clone(),
                    _ => "_".to_string(),
                }).collect::<Vec<_>>(),
                _ => bail!("named call on non-function"),
            };
            // Build ordered args
            let mut ordered: Vec<Val> = vec![Val::Unit; params.len()];
            let mut filled = vec![false; params.len()];
            for (name, expr) in named_args {
                let v = eval(expr, env, tracer, loader)?;
                if let Some(i) = params.iter().position(|p| p == name) {
                    ordered[i] = v;
                    filled[i] = true;
                } else {
                    bail!("named arg '{}' not found in function params {:?}", name, params);
                }
            }
            for (i, ok) in filled.iter().enumerate() {
                if !ok { bail!("named arg '{}' not provided", params[i]); }
            }
            call(fv, ordered, tracer, loader)
        }
        Expr::Return(e) => {
            let v = eval(e, env, tracer, loader)?;
            RETURN_VAL.with(|cell| { *cell.borrow_mut() = Some(v); });
            bail!("FARD_EARLY_RETURN");
        }
        Expr::While(init_expr, cond_expr, body_expr) => {
            let mut state = eval(init_expr, env, tracer, loader)?;
            let mut chain: [u8;32] = sha256_raw(b"").try_into().unwrap_or([0u8;32]);
            let mut step_idx: i64 = 0;
            let compact = std::env::var("FARD_COMPACT_WHILE").is_ok();
            if compact {
                let _ = tracer.emit_raw(&format!(r#"{{"t":"while_start"}}"#));
            }
            loop {
                let cv = call(eval(cond_expr, env, tracer, loader)?, vec![state.clone()], tracer, loader)?;
                match cv {
                    Val::Bool(false) => break,
                    Val::Bool(true) => {
                        let before = state.clone();
                        state = call(eval(body_expr, env, tracer, loader)?, vec![before.clone()], tracer, loader)?;
                        let before_j = before.to_json().map(|j| json_to_string(&j)).unwrap_or_else(|| "null".to_string());
                        let after_j  = state.to_json().map(|j| json_to_string(&j)).unwrap_or_else(|| "null".to_string());
                        let pre_hex  = hex_lower(&chain);
                        let args_str = format!("{{\"step\":{},\"before\":{},\"after\":{}}}", step_idx, before_j, after_j);
                        let digest_input = format!("{{\"args\":{},\"op\":\"WHILE_STEP\",\"post\":\"{}\",\"pre\":\"{}\"}}", args_str, after_j, pre_hex);
                        chain = sha256_raw(digest_input.as_bytes()).try_into().unwrap_or([0u8;32]);
                        if compact {
                            let ch16 = &hex_lower(&chain)[..16];
                            let _ = tracer.emit_raw(&format!(r#"{{"t":"while_step","s":{},"h":"{}"}}"#, step_idx, ch16));
                        }
                        step_idx += 1;
                    }
                    _ => bail!("while cond_fn must return bool"),
                }
            }
            if compact {
                let _ = tracer.emit_raw(&format!(r#"{{"t":"while_end","steps":{},"chain_hex":"{}"}}"#, step_idx, hex_lower(&chain)));
            }
            let mut result = BTreeMap::new();
            result.insert("value".to_string(), state);
            result.insert("steps".to_string(), Val::Int(step_idx));
            result.insert("chain_hex".to_string(), Val::Text(hex_lower(&chain)));
            Ok(Val::Record(result))
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
                ("-", Val::Float(f)) => Ok(Val::Float(-f)),
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
                ("/", Val::Int(l), Val::Int(r)) => { if r == 0 { bail!("ERROR_DIV_ZERO division by zero") } Ok(Val::Int(l / r)) }
                ("+", Val::Float(l), Val::Float(r)) => Ok(Val::Float(l + r)),
                ("-", Val::Float(l), Val::Float(r)) => Ok(Val::Float(l - r)),
                ("*", Val::Float(l), Val::Float(r)) => Ok(Val::Float(l * r)),
                ("/", Val::Float(l), Val::Float(r)) => Ok(Val::Float(l / r)),
                ("<", Val::Float(l), Val::Float(r)) => Ok(Val::Bool(l < r)),
                (">", Val::Float(l), Val::Float(r)) => Ok(Val::Bool(l > r)),
                ("<=", Val::Float(l), Val::Float(r)) => Ok(Val::Bool(l <= r)),
                (">=", Val::Float(l), Val::Float(r)) => Ok(Val::Bool(l >= r)),
                ("==", l, r) => {
                    if matches!((&l, &r), (Val::Float(_), _) | (_, Val::Float(_))) {
                        let mut lm = BTreeMap::new();
                        lm.insert("level".to_string(), J::Str("warn".to_string()));
                        lm.insert("msg".to_string(), J::Str("LINT_FLOAT_EQ: == on float values is unreliable; use float.eq instead".to_string()));
                        let _ = tracer.emit(&J::Object(lm));
                    }
                    Ok(Val::Bool(val_eq(&l, &r)))
                }
                ("!=", l, r) => {
                    if matches!((&l, &r), (Val::Float(_), _) | (_, Val::Float(_))) {
                        let mut lm = BTreeMap::new();
                        lm.insert("level".to_string(), J::Str("warn".to_string()));
                        lm.insert("msg".to_string(), J::Str("LINT_FLOAT_EQ: != on float values is unreliable; use float.eq instead".to_string()));
                        let _ = tracer.emit(&J::Object(lm));
                    }
                    Ok(Val::Bool(!val_eq(&l, &r)))
                }
                ("&&", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l && r)),
                ("||", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l || r)),
                ("<", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l < r)),
                (">", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l > r)),
                ("<=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l <= r)),
                (">=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l >= r)),
                ("%", Val::Int(l), Val::Int(r)) => {
                    if r == 0 { bail!("ERROR_DIV_ZERO modulo by zero") }
                    Ok(Val::Int(l % r))
                }
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
        Val::Record(m) => match m.get(RESULT_TAG_KEY) {
            Some(Val::Text(t)) if t == RESULT_OK_TAG => true,
            Some(Val::Text(t)) if t == RESULT_ERR_TAG => true,
            _ => false,
        },
        _ => false,
    }
}



fn fs_sandbox_check(path: &str) -> Result<()> {
    let p = std::path::Path::new(path);
    // Reject absolute paths and any component that is ".."
    if p.is_absolute() {
        bail!("ERROR_SANDBOX fs path must be relative: {}", path);
    }
    for component in p.components() {
        if component == std::path::Component::ParentDir {
            bail!("ERROR_SANDBOX fs path must not contain ..: {}", path);
        }
    }
    Ok(())
}

fn http_response_to_val(resp: ureq::Response) -> Result<Val> {
    let status = resp.status() as i64;
    let body = resp.into_string().unwrap_or_default();
    let mut m = BTreeMap::new();
    m.insert("status".to_string(), Val::Int(status));
    m.insert("body".to_string(), Val::Text(body));
    Ok(Val::Record(m))
}

fn http_response_to_val_with_status(code: u16, resp: ureq::Response) -> Result<Val> {
    let body = resp.into_string().unwrap_or_default();
    let mut m = BTreeMap::new();
    m.insert("status".to_string(), Val::Int(code as i64));
    m.insert("body".to_string(), Val::Text(body));
    Ok(Val::Record(m))
}

fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    // Proleptic Gregorian calendar days since 1970-01-01
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 12 } else { month };
    let d = day;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn unix_secs_to_iso8601(secs: i64) -> String {
    let (secs, _neg) = if secs < 0 { (-secs, true) } else { (secs, false) };
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    // Convert days since epoch to year/month/day
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe/1460 + doe/36524 - doe/146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365*yoe + yoe/4 - yoe/100);
    let mp = (5*doy + 2)/153;
    let d = doy - (153*mp+2)/5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, m, s)
}

fn result_is_ok(v: &Val) -> Result<bool> {
    match v {
        Val::Record(m) => {
            if m.len() != 2 {
                bail!("{} expected result", QMARK_EXPECT_RESULT);
            }
            match m.get(RESULT_TAG_KEY) {
                Some(Val::Text(t)) if t == RESULT_OK_TAG => {
                    if !m.contains_key(RESULT_OK_VAL_KEY) {
                        bail!("QMARK_EXPECT_RESULT ok missing v");
                    }
                    Ok(true)
                }
                Some(Val::Text(t)) if t == RESULT_ERR_TAG => {
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
        Val::Record(m) => match m.get(RESULT_TAG_KEY) {
            Some(Val::Text(t)) if t == RESULT_OK_TAG => m
                .get(RESULT_OK_VAL_KEY)
                .cloned()
                .ok_or_else(|| anyhow!("QMARK_EXPECT_RESULT ok missing v")),
            Some(Val::Text(t)) if t == RESULT_ERR_TAG => {
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
        Val::Record(m) => match m.get(RESULT_TAG_KEY) {
            Some(Val::Text(t)) if t == RESULT_ERR_TAG => match m.get(RESULT_ERR_VAL_KEY) {
                Some(x) => Ok(x.clone()),
                None => bail!("QMARK_EXPECT_RESULT err missing e"),
            },
            Some(Val::Text(t)) if t == RESULT_OK_TAG => {
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
        Val::Text(RESULT_OK_TAG.to_string()),
    );
    m.insert(RESULT_OK_VAL_KEY.to_string(), v);
    Val::Record(m)
}

fn mk_result_err(e: Val) -> Val {
    let mut m = BTreeMap::new();
    m.insert(
        RESULT_TAG_KEY.to_string(),
        Val::Text(RESULT_ERR_TAG.to_string()),
    );
    m.insert(RESULT_ERR_VAL_KEY.to_string(), e);
    Val::Record(m)
}
fn val_eq(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Int(x), Val::Int(y)) => x == y,
        (Val::Float(x), Val::Float(y)) => x == y,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Text(x), Val::Text(y)) => x == y,
        (Val::Unit, Val::Unit) => true,
        (Val::List(xs), Val::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| val_eq(x, y))
        }
        (Val::Record(xm), Val::Record(ym)) => {
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
            Val::VmFunc(fn_idx) => {
                // Use pre-set raw pointer — no clone, no RefCell borrow
                let ptr = VM_FNS_PTR.with(|p| p.get());
                if !ptr.is_null() {
                    // SAFETY: ptr points to VM_FNS vec which lives for the program duration
                    // VM_FNS is never modified during execution, only at load time
                    let fns = unsafe { &*ptr };
                    return vm_exec_fn(fns, fn_idx, cur_args, tracer, loader);
                }
                // Fallback: clone (first call before ptr is set)
                let fns = VM_FNS.with(|cell| cell.borrow().clone());
                return vm_exec_fn(&fns, fn_idx, cur_args, tracer, loader);
            }
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
                CALL_DEPTH.with(|d| *d.borrow_mut() += 1);
                match eval_tco(&fun.body, &mut e, tracer, loader) {
                    Ok(TcoResult::Done(v)) => {
                        CALL_DEPTH.with(|d| *d.borrow_mut() -= 1);
                        return Ok(v);
                    }
                    Ok(TcoResult::TailCall(next_f, next_args)) => {
                        CALL_DEPTH.with(|d| *d.borrow_mut() -= 1);
                        cur_f = next_f;
                        cur_args = next_args;
                        // loop continues
                    }
                    Err(err) => {
                        CALL_DEPTH.with(|d| *d.borrow_mut() -= 1);
                        if let Some(q) = err.downcast_ref::<QMarkUnwind>() {
                            // Convert to err value at function call boundary
                            // This is correct: ? unwinds to nearest call site
                            return Ok(mk_result_err(q.err.clone()));
                        } else if err.to_string() == "FARD_EARLY_RETURN" {
                            return Ok(RETURN_VAL.with(|cell| cell.borrow_mut().take()).unwrap_or(Val::Unit));
                        } else {
                            return Err(err);
                        }
                    }
                }
            }
            Val::BoundMethod(receiver, func) => {
                let mut full_args = vec![*receiver];
                full_args.extend(cur_args);
                cur_f = *func;
                cur_args = full_args;
                // loop continues
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
                Val::VmFunc(_) => Ok(TcoResult::Done(call(fv, av, tracer, loader)?)),
                Val::Builtin(b) => Ok(TcoResult::Done(call_builtin(b, av, tracer, loader)?)),
                Val::BoundMethod(receiver, func) => {
                    let mut full_args = vec![*receiver];
                    full_args.extend(av);
                    Ok(TcoResult::Done(call(*func, full_args, tracer, loader)?))
                }
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

// ═══════════════════════════════════════════════════════════════════════════
// FARD BYTECODE VM
// Compiles Expr trees to flat bytecode + executes in a register VM.
// Eliminates Arc<Mutex<HashMap>> env lookups — uses integer slot indices.
// Falls back to tree-walker for unsupported expressions.
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
enum VmOp {
    LoadInt(i64),
    LoadFloat(f64),
    LoadBool(bool),
    LoadNull,
    LoadStr(String),
    LoadSlot(usize),
    StoreSlot(usize),
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    Neg, Not,
    Jump(usize),
    JumpIfFalse(usize),
    JumpIfTrue(usize),
    Pop,
    MakeList(usize),
    MakeRec(usize),
    GetField(String),
    MakeClosure(usize),   // fn_idx into VmProgram.fns
    VmCall(usize),        // n_args
    Return,
    TryOp,
    Halt,
}

#[derive(Debug, Clone)]
struct VmCompiledFn {
    code: Vec<VmOp>,
    n_slots: usize,
    n_params: usize,
    slot_names: Vec<String>,
    self_slot: Option<usize>,
    free_vars: Vec<(usize, Val)>,  // sparse: (slot_idx, val) for non-param free vars
}

struct VmCompiler {
    code: Vec<VmOp>,
    slot_names: Vec<String>,
}

impl VmCompiler {
    fn new() -> Self { Self { code: Vec::new(), slot_names: Vec::new() } }

    fn slot_of(&mut self, name: &str) -> usize {
        if let Some(i) = self.slot_names.iter().rposition(|s| s == name) { return i; }
        let i = self.slot_names.len();
        self.slot_names.push(name.to_string());
        i
    }

    fn emit(&mut self, op: VmOp) -> usize {
        let i = self.code.len();
        self.code.push(op);
        i
    }

    fn patch(&mut self, idx: usize, target: usize) {
        match &mut self.code[idx] {
            VmOp::Jump(t) | VmOp::JumpIfFalse(t) | VmOp::JumpIfTrue(t) => *t = target,
            _ => {}
        }
    }

    fn compile(&mut self, expr: &Expr, fns: &mut Vec<VmCompiledFn>) -> Result<()> {
        match expr {
            Expr::Int(n)      => { self.emit(VmOp::LoadInt(*n)); }
            Expr::FloatLit(f) => { self.emit(VmOp::LoadFloat(*f)); }
            Expr::Bool(b)     => { self.emit(VmOp::LoadBool(*b)); }
            Expr::Null        => { self.emit(VmOp::LoadNull); }
            Expr::Str(s)      => { self.emit(VmOp::LoadStr(s.clone())); }

            Expr::Var(name) => {
                let slot = self.slot_of(name);
                self.emit(VmOp::LoadSlot(slot));
            }

            Expr::Let(name, e1, e2) => {
                self.compile(e1, fns)?;
                let slot = self.slot_of(name);
                self.emit(VmOp::StoreSlot(slot));
                self.compile(e2, fns)?;
            }

            Expr::If(cond, then_e, else_e) => {
                self.compile(cond, fns)?;
                let jf = self.emit(VmOp::JumpIfFalse(0));
                self.compile(then_e, fns)?;
                let jmp = self.emit(VmOp::Jump(0));
                let else_start = self.code.len();
                self.patch(jf, else_start);
                self.compile(else_e, fns)?;
                self.patch(jmp, self.code.len());
            }

            Expr::Bin(op, a, b) => {
                if op == "&&" {
                    self.compile(a, fns)?;
                    let jf = self.emit(VmOp::JumpIfFalse(0));
                    self.emit(VmOp::Pop);
                    self.compile(b, fns)?;
                    self.patch(jf, self.code.len());
                    return Ok(());
                }
                if op == "||" {
                    self.compile(a, fns)?;
                    let jt = self.emit(VmOp::JumpIfTrue(0));
                    self.emit(VmOp::Pop);
                    self.compile(b, fns)?;
                    self.patch(jt, self.code.len());
                    return Ok(());
                }
                self.compile(a, fns)?;
                self.compile(b, fns)?;
                let bin_op = match op.as_str() {
                    "+"  => VmOp::Add, "-"  => VmOp::Sub, "*"  => VmOp::Mul,
                    "/"  => VmOp::Div, "%"  => VmOp::Mod,
                    "==" => VmOp::Eq,  "!=" => VmOp::Ne,
                    "<"  => VmOp::Lt,  "<=" => VmOp::Le,
                    ">"  => VmOp::Gt,  ">=" => VmOp::Ge,
                    _    => bail!("vm: unsupported binary op: {op}"),
                };
                self.emit(bin_op);
            }

            Expr::Unary(op, e) => {
                self.compile(e, fns)?;
                match op.as_str() {
                    "-" => { self.emit(VmOp::Neg); }
                    "!" => { self.emit(VmOp::Not); }
                    _   => bail!("vm: unsupported unary op"),
                }
            }

            Expr::Get(obj, field) => {
                self.compile(obj, fns)?;
                self.emit(VmOp::GetField(field.clone()));
            }

            Expr::List(items) => {
                for item in items { self.compile(item, fns)?; }
                self.emit(VmOp::MakeList(items.len()));
            }

            Expr::Rec(kvs) => {
                for (k, v) in kvs {
                    self.emit(VmOp::LoadStr(k.clone()));
                    self.compile(v, fns)?;
                }
                self.emit(VmOp::MakeRec(kvs.len()));
            }

            Expr::Call(f_expr, args) => {
                self.compile(f_expr, fns)?;
                for arg in args { self.compile(arg, fns)?; }
                self.emit(VmOp::VmCall(args.len()));
            }

            Expr::Fn(params, body) | Expr::Lambda(params, body) => {
                // Don't VM-compile inline lambdas — they may capture tree-walker locals
                // that are not available in VM slot tables
                bail!("vm: inline Fn/Lambda not supported — use Item::Fn at top level");
                #[allow(unreachable_code)]
                let mut inner = VmCompiler::new();
                for p in params.iter() {
                    match p {
                        Pat::Bind(name) => { inner.slot_of(name); }
                        _ => bail!("vm: fn param must be simple bind"),
                    }
                }
                inner.compile(body, fns)?;
                inner.emit(VmOp::Return);
                let fn_idx = fns.len();
                let n_slots = inner.slot_names.len();
                fns.push(VmCompiledFn {
                    code: inner.code,
                    n_slots,
                    n_params: params.len(),
                    slot_names: inner.slot_names,
                    self_slot: None,
                    free_vars: Vec::new(),
                });
                self.emit(VmOp::MakeClosure(fn_idx));
            }

            Expr::Try(e) => {
                self.compile(e, fns)?;
                self.emit(VmOp::TryOp);
            }

            Expr::Return(e) => {
                self.compile(e, fns)?;
                self.emit(VmOp::Return);
            }

            // Unsupported — fall back to tree-walker
            _ => bail!("vm: unsupported expr"),
        }
        Ok(())
    }
}

fn vm_arith_add(a: Val, b: Val) -> Result<Val> {
    match (a, b) {
        (Val::Int(x), Val::Int(y))     => Ok(Val::Int(x.wrapping_add(y))),
        (Val::Float(x), Val::Float(y)) => Ok(Val::Float(x + y)),
        (Val::Text(x), Val::Text(y))   => Ok(Val::Text(x + &y)),
        _ => bail!("vm: type error in +"),
    }
}
fn vm_arith_sub(a: Val, b: Val) -> Result<Val> {
    match (a, b) {
        (Val::Int(x), Val::Int(y))     => Ok(Val::Int(x.wrapping_sub(y))),
        (Val::Float(x), Val::Float(y)) => Ok(Val::Float(x - y)),
        _ => bail!("vm: type error in -"),
    }
}
fn vm_arith_mul(a: Val, b: Val) -> Result<Val> {
    match (a, b) {
        (Val::Int(x), Val::Int(y))     => Ok(Val::Int(x.wrapping_mul(y))),
        (Val::Float(x), Val::Float(y)) => Ok(Val::Float(x * y)),
        _ => bail!("vm: type error in *"),
    }
}
fn vm_arith_div(a: Val, b: Val) -> Result<Val> {
    match (a, b) {
        (Val::Int(_, ), Val::Int(0))   => bail!("ERROR_DIV_ZERO"),
        (Val::Int(x), Val::Int(y))     => Ok(Val::Int(x / y)),
        (Val::Float(x), Val::Float(y)) => Ok(Val::Float(x / y)),
        _ => bail!("vm: type error in /"),
    }
}
fn vm_arith_mod(a: Val, b: Val) -> Result<Val> {
    match (a, b) {
        (Val::Int(_, ), Val::Int(0)) => bail!("ERROR_DIV_ZERO"),
        (Val::Int(x), Val::Int(y))   => Ok(Val::Int(x % y)),
        _ => bail!("vm: type error in %"),
    }
}
fn vm_val_eq(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Int(x), Val::Int(y))     => x == y,
        (Val::Float(x), Val::Float(y)) => x == y,
        (Val::Bool(x), Val::Bool(y))   => x == y,
        (Val::Text(x), Val::Text(y))   => x == y,
        (Val::Unit, Val::Unit)         => true,
        _ => false,
    }
}
fn vm_val_lt(a: &Val, b: &Val) -> Result<bool> {
    match (a, b) {
        (Val::Int(x), Val::Int(y))     => Ok(x < y),
        (Val::Float(x), Val::Float(y)) => Ok(x < y),
        (Val::Text(x), Val::Text(y))   => Ok(x < y),
        _ => bail!("vm: type error in <"),
    }
}

fn vm_exec_fn(
    fns: &Vec<VmCompiledFn>,
    fn_idx: usize,
    args: Vec<Val>,
    tracer: &mut Tracer,
    loader: &mut ModuleLoader,
) -> Result<Val> {
    let f = &fns[fn_idx];
    if args.len() != f.n_params {
        bail!("arity mismatch: expected {} args, got {}", f.n_params, args.len());
    }
    let f = &fns[fn_idx];
    let n_slots = f.n_slots.max(args.len());
    // Region-based slot allocation
    // For small fns (≤8 slots): use Vec with pre-reserved capacity from arena
    // For larger fns: standard allocation
    let mut slots = if n_slots <= 8 {
        let mut v = Vec::with_capacity(8);
        v.resize(n_slots, Val::Unit);
        v
    } else {
        SLOT_ARENA.with(|arena| {
            let mut a = arena.borrow_mut();
            if let Some(mut frame) = a.pop() {
                frame.resize(n_slots, Val::Unit);
                for v in frame.iter_mut() { *v = Val::Unit; }
                frame
            } else {
                vec![Val::Unit; n_slots]
            }
        })
    };
    // Populate free variables (sparse — only non-param, non-Unit slots)
    for (slot_idx, val) in &f.free_vars {
        if *slot_idx < slots.len() {
            slots[*slot_idx] = val.clone();
        }
    }
    // Params override initial vals
    for (i, a) in args.into_iter().enumerate() { slots[i] = a; }
    // Self-reference slot (cheap — just an index)
    if let Some(self_slot) = f.self_slot {
        if self_slot >= slots.len() { slots.resize(self_slot + 1, Val::Unit); }
        slots[self_slot] = Val::VmFunc(fn_idx);
    }

    let code = fns[fn_idx].code.clone();
    let mut stack: Vec<Val> = Vec::with_capacity(32);
    let mut ip = 0usize;

    loop {
        match &code[ip] {
            VmOp::LoadInt(n)   => { stack.push(Val::Int(*n)); ip += 1; }
            VmOp::LoadFloat(f) => { stack.push(Val::Float(*f)); ip += 1; }
            VmOp::LoadBool(b)  => { stack.push(Val::Bool(*b)); ip += 1; }
            VmOp::LoadNull     => { stack.push(Val::Unit); ip += 1; }
            VmOp::LoadStr(s)   => { stack.push(Val::Text(s.clone())); ip += 1; }

            VmOp::LoadSlot(i) => {
                let v = slots.get(*i).cloned().unwrap_or(Val::Unit);
                stack.push(v); ip += 1;
            }
            VmOp::StoreSlot(i) => {
                let v = stack.pop().unwrap_or(Val::Unit);
                if *i >= slots.len() { slots.resize(*i + 1, Val::Unit); }
                slots[*i] = v; ip += 1;
            }

            VmOp::Add => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_add(a, b)?); ip += 1; }
            VmOp::Sub => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_sub(a, b)?); ip += 1; }
            VmOp::Mul => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_mul(a, b)?); ip += 1; }
            VmOp::Div => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_div(a, b)?); ip += 1; }
            VmOp::Mod => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_mod(a, b)?); ip += 1; }
            VmOp::Eq  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_eq(&a, &b))); ip += 1; }
            VmOp::Ne  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(!vm_val_eq(&a, &b))); ip += 1; }
            VmOp::Lt  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&a, &b)?)); ip += 1; }
            VmOp::Le  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&a, &b)? || vm_val_eq(&a, &b))); ip += 1; }
            VmOp::Gt  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&b, &a)?)); ip += 1; }
            VmOp::Ge  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&b, &a)? || vm_val_eq(&a, &b))); ip += 1; }
            VmOp::Neg => { let a = stack.pop().unwrap(); match a { Val::Int(n) => stack.push(Val::Int(-n)), Val::Float(f) => stack.push(Val::Float(-f)), _ => bail!("vm: neg type error") } ip += 1; }
            VmOp::Not => { let a = stack.pop().unwrap(); match a { Val::Bool(b) => stack.push(Val::Bool(!b)), _ => bail!("vm: not type error") } ip += 1; }

            VmOp::Jump(target)        => { ip = *target; }
            VmOp::JumpIfFalse(target) => {
                match stack.last() {
                    Some(Val::Bool(false)) => { ip = *target; }
                    Some(Val::Bool(true))  => { ip += 1; }
                    _ => bail!("vm: JumpIfFalse requires bool"),
                }
            }
            VmOp::JumpIfTrue(target) => {
                match stack.last() {
                    Some(Val::Bool(true))  => { ip = *target; }
                    Some(Val::Bool(false)) => { ip += 1; }
                    _ => bail!("vm: JumpIfTrue requires bool"),
                }
            }

            VmOp::Pop => { stack.pop(); ip += 1; }

            VmOp::MakeList(n) => {
                let len = stack.len();
                let items: Vec<Val> = stack.drain(len - n ..).collect();
                stack.push(Val::List(items)); ip += 1;
            }

            VmOp::MakeRec(n) => {
                let len = stack.len();
                let pairs: Vec<Val> = stack.drain(len - n * 2 ..).collect();
                let mut m = BTreeMap::new();
                let mut i = 0;
                while i + 1 < pairs.len() {
                    let k = match &pairs[i] { Val::Text(s) => s.clone(), _ => bail!("vm: record key must be text") };
                    m.insert(k, pairs[i+1].clone());
                    i += 2;
                }
                stack.push(Val::Record(m)); ip += 1;
            }

            VmOp::GetField(field) => {
                let obj = stack.pop().unwrap();
                match obj {
                    Val::Record(m) => {
                        let v = m.get(field).cloned().ok_or_else(|| anyhow!("no member '{field}'"))?;
                        stack.push(v);
                    }
                    _ => bail!("vm: GetField on non-record"),
                }
                ip += 1;
            }

            VmOp::MakeClosure(fn_idx) => {
                stack.push(Val::VmFunc(*fn_idx)); ip += 1;
            }

            VmOp::VmCall(n_args) => {
                let len = stack.len();
                let args: Vec<Val> = stack.drain(len - n_args ..).collect();
                let fv = stack.pop().unwrap();
                let result = vm_dispatch_call(fns, fv, args, tracer, loader)?;
                stack.push(result); ip += 1;
            }

            VmOp::TryOp => {
                let v = stack.pop().unwrap();
                match &v {
                    Val::Record(m) => {
                        match m.get("t").and_then(|t| if let Val::Text(s) = t { Some(s.as_str()) } else { None }) {
                            Some("ok")  => { stack.push(m.get("v").cloned().unwrap_or(Val::Unit)); }
                            Some("err") => { return Ok(mk_result_err(m.get("e").cloned().unwrap_or(Val::Unit))); }
                            _ => bail!("vm: ? requires result record with t=ok or t=err"),
                        }
                    }
                    _ => bail!("vm: ? requires result record"),
                }
                ip += 1;
            }

            VmOp::Return => {
                let result = stack.pop().unwrap_or(Val::Unit);
                if slots.capacity() > 8 {
                    SLOT_ARENA.with(|arena| arena.borrow_mut().push(slots));
                }
                return Ok(result);
            }
            VmOp::Halt   => { break; }
        }
    }
    let result = stack.pop().unwrap_or(Val::Unit);
    if slots.capacity() > 8 {
        SLOT_ARENA.with(|arena| arena.borrow_mut().push(slots));
    }
    Ok(result)
}

fn vm_dispatch_call(
    fns: &Vec<VmCompiledFn>,
    fv: Val,
    args: Vec<Val>,
    tracer: &mut Tracer,
    loader: &mut ModuleLoader,
) -> Result<Val> {
    match fv {
        Val::VmFunc(fn_idx) => {
            vm_exec_fn(fns, fn_idx, args, tracer, loader)
        }
        Val::Func(f)        => call(Val::Func(f), args, tracer, loader),
        Val::Builtin(b)     => call_builtin(b, args, tracer, loader),
        Val::BoundMethod(recv, func) => {
            let mut full = vec![*recv];
            full.extend(args);
            vm_dispatch_call(&vec![], *func, full, tracer, loader)
        }
        _ => bail!("vm: call on non-function"),
    }
}

/// Try to compile and run expr in the VM.
/// Returns None if expr uses unsupported features — caller falls back to tree-walker.
fn try_vm_eval(
    expr: &Expr,
    env: &mut Env,
    tracer: &mut Tracer,
    loader: &mut ModuleLoader,
) -> Option<Result<Val>> {
    let mut fns: Vec<VmCompiledFn> = Vec::new();
    let mut compiler = VmCompiler::new();

    // Pre-populate slots from current env so Var lookups work
    let env_keys = env.keys();
    for k in &env_keys {
        compiler.slot_of(k);
    }

    if compiler.compile(expr, &mut fns).is_err() {
        return None;
    }
    compiler.emit(VmOp::Return);

    let n_slots = compiler.slot_names.len();
    let slot_names = compiler.slot_names.clone();

    fns.push(VmCompiledFn {
        code: compiler.code,
        n_slots,
        n_params: 0,
        slot_names: slot_names.clone(),
                    self_slot: None,
                    free_vars: Vec::new(),
                });
    let entry_idx = fns.len() - 1;

    // Build initial slot values from env
    let mut slots = vec![Val::Unit; n_slots];
    for (i, name) in slot_names.iter().enumerate() {
        if let Some(v) = env.get(name) {
            slots[i] = v;
        }
    }

    // Execute: inject pre-loaded slots directly
    let code = fns[entry_idx].code.clone();
    let mut stack: Vec<Val> = Vec::with_capacity(32);
    let mut ip = 0usize;

    let result = (|| {
        loop {
            match &code[ip] {
                VmOp::LoadInt(n)   => { stack.push(Val::Int(*n)); ip += 1; }
                VmOp::LoadFloat(f) => { stack.push(Val::Float(*f)); ip += 1; }
                VmOp::LoadBool(b)  => { stack.push(Val::Bool(*b)); ip += 1; }
                VmOp::LoadNull     => { stack.push(Val::Unit); ip += 1; }
                VmOp::LoadStr(s)   => { stack.push(Val::Text(s.clone())); ip += 1; }

                VmOp::LoadSlot(i) => {
                    stack.push(slots.get(*i).cloned().unwrap_or(Val::Unit)); ip += 1;
                }
                VmOp::StoreSlot(i) => {
                    let v = stack.pop().unwrap_or(Val::Unit);
                    if *i >= slots.len() { slots.resize(*i + 1, Val::Unit); }
                    slots[*i] = v; ip += 1;
                }
                VmOp::Add => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_add(a, b)?); ip += 1; }
                VmOp::Sub => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_sub(a, b)?); ip += 1; }
                VmOp::Mul => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_mul(a, b)?); ip += 1; }
                VmOp::Div => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_div(a, b)?); ip += 1; }
                VmOp::Mod => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(vm_arith_mod(a, b)?); ip += 1; }
                VmOp::Eq  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_eq(&a, &b))); ip += 1; }
                VmOp::Ne  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(!vm_val_eq(&a, &b))); ip += 1; }
                VmOp::Lt  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&a, &b)?)); ip += 1; }
                VmOp::Le  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&a, &b)? || vm_val_eq(&a, &b))); ip += 1; }
                VmOp::Gt  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&b, &a)?)); ip += 1; }
                VmOp::Ge  => { let b = stack.pop().unwrap(); let a = stack.pop().unwrap(); stack.push(Val::Bool(vm_val_lt(&b, &a)? || vm_val_eq(&a, &b))); ip += 1; }
                VmOp::Neg => { let a = stack.pop().unwrap(); match a { Val::Int(n) => stack.push(Val::Int(-n)), Val::Float(f) => stack.push(Val::Float(-f)), _ => bail!("vm: neg type error") } ip += 1; }
                VmOp::Not => { let a = stack.pop().unwrap(); match a { Val::Bool(b) => stack.push(Val::Bool(!b)), _ => bail!("vm: not type error") } ip += 1; }
                VmOp::Jump(target)        => { ip = *target; }
                VmOp::JumpIfFalse(target) => { match stack.last() { Some(Val::Bool(false)) => { ip = *target; } Some(Val::Bool(true)) => { ip += 1; } _ => bail!("vm: JumpIfFalse needs bool") } }
                VmOp::JumpIfTrue(target)  => { match stack.last() { Some(Val::Bool(true))  => { ip = *target; } Some(Val::Bool(false)) => { ip += 1; } _ => bail!("vm: JumpIfTrue needs bool") } }
                VmOp::Pop => { stack.pop(); ip += 1; }
                VmOp::MakeList(n) => { let len = stack.len(); let items: Vec<Val> = stack.drain(len - n ..).collect(); stack.push(Val::List(items)); ip += 1; }
                VmOp::MakeRec(n) => {
                    let len = stack.len(); let pairs: Vec<Val> = stack.drain(len - n * 2 ..).collect();
                    let mut m = BTreeMap::new(); let mut i = 0;
                    while i + 1 < pairs.len() { let k = match &pairs[i] { Val::Text(s) => s.clone(), _ => bail!("vm: rec key must be text") }; m.insert(k, pairs[i+1].clone()); i += 2; }
                    stack.push(Val::Record(m)); ip += 1;
                }
                VmOp::GetField(field) => {
                    let obj = stack.pop().unwrap();
                    match obj { Val::Record(m) => { let v = m.get(field).cloned().ok_or_else(|| anyhow!("no member '{field}'"))?; stack.push(v); } _ => bail!("vm: GetField on non-record") }
                    ip += 1;
                }
                VmOp::MakeClosure(fn_idx) => { stack.push(Val::VmFunc(*fn_idx)); ip += 1; }
                VmOp::VmCall(n_args) => {
                    let len = stack.len(); let args: Vec<Val> = stack.drain(len - n_args ..).collect();
                    let fv = stack.pop().unwrap();
                    let result = vm_dispatch_call(&fns, fv, args, tracer, loader)?;
                    stack.push(result); ip += 1;
                }
                VmOp::TryOp => {
                    let v = stack.pop().unwrap();
                    match &v { Val::Record(m) => { match m.get("t").and_then(|t| if let Val::Text(s) = t { Some(s.as_str()) } else { None }) { Some("ok") => { stack.push(m.get("v").cloned().unwrap_or(Val::Unit)); } Some("err") => { return Ok(mk_result_err(m.get("e").cloned().unwrap_or(Val::Unit))); } _ => bail!("vm: ? needs result") } } _ => bail!("vm: ? needs record") }
                    ip += 1;
                }
                VmOp::Return => { return Ok(stack.pop().unwrap_or(Val::Unit)); }
                VmOp::Halt   => { break; }
            }
        }
        Ok(stack.pop().unwrap_or(Val::Unit))
    })();

    Some(result)
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
            let bs = hex_decode("89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000f494441547801010400fbff00ff0000030101008d1de5820000000049454e44ae426082")?;
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

        // Type checking constructor
        Builtin::TypeCheck(type_name, required_fields) => match args.as_slice() {
            [Val::Record(m)] => {
                let mut missing: Vec<&String> = required_fields.iter()
                    .filter(|f| !m.contains_key(f.as_str()))
                    .collect();
                if !missing.is_empty() {
                    missing.sort();
                    bail!("ERROR_TYPE {}: missing required field(s): {}",
                        type_name, missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
                }
                Ok(Val::Record(m.clone()))
            }
            [other] => bail!("ERROR_TYPE {}: expected a record, got {:?}", type_name, other),
            _ => bail!("ERROR_TYPE {}: constructor takes exactly one argument", type_name),
        },
        // std/math
        Builtin::MathAbs => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Int(n.abs())),
            [Val::Float(f)] => Ok(Val::Float(f.abs())),
            _ => bail!("math.abs expects a number"),
        },
        Builtin::MathMin => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(*a.min(b))),
            [Val::Float(a), Val::Float(b)] => Ok(Val::Float(a.min(*b))),
            _ => bail!("math.min expects two numbers"),
        },
        Builtin::MathMax => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(*a.max(b))),
            [Val::Float(a), Val::Float(b)] => Ok(Val::Float(a.max(*b))),
            _ => bail!("math.max expects two numbers"),
        },
        Builtin::MathPow => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(a.pow(*b as u32))),
            [Val::Float(a), Val::Float(b)] => Ok(Val::Float(a.powf(*b))),
            _ => bail!("math.pow expects two numbers"),
        },
        Builtin::MathSqrt => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Float((*n as f64).sqrt())),
            [Val::Float(f)] => Ok(Val::Float(f.sqrt())),
            _ => bail!("math.sqrt expects a number"),
        },
        Builtin::MathFloor => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Int(*n)),
            [Val::Float(f)] => Ok(Val::Int(f.floor() as i64)),
            _ => bail!("math.floor expects a number"),
        },
        Builtin::MathCeil => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Int(*n)),
            [Val::Float(f)] => Ok(Val::Int(f.ceil() as i64)),
            _ => bail!("math.ceil expects a number"),
        },
        Builtin::MathRound => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Int(*n)),
            [Val::Float(f)] => Ok(Val::Int(f.round() as i64)),
            _ => bail!("math.round expects a number"),
        },
        Builtin::MathLog => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.ln())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).ln())),
            _ => bail!("math.log expects a number"),
        },
        Builtin::MathExp => match args.as_slice() {
            [Val::Float(x)] => Ok(Val::Float(x.exp())),
            [Val::Int(x)]   => Ok(Val::Float((*x as f64).exp())),
            _ => bail!("ERROR_BADARG math.exp expects one numeric arg"),
        }
        Builtin::MathLog2 => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.log2())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).log2())),
            _ => bail!("math.log2 expects a number"),
        },
        Builtin::BitAnd => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(a & b)),
            _ => bail!("bits.band expects (int, int)"),
        },
        Builtin::BitOr => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(a | b)),
            _ => bail!("bits.bor expects (int, int)"),
        },
        Builtin::BitXor => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(a ^ b)),
            _ => bail!("bits.bxor expects (int, int)"),
        },
        Builtin::BitNot => match args.as_slice() {
            [Val::Int(a)] => Ok(Val::Int(!a)),
            _ => bail!("bits.bnot expects int"),
        },
        Builtin::BitShl => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(a << (b & 63))),
            _ => bail!("bits.bshl expects (int, int)"),
        },
        Builtin::BitShr => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(a >> (b & 63))),
            _ => bail!("bits.bshr expects (int, int)"),
        },
        Builtin::BitPopcount => match args.as_slice() {
            [Val::Int(a)] => Ok(Val::Int(a.count_ones() as i64)),
            _ => bail!("bits.popcount expects int"),
        },
        // std/null
        Builtin::NullIsNull => match args.as_slice() {
            [Val::Unit] => Ok(Val::Bool(true)),
            [_] => Ok(Val::Bool(false)),
            _ => bail!("null.isNull expects one argument"),
        },
        Builtin::NullCoalesce => match args.as_slice() {
            [Val::Unit, b] => Ok(b.clone()),
            [a, _] => Ok(a.clone()),
            _ => bail!("null.coalesce expects two arguments"),
        },
        Builtin::NullGuard => match args.as_slice() {
            [Val::Unit] => bail!("ERROR_NULL_GUARD value was null"),
            [a] => Ok(a.clone()),
            _ => bail!("null.guardNotNull expects one argument"),
        },
        // std/path
        Builtin::PathBase => match args.as_slice() {
            [Val::Text(s)] => Ok(Val::Text(
                std::path::Path::new(s.as_str())
                    .file_name().map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )),
            _ => bail!("path.base expects a string"),
        },
        Builtin::PathDir => match args.as_slice() {
            [Val::Text(s)] => Ok(Val::Text(
                std::path::Path::new(s.as_str())
                    .parent().map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".".to_string())
            )),
            _ => bail!("path.dir expects a string"),
        },
        Builtin::PathExt => match args.as_slice() {
            [Val::Text(s)] => Ok(Val::Text(
                std::path::Path::new(s.as_str())
                    .extension().map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default()
            )),
            _ => bail!("path.ext expects a string"),
        },
        Builtin::PathIsAbs => match args.as_slice() {
            [Val::Text(s)] => Ok(Val::Bool(std::path::Path::new(s.as_str()).is_absolute())),
            _ => bail!("path.isAbs expects a string"),
        },
        Builtin::PathJoin => match args.as_slice() {
            [Val::Text(a), Val::Text(b)] => {
                let p = std::path::Path::new(a.as_str()).join(b.as_str());
                Ok(Val::Text(p.to_string_lossy().into_owned()))
            },
            _ => bail!("path.join expects two strings"),
        },
        Builtin::PathJoinAll => match args.as_slice() {
            [Val::List(parts)] => {
                let mut p = std::path::PathBuf::new();
                for part in parts.iter() {
                    if let Val::Text(s) = part { p.push(s.as_str()); }
                    else { bail!("path.joinAll: all elements must be strings"); }
                }
                Ok(Val::Text(p.to_string_lossy().into_owned()))
            },
            _ => bail!("path.joinAll expects a list of strings"),
        },
        Builtin::PathNormalize => match args.as_slice() {
            [Val::Text(s)] => {
                // Simple normalize: resolve . and .. components
                let mut parts: Vec<&str> = Vec::new();
                for c in s.split('/') {
                    match c {
                        "." | "" => {},
                        ".." => { parts.pop(); },
                        other => parts.push(other),
                    }
                }
                let result = if s.starts_with('/') {
                    format!("/{}", parts.join("/"))
                } else {
                    parts.join("/")
                };
                Ok(Val::Text(result))
            },
            _ => bail!("path.normalize expects a string"),
        },
        Builtin::Unimplemented(name) => bail!("ERROR_RUNTIME UNIMPLEMENTED_BUILTIN: {}", name),
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

        Builtin::ResultUnwrapOr => {
            if args.len() != 2 { bail!("ERROR_BADARG result.unwrap_or expects 2 args"); }
            let r = args[0].clone();
            let default = args[1].clone();
            if result_is_ok(&r)? { result_unwrap_ok(&r) } else { Ok(default) }
        }
        Builtin::ResultErr => {
            if args.len() != 1 {
                bail!("ERROR_BADARG result.err expects 1 arg");
            }
            Ok(mk_result_err(args[0].clone()))
        }
        Builtin::ResultIsOk => {
            if args.len() != 1 { bail!("ERROR_BADARG result.is_ok expects 1 arg"); }
            Ok(Val::Bool(result_is_ok(&args[0])?))
        }
        Builtin::ResultIsErr => {
            if args.len() != 1 { bail!("ERROR_BADARG result.is_err expects 1 arg"); }
            Ok(Val::Bool(!result_is_ok(&args[0])?))
        }
        Builtin::ResultMap => {
            if args.len() != 2 { bail!("ERROR_BADARG result.map expects 2 args"); }
            let r = args[0].clone(); let f = args[1].clone();
            if result_is_ok(&r)? {
                let v = result_unwrap_ok(&r)?;
                Ok(mk_result_ok(call(f, vec![v], tracer, loader)?))
            } else { Ok(r) }
        }
        Builtin::ResultMapErr => {
            if args.len() != 2 { bail!("ERROR_BADARG result.map_err expects 2 args"); }
            let r = args[0].clone(); let f = args[1].clone();
            if !result_is_ok(&r)? {
                let e = result_unwrap_err(&r)?;
                Ok(mk_result_err(call(f, vec![e], tracer, loader)?))
            } else { Ok(r) }
        }
        Builtin::ResultOrElse => {
            if args.len() != 2 { bail!("ERROR_BADARG result.or_else expects 2 args"); }
            let r = args[0].clone(); let f = args[1].clone();
            if !result_is_ok(&r)? {
                let e = result_unwrap_err(&r)?;
                Ok(call(f, vec![e], tracer, loader)?)
            } else { Ok(r) }
        }
        // --- std/option ---
        Builtin::OptionNone => Ok(Val::Unit),
        Builtin::OptionSome => {
            if args.len() != 1 { bail!("ERROR_BADARG option.some expects 1 arg"); }
            let mut m = BTreeMap::new();
            m.insert("t".to_string(), Val::Text("some".to_string()));
            m.insert("v".to_string(), args[0].clone());
            Ok(Val::Record(m))
        }
        Builtin::OptionIsNone => {
            if args.len() != 1 { bail!("ERROR_BADARG option.is_none expects 1 arg"); }
            Ok(Val::Bool(matches!(args[0], Val::Unit)))
        }
        Builtin::OptionIsSome => {
            if args.len() != 1 { bail!("ERROR_BADARG option.is_some expects 1 arg"); }
            Ok(Val::Bool(!matches!(args[0], Val::Unit)))
        }
        Builtin::OptionFromNullable => {
            if args.len() != 1 { bail!("ERROR_BADARG option.from_nullable expects 1 arg"); }
            if matches!(args[0], Val::Unit) {
                Ok(Val::Unit)
            } else {
                let mut m = BTreeMap::new();
                m.insert("t".to_string(), Val::Text("some".to_string()));
                m.insert("v".to_string(), args[0].clone());
                Ok(Val::Record(m))
            }
        }
        Builtin::OptionToNullable => {
            if args.len() != 1 { bail!("ERROR_BADARG option.to_nullable expects 1 arg"); }
            match &args[0] {
                Val::Unit => Ok(Val::Unit),
                Val::Record(m) if matches!(m.get("t"), Some(Val::Text(s)) if s == "some") => {
                    Ok(m.get("v").cloned().unwrap_or(Val::Unit))
                }
                _ => Ok(Val::Unit),
            }
        }
        Builtin::OptionMap => {
            if args.len() != 2 { bail!("ERROR_BADARG option.map expects 2 args"); }
            let opt = args[0].clone(); let f = args[1].clone();
            match &opt {
                Val::Unit => Ok(Val::Unit),
                Val::Record(m) if matches!(m.get("t"), Some(Val::Text(s)) if s == "some") => {
                    let v = m.get("v").cloned().unwrap_or(Val::Unit);
                    let result = call(f, vec![v], tracer, loader)?;
                    let mut nm = BTreeMap::new();
                    nm.insert("t".to_string(), Val::Text("some".to_string()));
                    nm.insert("v".to_string(), result);
                    Ok(Val::Record(nm))
                }
                _ => Ok(Val::Unit),
            }
        }
        Builtin::OptionAndThen => {
            if args.len() != 2 { bail!("ERROR_BADARG option.and_then expects 2 args"); }
            let opt = args[0].clone(); let f = args[1].clone();
            match &opt {
                Val::Unit => Ok(Val::Unit),
                Val::Record(m) if matches!(m.get("t"), Some(Val::Text(s)) if s == "some") => {
                    let v = m.get("v").cloned().unwrap_or(Val::Unit);
                    call(f, vec![v], tracer, loader)
                }
                _ => Ok(Val::Unit),
            }
        }
        Builtin::OptionUnwrapOr => {
            if args.len() != 2 { bail!("ERROR_BADARG option.unwrap_or expects 2 args"); }
            let opt = args[0].clone(); let default = args[1].clone();
            match &opt {
                Val::Unit => Ok(default),
                Val::Record(m) if matches!(m.get("t"), Some(Val::Text(s)) if s == "some") => {
                    Ok(m.get("v").cloned().unwrap_or(default))
                }
                _ => Ok(default),
            }
        }
        Builtin::OptionUnwrapOrElse => {
            if args.len() != 2 { bail!("ERROR_BADARG option.unwrap_or_else expects 2 args"); }
            let opt = args[0].clone(); let f = args[1].clone();
            match &opt {
                Val::Unit => call(f, vec![], tracer, loader),
                Val::Record(m) if matches!(m.get("t"), Some(Val::Text(s)) if s == "some") => {
                    Ok(m.get("v").cloned().unwrap_or(Val::Unit))
                }
                _ => call(f, vec![], tracer, loader),
            }
        }

        // --- std/trace ---
        Builtin::TraceInfo => {
            if args.len() != 1 { bail!("ERROR_BADARG trace.info expects 1 arg"); }
            let msg = args[0].to_json().ok_or_else(|| anyhow!("trace.info arg must be jsonable"))?;
            let mut m = std::collections::BTreeMap::new();
            m.insert("level".to_string(), J::Str("info".to_string()));
            m.insert("msg".to_string(), msg);
            tracer.emit(&J::Object(m))?;
            Ok(Val::Unit)
        }
        Builtin::TraceWarn => {
            if args.len() != 1 { bail!("ERROR_BADARG trace.warn expects 1 arg"); }
            let msg = args[0].to_json().ok_or_else(|| anyhow!("trace.warn arg must be jsonable"))?;
            let mut m = std::collections::BTreeMap::new();
            m.insert("level".to_string(), J::Str("warn".to_string()));
            m.insert("msg".to_string(), msg);
            tracer.emit(&J::Object(m))?;
            Ok(Val::Unit)
        }
        Builtin::TraceError => {
            if args.len() != 1 { bail!("ERROR_BADARG trace.error expects 1 arg"); }
            let msg = args[0].to_json().ok_or_else(|| anyhow!("trace.error arg must be jsonable"))?;
            let mut m = std::collections::BTreeMap::new();
            m.insert("level".to_string(), J::Str("error".to_string()));
            m.insert("msg".to_string(), msg);
            tracer.emit(&J::Object(m))?;
            Ok(Val::Unit)
        }
        Builtin::TraceSpan => {
            // span(name, fn) — emits span_start/span_end around fn(), returns fn result
            if args.len() != 2 { bail!("ERROR_BADARG trace.span expects 2 args (name, fn)"); }
            let name = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG trace.span name must be text"),
            };
            let f = args[1].clone();
            // emit span_start
            let mut start = std::collections::BTreeMap::new();
            start.insert("level".to_string(), J::Str("span_start".to_string()));
            start.insert("msg".to_string(), J::Str(name.clone()));
            tracer.emit(&J::Object(start))?;
            // call the function
            let result = call(f, vec![], tracer, loader);
            // emit span_end regardless of success
            let mut end = std::collections::BTreeMap::new();
            end.insert("level".to_string(), J::Str("span_end".to_string()));
            end.insert("msg".to_string(), J::Str(name));
            tracer.emit(&J::Object(end))?;
            result
        }
        Builtin::ListGroupBy => {
            // group_by(xs, key_fn) -> record of { key: [elements] }
            if args.len() != 2 { bail!("ERROR_ARITY list.group_by expects 2 args"); }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG list.group_by arg0 must be list"),
            };
            let f = args[1].clone();
            let mut groups: BTreeMap<String, Vec<Val>> = BTreeMap::new();
            for x in xs {
                let key_val = call(f.clone(), vec![x.clone()], tracer, loader)?;
                let key_str = match &key_val {
                    Val::Text(s) => s.clone(),
                    Val::Bool(b) => b.to_string(),
                    Val::Int(n) => n.to_string(),
                    _ => bail!("ERROR_BADARG list.group_by key_fn must return text, bool, or int"),
                };
                groups.entry(key_str).or_default().push(x);
            }
            let record: BTreeMap<String, Val> = groups.into_iter()
                .map(|(k, v)| (k, Val::List(v)))
                .collect();
            Ok(Val::Record(record))
        }
        // --- std/sembit ---
        Builtin::SembitPartition => {
            if args.len() != 2 { bail!("ERROR_ARITY sembit.partition expects 2 args"); }
            let domain = match &args[0] { Val::List(v) => v.clone(), _ => bail!("ERROR_BADARG sembit.partition domain must be list") };
            let tests  = match &args[1] { Val::List(v) => v.clone(), _ => bail!("ERROR_BADARG sembit.partition tests must be list") };
            let raw_items = domain.len();
            let mut test_ids: Vec<String> = Vec::new();
            let mut test_fns: Vec<Val> = Vec::new();
            let mut tests_preimage = String::new();
            for t in &tests {
                let m = match t { Val::Record(m) => m, _ => bail!("ERROR_BADARG each test must be a record") };
                let id = match m.get("id") { Some(Val::Text(s)) => s.clone(), _ => bail!("ERROR_BADARG test.id must be text") };
                let f  = match m.get("f")  { Some(v) => v.clone(), _ => bail!("ERROR_BADARG test.f missing") };
                tests_preimage.push_str(&id); tests_preimage.push('\n');
                test_ids.push(id); test_fns.push(f);
            }
            let tests_hash = format!("sha256:{}", sha256_bytes_hex(tests_preimage.as_bytes()));
            let mut sig_map: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            let mut domain_preimage = String::new();
            for (i, x) in domain.iter().enumerate() {
                let mut sig_bools: Vec<bool> = Vec::new();
                for f in &test_fns {
                    let r = call(f.clone(), vec![x.clone()], tracer, loader)?;
                    match r { Val::Bool(b) => sig_bools.push(b), _ => bail!("ERROR_BADARG test fn must return bool") };
                }
                let sig: String = sig_bools.iter().map(|b| if *b { '1' } else { '0' }).collect();
                sig_map.entry(sig).or_default().push(i);
                if let Some(j) = x.to_json() { domain_preimage.push_str(&canonical_json_string(&j)); domain_preimage.push('\n'); }
            }
            let quotient_digest = format!("sha256:{}", sha256_bytes_hex(domain_preimage.as_bytes()));
            let classes = sig_map.len();
            let raw_f = raw_items as f64;
            let raw_entropy = if raw_items > 1 { raw_f.log2() } else { 0.0 };
            let mut sem_entropy = 0.0f64;
            let mut singletons = 0usize;
            let mut quotient_list: Vec<Val> = Vec::new();
            for (sig, members) in &sig_map {
                let count = members.len();
                if count == 1 { singletons += 1; }
                let p = count as f64 / raw_f;
                if p > 0.0 { sem_entropy -= p * p.log2(); }
                let examples: Vec<Val> = members.iter().take(3).map(|&i| domain[i].clone()).collect();
                let mut cls = BTreeMap::new();
                cls.insert("examples".to_string(), Val::List(examples));
                cls.insert("members".to_string(), Val::Int(count as i64));
                cls.insert("sig".to_string(), Val::Text(sig.clone()));
                quotient_list.push(Val::Record(cls));
            }
            let saved_bits = raw_entropy - sem_entropy;
            let compression_pct = if raw_entropy > 0.0 { (saved_bits / raw_entropy) * 100.0 } else { 0.0 };
            // emit cert event
            let mut cert = BTreeMap::new();
            cert.insert("classes".to_string(), J::Int(classes as i64));
            cert.insert("level".to_string(), J::Str("info".to_string()));
            cert.insert("meaning_gap".to_string(), J::Int((raw_items - classes) as i64));
            cert.insert("msg".to_string(), J::Str("sembit.partition".to_string()));
            cert.insert("quotient_digest".to_string(), J::Str(quotient_digest.clone()));
            cert.insert("tests_hash".to_string(), J::Str(tests_hash.clone()));
            tracer.emit(&J::Object(cert))?;
            let mut result = BTreeMap::new();
            result.insert("classes".to_string(),            Val::Int(classes as i64));
            result.insert("compression_percent".to_string(),Val::Float(compression_pct));
            result.insert("entropy_bits".to_string(),       Val::Float(sem_entropy));
            result.insert("meaning_gap".to_string(),        Val::Int((raw_items - classes) as i64));
            result.insert("quotient".to_string(),           Val::List(quotient_list));
            result.insert("quotient_digest".to_string(),    Val::Text(quotient_digest));
            result.insert("raw_entropy_bits".to_string(),   Val::Float(raw_entropy));
            result.insert("raw_items".to_string(),          Val::Int(raw_items as i64));
            result.insert("saved_bits".to_string(),         Val::Float(saved_bits));
            result.insert("singletons".to_string(),         Val::Int(singletons as i64));
            result.insert("tests_hash".to_string(),         Val::Text(tests_hash));
            Ok(Val::Record(result))
        }
        // --- std/http ---
        Builtin::HttpGet => {
            // http.get(url) -> {status: int, body: text, headers: record}
            if args.len() != 1 { bail!("ERROR_BADARG http.get expects 1 arg (url)"); }
            let url = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG http.get url must be text"),
            };
            match ureq::get(&url).call() {
                Ok(resp) => Ok(http_response_to_val(resp)?),
                Err(ureq::Error::Status(code, resp)) => Ok(http_response_to_val_with_status(code, resp)?),
                Err(e) => bail!("ERROR_HTTP_GET {}", e),
            }
        }
        Builtin::HttpPost => {
            // http.post(url, body_text) -> {status: int, body: text, headers: record}
            if args.len() != 2 { bail!("ERROR_BADARG http.post expects 2 args (url, body)"); }
            let url = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG http.post url must be text"),
            };
            let body = match &args[1] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG http.post body must be text"),
            };
            match ureq::post(&url).send_string(&body) {
                Ok(resp) => Ok(http_response_to_val(resp)?),
                Err(ureq::Error::Status(code, resp)) => Ok(http_response_to_val_with_status(code, resp)?),
                Err(e) => bail!("ERROR_HTTP_POST {}", e),
            }
        }
        Builtin::HttpRequest => {
            // http.request({method, url, body?, headers?}) -> {status, body, headers}
            if args.len() != 1 { bail!("ERROR_BADARG http.request expects 1 arg (record)"); }
            let rec = match &args[0] {
                Val::Record(m) => m.clone(),
                _ => bail!("ERROR_BADARG http.request expects record"),
            };
            let method = match rec.get("method") {
                Some(Val::Text(s)) => s.to_uppercase(),
                _ => bail!("ERROR_BADARG http.request missing method"),
            };
            let url = match rec.get("url") {
                Some(Val::Text(s)) => s.clone(),
                _ => bail!("ERROR_BADARG http.request missing url"),
            };
            let mut req = ureq::request(&method, &url);
            if let Some(Val::Record(hdrs)) = rec.get("headers") {
                for (k, v) in hdrs {
                    if let Val::Text(vt) = v {
                        req = req.set(k, vt);
                    }
                }
            }
            let result = if let Some(Val::Text(body)) = rec.get("body") {
                req.send_string(body)
            } else {
                req.call()
            };
            match result {
                Ok(resp) => Ok(http_response_to_val(resp)?),
                Err(ureq::Error::Status(code, resp)) => Ok(http_response_to_val_with_status(code, resp)?),
                Err(e) => bail!("ERROR_HTTP_REQUEST {}", e),
            }
        }
        // --- std/time ---
        Builtin::TimeNow => {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            Ok(Val::Int(secs as i64))
        }
        Builtin::TimeParse => {
            if args.len() != 1 { bail!("ERROR_BADARG time.parse expects 1 arg"); }
            match &args[0] {
                Val::Text(s) => {
                    // Parse ISO 8601 UTC: "2024-01-15T12:34:56Z" or with ms "...T12:34:56.123Z"
                    let s = s.trim_end_matches('Z');
                    let (date_part, time_part) = if let Some(t) = s.find('T') {
                        (&s[..t], &s[t+1..])
                    } else {
                        bail!("ERROR_BADARG time.parse invalid ISO 8601: missing T");
                    };
                    let dp: Vec<&str> = date_part.split('-').collect();
                    if dp.len() != 3 { bail!("ERROR_BADARG time.parse invalid date"); }
                    let year: i64 = dp[0].parse().context("year")?;
                    let month: i64 = dp[1].parse().context("month")?;
                    let day: i64 = dp[2].parse().context("day")?;
                    let tp_base = time_part.split('.').next().unwrap_or(time_part);
                    let tp: Vec<&str> = tp_base.split(':').collect();
                    if tp.len() != 3 { bail!("ERROR_BADARG time.parse invalid time"); }
                    let hour: i64 = tp[0].parse().context("hour")?;
                    let min: i64 = tp[1].parse().context("min")?;
                    let sec: i64 = tp[2].parse().context("sec")?;
                    // Days since epoch (simple calculation, no leap seconds)
                    let days = days_since_epoch(year, month, day);
                    let unix_secs = days * 86400 + hour * 3600 + min * 60 + sec;
                    Ok(Val::Int(unix_secs))
                }
                _ => bail!("ERROR_BADARG time.parse expects text"),
            }
        }
        Builtin::TimeFormat => {
            if args.len() != 1 { bail!("ERROR_BADARG time.format expects 1 arg"); }
            match &args[0] {
                Val::Int(secs) => Ok(Val::Text(unix_secs_to_iso8601(*secs))),
                _ => bail!("ERROR_BADARG time.format expects int (unix seconds)"),
            }
        }
        Builtin::TimeAdd => {
            if args.len() != 2 { bail!("ERROR_BADARG time.add expects 2 args (unix_secs, delta_secs)"); }
            match (&args[0], &args[1]) {
                (Val::Int(ts), Val::Int(delta)) => Ok(Val::Int(ts + delta)),
                _ => bail!("ERROR_BADARG time.add expects (int, int)"),
            }
        }
        Builtin::TimeSub => {
            if args.len() != 2 { bail!("ERROR_BADARG time.sub expects 2 args"); }
            match (&args[0], &args[1]) {
                (Val::Int(a), Val::Int(b)) => Ok(Val::Int(a - b)),
                _ => bail!("ERROR_BADARG time.sub expects (int, int)"),
            }
        }
        Builtin::TimeDurationMs => {
            if args.len() != 1 { bail!("ERROR_BADARG time.Duration.ms expects 1 arg"); }
            match &args[0] {
                Val::Int(n) => Ok(Val::Int(n / 1000)),
                _ => bail!("ERROR_BADARG time.Duration.ms expects int"),
            }
        }
        Builtin::TimeDurationSec => {
            if args.len() != 1 { bail!("ERROR_BADARG time.Duration.sec expects 1 arg"); }
            match &args[0] {
                Val::Int(n) => Ok(Val::Int(*n)),
                _ => bail!("ERROR_BADARG time.Duration.sec expects int"),
            }
        }
        Builtin::TimeDurationMin => {
            if args.len() != 1 { bail!("ERROR_BADARG time.Duration.min expects 1 arg"); }
            match &args[0] {
                Val::Int(n) => Ok(Val::Int(n * 60)),
                _ => bail!("ERROR_BADARG time.Duration.min expects int"),
            }
        }
        Builtin::OptionToResult => {
            if args.len() != 2 { bail!("ERROR_BADARG option.to_result expects 2 args"); }
            let opt = args[0].clone(); let err_val = args[1].clone();
            match &opt {
                Val::Unit => Ok(mk_result_err(err_val)),
                Val::Record(m) if matches!(m.get("t"), Some(Val::Text(s)) if s == "some") => {
                    Ok(mk_result_ok(m.get("v").cloned().unwrap_or(Val::Unit)))
                }
                _ => Ok(mk_result_err(err_val)),
            }
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
                Val::Text(s) => Ok(Val::Int(s.len() as i64)),
                _ => bail!("ERROR_RUNTIME type"),
            }
        }
        Builtin::StrConcat => {
            if args.len() != 2 {
                bail!("ERROR_RUNTIME arity");
            }
            let a = match &args[0] {
                Val::Text(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let b = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            Ok(Val::Text(format!("{}{}", a, b)))
        }
        Builtin::MapGet => {
            if args.len() != 2 {
                bail!("ERROR_RUNTIME arity");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            Ok(m.get(k).cloned().unwrap_or(Val::Unit))
        }
        Builtin::MapSet => {
            if args.len() != 3 {
                bail!("ERROR_RUNTIME arity");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_RUNTIME type"),
            };
            let v = args[2].clone();
            let mut out = m.clone();
            out.insert(k.clone(), v);
            Ok(Val::Record(out))
        }
        Builtin::JsonEncode => {
            if args.len() != 1 {
                bail!("ERROR_RUNTIME arity");
            }
            let j = args[0]
                .to_json()
                .ok_or_else(|| anyhow!("ERROR_RUNTIME json encode non-jsonable"))?;
            // Use ASCII-safe encoding: escape non-ASCII as \uXXXX to match
            // Python json.dumps default (ensure_ascii=True)
            let raw = json_to_string(&j);
            let mut escaped = String::with_capacity(raw.len());
            for c in raw.chars() {
                if c.is_ascii() {
                    escaped.push(c);
                } else {
                    let n = c as u32;
                    if n <= 0xFFFF {
                        escaped.push('\\');
                        escaped.push('u');
                        escaped.push_str(&format!("{:04x}", n));
                    } else {
                        let n2 = n - 0x10000;
                        let hi = 0xD800 + (n2 >> 10);
                        let lo = 0xDC00 + (n2 & 0x3FF);
                        escaped.push('\\');
                        escaped.push('u');
                        escaped.push_str(&format!("{:04x}", hi));
                        escaped.push('\\');
                        escaped.push('u');
                        escaped.push_str(&format!("{:04x}", lo));
                    }
                }
            }
            Ok(Val::Text(escaped))
        }
        Builtin::JsonDecode => {
            if args.len() != 1 {
                bail!("ERROR_RUNTIME arity");
            }
            let j: J = match &args[0] {
                Val::Text(ss) => json_from_str(ss)?,
                Val::Bytes(bs) => json_from_slice(bs)?,
                _ => bail!("ERROR_RUNTIME type"),
            };
            val_from_json(&j)
        }

        Builtin::JsonCanonicalize => {
            if args.len() != 1 { bail!("ERROR_RUNTIME arity"); }
            let j = args[0].to_json().ok_or_else(|| anyhow::anyhow!("ERROR_RUNTIME cannot canonicalize"))?;
            let canonical = json_to_string(&j);
            Ok(Val::Text(canonical))
        }
        Builtin::CryptoEd25519Verify => {
            if args.len() != 3 { bail!("ERROR_RUNTIME ed25519_verify expects 3 args"); }
            let pk_hex  = match &args[0] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            let msg_hex = match &args[1] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            let sig_hex = match &args[2] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            let pk_bytes  = hex_decode(&pk_hex)?;
            let msg_bytes = hex_decode(&msg_hex)?;
            let sig_bytes = hex_decode(&sig_hex)?;
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
                Val::Text(ss) => hex_decode(ss)?,
                Val::Bytes(bs) => bs.clone(),
                _ => bail!("ERROR_RUNTIME hmac_sha256 key must be hex str or bytes"),
            };
            let msg_bytes = match &args[1] {
                Val::Text(ss) => ss.as_bytes().to_vec(),
                Val::Bytes(bs) => bs.clone(),
                _ => bail!("ERROR_RUNTIME hmac_sha256 msg must be str or bytes"),
            };
            let result = valuecore::hmac_sha256(&key_bytes, &msg_bytes);
            Ok(Val::Text(hex_lower(&result)))
        }
        Builtin::CodecBase64UrlEncode => {
            if args.len() != 1 { bail!("ERROR_RUNTIME base64url_encode expects 1 arg"); }
            let input = match &args[0] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            Ok(Val::Text(valuecore::base64url::encode(input.as_bytes())))
        }
        Builtin::CodecBase64UrlEncodeHex => {
            // base64url_encode_hex(hex_str) -> base64url of the raw bytes represented by hex
            if args.len() != 1 { bail!("ERROR_RUNTIME base64url_encode_hex expects 1 arg"); }
            let hex_str = match &args[0] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME base64url_encode_hex expects text") };
            let bytes = hex_decode(&hex_str)?;
            Ok(Val::Text(valuecore::base64url::encode(&bytes)))
        }
        Builtin::CodecBase64UrlDecode => {
            if args.len() != 1 { bail!("ERROR_RUNTIME base64url_decode expects 1 arg"); }
            let input = match &args[0] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_RUNTIME type") };
            let bytes = valuecore::base64url::decode(input.as_bytes())
                .map_err(|e| anyhow::anyhow!(e))?;
            Ok(Val::Bytes(bytes))
        }
        Builtin::RandUuidV4 => {
            if args.len() != 0 { bail!("ERROR_RUNTIME rand.uuid_v4 expects 0 args"); }
            Ok(Val::Text(valuecore::uuid::new_v4()))
        }
        Builtin::ListLen => {
            match args.first() {
                Some(Val::List(xs)) => Ok(Val::Int(xs.len() as i64)),
                Some(Val::Text(s)) => Ok(Val::Int(s.chars().count() as i64)),
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
                    Val::Record(m) => match m.get("k") {
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
            Ok(Val::Record(BTreeMap::new()))
        }
        Builtin::RecKeys => {
            if args.len() != 1 {
                bail!("ERROR_BADARG rec.keys expects 1 arg");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.keys arg0 must be record"),
            };
            let mut out: Vec<Val> = Vec::new();
            for k in m.keys() {
                out.push(Val::Text(k.clone()));
            }
            Ok(Val::List(out))
        }
        Builtin::RecValues => {
            if args.len() != 1 {
                bail!("ERROR_BADARG rec.values expects 1 arg");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
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
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.has arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_BADARG rec.has arg1 must be string"),
            };
            Ok(Val::Bool(m.contains_key(k)))
        }
        Builtin::RecGet => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.get expects 2 args");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.get arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_BADARG rec.get arg1 must be string"),
            };
            Ok(m.get(k).cloned().unwrap_or(Val::Unit))
        }
        Builtin::RecGetOr => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.getOr expects 3 args");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.getOr arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
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
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.getOrErr arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
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
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.set arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_BADARG rec.set arg1 must be string"),
            };
            let v = args[2].clone();
            let mut out = m.clone();
            out.insert(k.clone(), v);
            Ok(Val::Record(out))
        }
        Builtin::RecRemove => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.remove expects 2 args");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.remove arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_BADARG rec.remove arg1 must be string"),
            };
            let mut out = m.clone();
            out.remove(k);
            Ok(Val::Record(out))
        }
        Builtin::RecMerge => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.merge expects 2 args");
            }
            let a = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.merge arg0 must be record"),
            };
            let b = match &args[1] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.merge arg1 must be record"),
            };
            let mut out = a.clone();
            for (k, v) in b.iter() {
                out.insert(k.clone(), v.clone());
            }
            Ok(Val::Record(out))
        }
        Builtin::RecSelect => {
            if args.len() != 2 {
                bail!("ERROR_BADARG rec.select expects 2 args");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.select arg0 must be record"),
            };
            let ks = match &args[1] {
                Val::List(v) => v,
                _ => bail!("ERROR_BADARG rec.select arg1 must be list"),
            };
            let mut out: BTreeMap<String, Val> = BTreeMap::new();
            for x in ks.iter() {
                let k = match x {
                    Val::Text(s) => s,
                    _ => bail!("ERROR_BADARG rec.select keys must be strings"),
                };
                if let Some(v) = m.get(k) {
                    out.insert(k.clone(), v.clone());
                }
            }
            Ok(Val::Record(out))
        }
        Builtin::RecRename => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.rename expects 3 args");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.rename arg0 must be record"),
            };
            let a = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_BADARG rec.rename arg1 must be string"),
            };
            let b = match &args[2] {
                Val::Text(s) => s,
                _ => bail!("ERROR_BADARG rec.rename arg2 must be string"),
            };
            let mut out = m.clone();
            if let Some(v) = out.remove(a) {
                out.insert(b.clone(), v);
            }
            Ok(Val::Record(out))
        }
        Builtin::RecUpdate => {
            if args.len() != 3 {
                bail!("ERROR_BADARG rec.update expects 3 args");
            }
            let m = match &args[0] {
                Val::Record(mm) => mm,
                _ => bail!("ERROR_BADARG rec.update arg0 must be record"),
            };
            let k = match &args[1] {
                Val::Text(s) => s,
                _ => bail!("ERROR_BADARG rec.update arg1 must be string"),
            };
            let f = args[2].clone();
            let old = m.get(k).cloned().unwrap_or(Val::Unit);
            let newv = call(f, vec![old], tracer, loader)?;
            let mut out = m.clone();
            out.insert(k.clone(), newv);
            Ok(Val::Record(out))
        }
        Builtin::GrowUnfoldTree => {
            if args.len() < 2 {
                bail!("ERROR_BADARG unfold_tree expects at least 2 args");
            }
            let seed = args[0].clone();
            let depth = match &args[1] {
                Val::Record(m) => match m.get("depth") {
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
                    Val::Record(m) => match m.get("n") {
                        Some(Val::Int(x)) => *x,
                        _ => 0,
                    },
                    _ => 0,
                };
                let c1 = Val::Record({
                    let mut m = BTreeMap::new();
                    m.insert("n".to_string(), Val::Int(n + 1));
                    m
                });
                let c2 = Val::Record({
                    let mut m = BTreeMap::new();
                    m.insert("n".to_string(), Val::Int(n + 2));
                    m
                });
                q.push_back((c1, d + 1));
                q.push_back((c2, d + 1));
            }
            return Ok(Val::Unit);
        }
        Builtin::StrTrim => {
            if args.len() != 1 {
                bail!("ERROR_BADARG str.trim expects 1 arg");
            }
            let s = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG str.trim arg0 must be string"),
            };
            Ok(Val::Text(s.trim().to_string()))
        }
        Builtin::StrToLower => {
            if args.len() != 1 {
                bail!("ERROR_BADARG str.toLower expects 1 arg");
            }
            let s = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG str.toLower arg0 must be string"),
            };
            Ok(Val::Text(s.to_ascii_lowercase()))
        }
        Builtin::IntMul => {
            if args.len() != 2 { bail!("ERROR_BADARG int.mul expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Int(i64_mul(a, b).map_err(|e| anyhow!("{}", e))?))
        }
        Builtin::IntDiv => {
            if args.len() != 2 { bail!("ERROR_BADARG int.div expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Int(i64_div(a, b).map_err(|e| anyhow!("{}", e))?))
        }
        Builtin::IntAbs => {
            match args.first() {
                Some(Val::Int(n)) => Ok(Val::Int(n.abs())),
                _ => bail!("ERROR_BADARG int.abs expects int"),
            }
        }
        Builtin::IntMin => {
            if args.len() != 2 { bail!("ERROR_ARITY int.min"); }
            match (&args[0], &args[1]) {
                (Val::Int(a), Val::Int(b)) => Ok(Val::Int(*a.min(b))),
                _ => bail!("ERROR_BADARG int.min expects int, int"),
            }
        }
        Builtin::IntMax => {
            if args.len() != 2 { bail!("ERROR_ARITY int.max"); }
            match (&args[0], &args[1]) {
                (Val::Int(a), Val::Int(b)) => Ok(Val::Int(*a.max(b))),
                _ => bail!("ERROR_BADARG int.max expects int, int"),
            }
        }
        Builtin::IntToText => {
            match args.first() {
                Some(Val::Int(n)) => Ok(Val::Text(n.to_string())),
                _ => bail!("ERROR_BADARG int.to_text expects int"),
            }
        }
        Builtin::IntFromText => {
            match args.first() {
                Some(Val::Text(s)) => s.trim().parse::<i64>()
                    .map(Val::Int)
                    .map_err(|_| anyhow!("ERROR_PARSE int.from_text: {:?}", s)),
                _ => bail!("ERROR_BADARG int.from_text expects string"),
            }
        }
        Builtin::IntNeg => {
            match args.first() {
                Some(Val::Int(n)) => Ok(Val::Int(-n)),
                _ => bail!("ERROR_BADARG int.neg expects int"),
            }
        }
        Builtin::IntClamp => {
            if args.len() != 3 { bail!("ERROR_ARITY int.clamp"); }
            match (&args[0], &args[1], &args[2]) {
                (Val::Int(x), Val::Int(lo), Val::Int(hi)) => Ok(Val::Int((*x).max(*lo).min(*hi))),
                _ => bail!("ERROR_BADARG int.clamp expects int, int, int"),
            }
        }
        Builtin::IntSub => {
            if args.len() != 2 { bail!("ERROR_BADARG int.sub expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Int(i64_sub(a, b).map_err(|e| anyhow!("{}", e))?))
        }
        Builtin::IntMod => {
            if args.len() != 2 { bail!("ERROR_BADARG int.mod expects 2 args"); }
            let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG type") };
            Ok(Val::Int(i64_rem(a, b).map_err(|e| anyhow!("{}", e))?))
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
            let s = match &args[0] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_BADARG type") };
            let bytes = sha256_raw(s.as_bytes());
            let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
            Ok(Val::Text(format!("sha256:{}", hex)))
        }
        Builtin::WitnessSelfDigest => {
            SELF_DIGEST_ACCESSED.with(|a| *a.borrow_mut() = true);
            let d = SELF_DIGEST.with(|d| d.borrow().clone());
            if d.is_empty() { Ok(Val::Text("sha256:pending".to_string())) } else { Ok(Val::Text(d)) }
        }
        Builtin::WitnessDeps => {
            // Returns list of run_ids this program depends on (via artifact keyword)
            // Read artifact_dep entries from trace.ndjson in out_dir
            // Since we don't have out_dir here, read from WITNESS_DEPS thread-local
            let deps = WITNESS_DEPS.with(|d| d.borrow().clone());
            Ok(Val::List(deps.into_iter().map(Val::Text).collect()))
        }
        Builtin::WitnessVerify => {
            // verify(run_id) -> {t:"ok", v: record} | {t:"err", e: text}
            if args.len() != 1 { bail!("ERROR_BADARG witness.verify expects 1 arg"); }
            let run_id = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG witness.verify expects text run_id"),
            };
            let hex = run_id.strip_prefix("sha256:").unwrap_or(&run_id);
            let receipt_path = format!("receipts/sha256_{}.json", hex);
            // Try local first, then fall back to FARD_REGISTRY_URL if set
            let receipt_bytes_opt: Option<Vec<u8>> = std::fs::read(&receipt_path).ok().or_else(|| {
                if let Ok(base_url) = std::env::var("FARD_REGISTRY_URL") {
                    let url = format!("{}/receipt/{}", base_url.trim_end_matches('/'), run_id);
                    ureq::get(&url).call().ok()
                        .and_then(|r| r.into_string().ok())
                        .map(|s| s.into_bytes())
                } else {
                    None
                }
            });
            match receipt_bytes_opt {
                None => {
                    let mut m = BTreeMap::new();
                    m.insert("e".to_string(), Val::Text(format!("run_id not found: {}", run_id)));
                    m.insert("t".to_string(), Val::Text("err".to_string()));
                    Ok(Val::Record(m))
                }
                Some(bytes) => {
                    match json_from_slice(&bytes) {
                        Err(e) => {
                            let mut m = BTreeMap::new();
                            m.insert("e".to_string(), Val::Text(format!("malformed receipt: {}", e)));
                            m.insert("t".to_string(), Val::Text("err".to_string()));
                            Ok(Val::Record(m))
                        }
                        Ok(receipt) => {
                            // Verify run_id matches
                            let stored = match &receipt {
                                J::Object(rm) => rm.get("run_id")
                                    .and_then(|v| if let J::Str(s) = v { Some(s.clone()) } else { None })
                                    .unwrap_or_default(),
                                _ => String::new(),
                            };
                            if stored != run_id {
                                let mut m = BTreeMap::new();
                                m.insert("e".to_string(), Val::Text(format!("run_id mismatch: stored={}", stored)));
                                m.insert("t".to_string(), Val::Text("err".to_string()));
                                return Ok(Val::Record(m));
                            }
                            let val = jval_to_val(&receipt);
                            let mut m = BTreeMap::new();
                            m.insert("t".to_string(), Val::Text("ok".to_string()));
                            m.insert("v".to_string(), val);
                            Ok(Val::Record(m))
                        }
                    }
                }
            }
        }
        Builtin::WitnessVerifyChain => {
            // verify_chain(run_id) -> {ok: depth} | {err: {run_id, reason}}
            if args.len() != 1 { bail!("ERROR_BADARG witness.verify_chain expects 1 arg"); }
            let root_id = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG verify_chain expects text") };
            // Walk derived_from chain recursively, verify every node
            let mut queue: Vec<String> = vec![root_id.clone()];
            let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut depth: i64 = 0;
            while let Some(run_id) = queue.pop() {
                if visited.contains(&run_id) { continue; }
                visited.insert(run_id.clone());
                let hex = run_id.strip_prefix("sha256:").unwrap_or(&run_id);
                let receipt_path = format!("receipts/sha256_{}.json", hex);
                let bytes_opt: Option<Vec<u8>> = std::fs::read(&receipt_path).ok().or_else(|| {
                    if let Ok(base_url) = std::env::var("FARD_REGISTRY_URL") {
                        let url = format!("{}/receipt/{}", base_url.trim_end_matches('/'), run_id);
                        ureq::get(&url).call().ok()
                            .and_then(|r| r.into_string().ok())
                            .map(|s| s.into_bytes())
                    } else { None }
                });
                let bytes = match bytes_opt {
                    Some(b) => b,
                    None => {
                        let mut em = BTreeMap::new();
                        em.insert("reason".to_string(), Val::Text(format!("receipt not found")));
                        em.insert("run_id".to_string(), Val::Text(run_id));
                        let mut m = BTreeMap::new();
                        m.insert("e".to_string(), Val::Record(em));
                        m.insert("t".to_string(), Val::Text("err".to_string()));
                        return Ok(Val::Record(m));
                    }
                };
                let receipt = match json_from_slice(&bytes) {
                    Ok(r) => r,
                    Err(e) => {
                        let mut em = BTreeMap::new();
                        em.insert("reason".to_string(), Val::Text(format!("malformed receipt: {}", e)));
                        em.insert("run_id".to_string(), Val::Text(run_id));
                        let mut m = BTreeMap::new();
                        m.insert("e".to_string(), Val::Record(em));
                        m.insert("t".to_string(), Val::Text("err".to_string()));
                        return Ok(Val::Record(m));
                    }
                };
                // Verify run_id matches
                let stored = match &receipt {
                    J::Object(rm) => rm.get("run_id").and_then(|v| if let J::Str(s) = v { Some(s.clone()) } else { None }).unwrap_or_default(),
                    _ => String::new(),
                };
                if stored != run_id {
                    let mut em = BTreeMap::new();
                    em.insert("reason".to_string(), Val::Text(format!("run_id mismatch: stored={}", stored)));
                    em.insert("run_id".to_string(), Val::Text(run_id));
                    let mut m = BTreeMap::new();
                    m.insert("e".to_string(), Val::Record(em));
                    m.insert("t".to_string(), Val::Text("err".to_string()));
                    return Ok(Val::Record(m));
                }
                depth += 1;
                // Enqueue derived_from
                if let J::Object(rm) = &receipt {
                    if let Some(J::Array(deps)) = rm.get("derived_from") {
                        for dep in deps {
                            if let J::Str(dep_id) = dep {
                                queue.push(dep_id.clone());
                            }
                        }
                    }
                }
            }
            let mut m = BTreeMap::new();
            m.insert("depth".to_string(), Val::Int(depth));
            m.insert("t".to_string(), Val::Text("ok".to_string()));
            Ok(Val::Record(m))
        }

        Builtin::FfiOpen => {
            if args.len() != 1 { bail!("ERROR_BADARG ffi.open expects 1 arg"); }
            let path = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.open expects text path") };
            let mut m = BTreeMap::new();
            match unsafe { libloading::Library::new(&path) } {
                Ok(lib) => {
                    FFI_LIBS.with(|libs| { libs.borrow_mut().insert(path.clone(), lib); });
                    m.insert("ok".to_string(), Val::Text(path));
                    m.insert("t".to_string(), Val::Text("ok".to_string()));
                }
                Err(e) => {
                    m.insert("e".to_string(), Val::Text(format!("{}", e)));
                    m.insert("t".to_string(), Val::Text("err".to_string()));
                }
            }
            Ok(Val::Record(m))
        }
        Builtin::FfiCall => {
            if args.len() != 3 { bail!("ERROR_BADARG ffi.call expects 3 args"); }
            // Emit oracle boundary warning — ffi.call is non-deterministic
            let _ = tracer.emit_raw(&format!(
                r#"{{"t":"ffi_oracle","symbol":"{}","boundary":"non-deterministic"}}"#,
                match args.get(1) { Some(Val::Text(s)) => s.as_str(), _ => "unknown" }
            ));
            let handle = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call: handle must be text") };
            let symbol = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call: symbol must be text") };
            let ffi_args = match &args[2] { Val::List(l) => l.clone(), _ => bail!("ERROR_BADARG ffi.call: args must be list") };
            let mut m = BTreeMap::new();
            // Mixed args: Int -> i64, Bool -> 0/1, Text -> pointer to leaked CString
            let mut _cstrings: Vec<std::ffi::CString> = Vec::new();
            let int_args: anyhow::Result<Vec<i64>> = ffi_args.iter().map(|v| match v {
                Val::Int(n) => Ok(*n),
                Val::Bool(b) => Ok(if *b { 1 } else { 0 }),
                Val::Text(s) => {
                    let cs = std::ffi::CString::new(s.as_str()).unwrap_or_default();
                    let ptr = cs.as_ptr() as i64;
                    _cstrings.push(cs);
                    Ok(ptr)
                }
                _ => Err(anyhow::anyhow!("ERROR_FFI unsupported arg type")),
            }).collect();
            match int_args {
                Err(e) => { m.insert("e".to_string(), Val::Text(format!("{}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                Ok(iargs) => {
                    let result: anyhow::Result<i64> = FFI_LIBS.with(|libs| {
                        let libs = libs.borrow();
                        let lib = libs.get(&handle).ok_or_else(|| anyhow::anyhow!("ERROR_FFI handle not found: {}", handle))?;
                        unsafe {
                            match iargs.len() {
                                0 => { let f: libloading::Symbol<unsafe extern "C" fn() -> i64> = lib.get(symbol.as_bytes())?; Ok(f()) }
                                1 => { let f: libloading::Symbol<unsafe extern "C" fn(i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0])) }
                                2 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1])) }
                                3 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2])) }
                                4 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3])) }
                                5 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4])) }
                                6 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4],iargs[5])) }
                                7 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4],iargs[5],iargs[6])) }
                                8 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4],iargs[5],iargs[6],iargs[7])) }
                                _ => Err(anyhow::anyhow!("ERROR_FFI max 8 args in v1")),
                            }
                        }
                    });
                    match result {
                        Ok(n) => { m.insert("ok".to_string(), Val::Int(n)); m.insert("t".to_string(), Val::Text("ok".to_string())); }
                        Err(e) => { m.insert("e".to_string(), Val::Text(format!("{}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                    }
                }
            }
            Ok(Val::Record(m))
        }
        Builtin::FfiClose => {
            if args.len() != 1 { bail!("ERROR_BADARG ffi.close expects 1 arg"); }
            let handle = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.close expects text") };
            FFI_LIBS.with(|libs| { libs.borrow_mut().remove(&handle); });
            Ok(Val::Unit)
        }
        Builtin::FfiCallPure => {
            // call_pure: same as call but records sym+args+result in trace for witness hashing
            if args.len() != 3 { bail!("ERROR_BADARG ffi.call_pure expects 3 args"); }
            let handle = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call_pure: handle must be text") };
            let symbol = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call_pure: symbol must be text") };
            let ffi_args = match &args[2] { Val::List(l) => l.clone(), _ => bail!("ERROR_BADARG ffi.call_pure: args must be list") };
            let mut m = BTreeMap::new();
            let int_args: anyhow::Result<Vec<i64>> = ffi_args.iter().map(|v| match v {
                Val::Int(n) => Ok(*n),
                Val::Bool(b) => Ok(if *b { 1 } else { 0 }),
                _ => Err(anyhow::anyhow!("ERROR_FFI call_pure: only int/bool args supported")),
            }).collect();
            match int_args {
                Err(e) => {
                    m.insert("e".to_string(), Val::Text(format!("{}", e)));
                    m.insert("t".to_string(), Val::Text("err".to_string()));
                }
                Ok(iargs) => {
                    let result: anyhow::Result<i64> = FFI_LIBS.with(|libs| {
                        let libs = libs.borrow();
                        let lib = libs.get(&handle).ok_or_else(|| anyhow::anyhow!("ERROR_FFI handle not found: {}", handle))?;
                        unsafe {
                            match iargs.len() {
                                0 => { let f: libloading::Symbol<unsafe extern "C" fn() -> i64> = lib.get(symbol.as_bytes())?; Ok(f()) }
                                1 => { let f: libloading::Symbol<unsafe extern "C" fn(i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0])) }
                                2 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1])) }
                                3 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2])) }
                                _ => Err(anyhow::anyhow!("ERROR_FFI max 3 args")),
                            }
                        }
                    });
                    match result {
                        Ok(n) => {
                            m.insert("ok".to_string(), Val::Int(n));
                            m.insert("t".to_string(), Val::Text("ok".to_string()));
                        }
                        Err(e) => {
                            m.insert("e".to_string(), Val::Text(format!("{}", e)));
                            m.insert("t".to_string(), Val::Text("err".to_string()));
                        }
                    }
                }
            }
            Ok(Val::Record(m))
        }


        Builtin::FfiCallChecked => {
            // call_checked: call fn twice, assert identical outputs (determinism check)
            // If deterministic — include in witness chain. If not — return error with evidence.
            if args.len() < 3 || args.len() > 4 {
                bail!("ERROR_BADARG ffi.call_checked expects 3-4 args: (handle, symbol, args[, reason])");
            }
            let handle = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call_checked: handle must be text") };
            let symbol = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call_checked: symbol must be text") };
            let ffi_args = match &args[2] { Val::List(l) => l.clone(), _ => bail!("ERROR_BADARG ffi.call_checked: args must be list") };
            let reason = match args.get(3) {
                Some(Val::Text(s)) => s.clone(),
                _ => "no reason provided".to_string(),
            };

            // For Text args, we keep the string alive and pass ptr+len
            let mut _text_keep: Vec<String> = Vec::new();
            let int_args: anyhow::Result<Vec<i64>> = {
                let mut out: Vec<i64> = Vec::new();
                for v in &ffi_args {
                    match v {
                        Val::Int(n) => out.push(*n),
                        Val::Bool(b) => out.push(if *b { 1 } else { 0 }),
                        Val::Text(s) => {
                            // Pass text as pointer then length
                            _text_keep.push(s.clone());
                            let kept = _text_keep.last().unwrap();
                            out.push(kept.as_ptr() as i64);
                            out.push(kept.len() as i64);
                        }
                        Val::List(l) if l.is_empty() => {
                            // Empty list — pass null ptr + 0 length
                            out.push(0i64);
                            out.push(0i64);
                        }
                        _ => return Err(anyhow::anyhow!("ERROR_FFI call_checked: unsupported arg type")),
                    }
                }
                Ok(out)
            };

            let iargs = match int_args {
                Err(e) => {
                    let mut m = BTreeMap::new();
                    m.insert("t".to_string(), Val::Text("err".to_string()));
                    m.insert("e".to_string(), Val::Text(format!("{}", e)));
                    return Ok(Val::Record(m));
                }
                Ok(v) => v,
            };

            // Helper closure to call the fn once
            let call_once = |iargs: &[i64]| -> anyhow::Result<i64> {
                FFI_LIBS.with(|libs| {
                    let libs = libs.borrow();
                    let lib = libs.get(&handle).ok_or_else(|| anyhow::anyhow!("ERROR_FFI handle not found: {}", handle))?;
                    unsafe {
                        match iargs.len() {
                            0 => { let f: libloading::Symbol<unsafe extern "C" fn() -> i64> = lib.get(symbol.as_bytes())?; Ok(f()) }
                            1 => { let f: libloading::Symbol<unsafe extern "C" fn(i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0])) }
                            2 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1])) }
                            3 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2])) }
                            4 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3])) }
                            5 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4])) }
                            6 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4],iargs[5])) }
                            7 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4],iargs[5],iargs[6])) }
                            8 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64,i64,i64,i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; Ok(f(iargs[0],iargs[1],iargs[2],iargs[3],iargs[4],iargs[5],iargs[6],iargs[7])) }
                            _ => Err(anyhow::anyhow!("ERROR_FFI max 8 args")),
                        }
                    }
                })
            };

            // Call twice
            let r1 = call_once(&iargs);
            let r2 = call_once(&iargs);

            let mut m = BTreeMap::new();
            match (r1, r2) {
                (Ok(v1), Ok(v2)) => {
                    if v1 == v2 {
                        // Deterministic — emit checked event and include in witness
                        let _ = tracer.emit_raw(&format!(
                            r#"{{"t":"ffi_checked","symbol":"{}","result":{},"reason":"{}","deterministic":true}}"#,
                            symbol, v1, reason.replace('"', "\\\"")
                        ));
                        m.insert("t".to_string(), Val::Text("ok".to_string()));
                        m.insert("ok".to_string(), Val::Int(v1));
                        m.insert("checked".to_string(), Val::Bool(true));
                        m.insert("deterministic".to_string(), Val::Bool(true));
                    } else {
                        // Non-deterministic — reject with evidence
                        let _ = tracer.emit_raw(&format!(
                            r#"{{"t":"ffi_checked","symbol":"{}","deterministic":false,"run1":{},"run2":{}}}"#,
                            symbol, v1, v2
                        ));
                        m.insert("t".to_string(), Val::Text("err".to_string()));
                        m.insert("e".to_string(), Val::Text(format!(
                            "ERROR_FFI call_checked: non-deterministic result for {}: run1={} run2={}",
                            symbol, v1, v2
                        )));
                        m.insert("run1".to_string(), Val::Int(v1));
                        m.insert("run2".to_string(), Val::Int(v2));
                        m.insert("deterministic".to_string(), Val::Bool(false));
                    }
                }
                (Err(e), _) | (_, Err(e)) => {
                    m.insert("t".to_string(), Val::Text("err".to_string()));
                    m.insert("e".to_string(), Val::Text(format!("{}", e)));
                }
            }
            Ok(Val::Record(m))
        }
        Builtin::FfiCallStr => {
            // call_str: calls a C function that returns char* (as i64 pointer), converts to FARD text
            if args.len() != 3 { bail!("ERROR_BADARG ffi.call_str expects 3 args"); }
            let handle = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call_str: handle must be text") };
            let symbol = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ffi.call_str: symbol must be text") };
            let ffi_args = match &args[2] { Val::List(l) => l.clone(), _ => bail!("ERROR_BADARG ffi.call_str: args must be list") };
            let mut m = BTreeMap::new();
            let mut _cstrings: Vec<std::ffi::CString> = Vec::new();
            let int_args: anyhow::Result<Vec<i64>> = ffi_args.iter().map(|v| match v {
                Val::Int(n) => Ok(*n),
                Val::Bool(b) => Ok(if *b { 1 } else { 0 }),
                Val::Text(s) => {
                    let cs = std::ffi::CString::new(s.as_str()).unwrap_or_default();
                    let ptr = cs.as_ptr() as i64;
                    _cstrings.push(cs);
                    Ok(ptr)
                }
                _ => Err(anyhow::anyhow!("ERROR_FFI call_str: unsupported arg")),
            }).collect();
            match int_args {
                Err(e) => { m.insert("e".to_string(), Val::Text(format!("{}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                Ok(iargs) => {
                    let result: anyhow::Result<String> = FFI_LIBS.with(|libs| {
                        let libs = libs.borrow();
                        let lib = libs.get(&handle).ok_or_else(|| anyhow::anyhow!("ERROR_FFI handle not found: {}", handle))?;
                        unsafe {
                            let ptr: i64 = match iargs.len() {
                                0 => { let f: libloading::Symbol<unsafe extern "C" fn() -> i64> = lib.get(symbol.as_bytes())?; f() }
                                1 => { let f: libloading::Symbol<unsafe extern "C" fn(i64) -> i64> = lib.get(symbol.as_bytes())?; f(iargs[0]) }
                                2 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64) -> i64> = lib.get(symbol.as_bytes())?; f(iargs[0],iargs[1]) }
                                3 => { let f: libloading::Symbol<unsafe extern "C" fn(i64,i64,i64) -> i64> = lib.get(symbol.as_bytes())?; f(iargs[0],iargs[1],iargs[2]) }
                                _ => bail!("ERROR_FFI max 3 args"),
                            };
                            if ptr == 0 { return Ok(String::new()); }
                            let cstr = std::ffi::CStr::from_ptr(ptr as *const std::os::raw::c_char);
                            Ok(cstr.to_string_lossy().to_string())
                        }
                    });
                    match result {
                        Ok(s) => { m.insert("ok".to_string(), Val::Text(s)); m.insert("t".to_string(), Val::Text("ok".to_string())); }
                        Err(e) => { m.insert("e".to_string(), Val::Text(format!("{}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                    }
                }
            }
            Ok(Val::Record(m))
        }
        Builtin::NetServe => {
            // net.serve(port, handler_fn) -> blocking server
            // handler_fn receives {method, path, headers, body} -> {status, headers, body}
            if args.len() != 2 { bail!("ERROR_BADARG net.serve expects 2 args: port, handler"); }
            let port = match &args[0] { Val::Int(n) => *n as u16, _ => bail!("ERROR_BADARG net.serve: port must be int") };
            let handler = args[1].clone();
            let addr = format!("0.0.0.0:{}", port);
            let server = tiny_http::Server::http(&addr)
                .map_err(|e| anyhow!("ERROR_NET net.serve failed to bind {}: {}", addr, e))?;
            eprintln!("[fard] net.serve listening on http://{}", addr);
            for raw_req in server.incoming_requests() {
                let method = raw_req.method().as_str().to_string();
                let url = raw_req.url().to_string();
                let mut header_map = BTreeMap::new();
                for h in raw_req.headers() {
                    header_map.insert(
                        h.field.as_str().as_str().to_lowercase(),
                        Val::Text(h.value.as_str().to_string()),
                    );
                }
                let mut body_bytes = Vec::new();
                let mut req_body = raw_req;
                std::io::Read::read_to_end(req_body.as_reader(), &mut body_bytes).ok();
                let body_str = String::from_utf8_lossy(&body_bytes).to_string();
                // Build request record
                let mut req_rec = BTreeMap::new();
                req_rec.insert("method".to_string(), Val::Text(method));
                req_rec.insert("path".to_string(), Val::Text(url));
                req_rec.insert("headers".to_string(), Val::Record(header_map));
                req_rec.insert("body".to_string(), Val::Text(body_str));
                let req_val = Val::Record(req_rec);
                // Call handler
                let resp_val = match call(handler.clone(), vec![req_val], tracer, loader) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("[fard] net.serve handler error: {}", e);
                        let mut m = BTreeMap::new();
                        m.insert("status".to_string(), Val::Int(500));
                        m.insert("body".to_string(), Val::Text(format!("handler error: {}", e)));
                        m.insert("headers".to_string(), Val::Record(BTreeMap::new()));
                        Val::Record(m)
                    }
                };
                // Extract response fields
                let status = match &resp_val { Val::Record(m) => match m.get("status") { Some(Val::Int(n)) => *n as u32, _ => 200 }, _ => 200 };
                let body_text = match &resp_val { Val::Record(m) => match m.get("body") { Some(Val::Text(s)) => s.clone(), Some(v) => format!("{:?}", v), _ => String::new() }, _ => String::new() };
                let content_type = match &resp_val {
                    Val::Record(m) => match m.get("headers") {
                        Some(Val::Record(hm)) => match hm.get("content-type") {
                            Some(Val::Text(ct)) => ct.clone(),
                            _ => "text/plain".to_string(),
                        },
                        _ => "text/plain".to_string(),
                    },
                    _ => "text/plain".to_string(),
                };
                let response = tiny_http::Response::from_string(body_text)
                    .with_status_code(status)
                    .with_header(tiny_http::Header::from_bytes(b"Content-Type", content_type.as_bytes()).unwrap());
                req_body.respond(response).ok();
            }
            Ok(Val::Unit)
        }
        Builtin::NetRespond => {
            Ok(Val::Unit)
        }

        Builtin::CryptoSha512 => {
            if args.len() != 1 { bail!("ERROR_BADARG crypto.sha512 expects 1 arg"); }
            let bytes = match &args[0] {
                Val::Bytes(b) => b.clone(),
                Val::Text(s) => s.as_bytes().to_vec(),
                _ => bail!("ERROR_BADARG crypto.sha512 expects bytes or text"),
            };
            let mut h = sha2::Sha512::new();
            h.update(&bytes);
            let result = h.finalize();
            let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
            Ok(Val::Text(format!("sha512:{}", hex)))
        }
        Builtin::CryptoAesEncrypt => {
            if args.len() != 3 { bail!("ERROR_BADARG crypto.aes_encrypt expects 3 args: key_hex, nonce_hex, plaintext"); }
            let key_hex = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG key must be text hex") };
            let nonce_hex = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG nonce must be text hex") };
            let plaintext = match &args[2] { Val::Text(s) => s.as_bytes().to_vec(), Val::Bytes(b) => b.clone(), _ => bail!("ERROR_BADARG plaintext must be text or bytes") };
            let mut m = BTreeMap::new();
            let key_bytes = hex_decode(&key_hex);
            let nonce_bytes = hex_decode(&nonce_hex);
            match (key_bytes, nonce_bytes) {
                (Ok(k), Ok(n)) if k.len() == 32 && n.len() == 12 => {
                    use aes_gcm::{Aes256Gcm, KeyInit, aead::{Aead, Nonce}};
                    let cipher = Aes256Gcm::new_from_slice(&k).map_err(|e| anyhow!("ERROR_CRYPTO key error: {}", e))?;
                    let nonce = Nonce::<Aes256Gcm>::from_slice(&n);
                    match cipher.encrypt(nonce, plaintext.as_ref()) {
                        Ok(ciphertext) => {
                            let hex: String = ciphertext.iter().map(|b| format!("{:02x}", b)).collect();
                            m.insert("ok".to_string(), Val::Text(hex));
                            m.insert("t".to_string(), Val::Text("ok".to_string()));
                        }
                        Err(e) => { m.insert("e".to_string(), Val::Text(format!("{}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                    }
                }
                (Ok(k), Ok(_)) => { m.insert("e".to_string(), Val::Text(format!("key must be 32 bytes (got {}), nonce must be 12 bytes", k.len()))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                (Err(e), _) | (_, Err(e)) => { m.insert("e".to_string(), Val::Text(format!("hex decode error: {}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
            }
            Ok(Val::Record(m))
        }
        Builtin::CryptoAesDecrypt => {
            if args.len() != 3 { bail!("ERROR_BADARG crypto.aes_decrypt expects 3 args: key_hex, nonce_hex, ciphertext_hex"); }
            let key_hex = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG key must be text hex") };
            let nonce_hex = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG nonce must be text hex") };
            let ct_hex = match &args[2] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG ciphertext must be text hex") };
            let mut m = BTreeMap::new();
            let key_bytes = hex_decode(&key_hex);
            let nonce_bytes = hex_decode(&nonce_hex);
            let ct_bytes = hex_decode(&ct_hex);
            match (key_bytes, nonce_bytes, ct_bytes) {
                (Ok(k), Ok(n), Ok(ct)) if k.len() == 32 && n.len() == 12 => {
                    use aes_gcm::{Aes256Gcm, KeyInit, aead::{Aead, Nonce}};
                    let cipher = Aes256Gcm::new_from_slice(&k).map_err(|e| anyhow!("ERROR_CRYPTO key error: {}", e))?;
                    let nonce = Nonce::<Aes256Gcm>::from_slice(&n);
                    match cipher.decrypt(nonce, ct.as_ref()) {
                        Ok(plaintext) => {
                            let text = String::from_utf8_lossy(&plaintext).to_string();
                            m.insert("ok".to_string(), Val::Text(text));
                            m.insert("t".to_string(), Val::Text("ok".to_string()));
                        }
                        Err(e) => { m.insert("e".to_string(), Val::Text(format!("{}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                    }
                }
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => { m.insert("e".to_string(), Val::Text(format!("hex decode error: {}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                _ => { m.insert("e".to_string(), Val::Text("key must be 32 bytes, nonce 12 bytes".to_string())); m.insert("t".to_string(), Val::Text("err".to_string())); }
            }
            Ok(Val::Record(m))
        }
        Builtin::CryptoMerkleRoot => {
            if args.len() != 1 { bail!("ERROR_BADARG crypto.merkle_root expects 1 arg: list of hex strings"); }
            let items = match &args[0] { Val::List(l) => l.clone(), _ => bail!("ERROR_BADARG merkle_root expects list") };
            if items.is_empty() {
                return Ok(Val::Text("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()));
            }
            let mut layer: Vec<Vec<u8>> = items.iter().map(|v| {
                let hex = match v { Val::Text(s) => s.trim_start_matches("sha256:").to_string(), _ => format!("{:?}", v) };
                hex_decode(&hex).unwrap_or_else(|_| sha256_raw(hex.as_bytes()))
            }).collect();
            while layer.len() > 1 {
                let mut next = Vec::new();
                let mut i = 0;
                while i < layer.len() {
                    let left = &layer[i];
                    let right = if i + 1 < layer.len() { &layer[i+1] } else { &layer[i] };
                    let mut combined = left.clone();
                    combined.extend_from_slice(right);
                    next.push(sha256_raw(&combined));
                    i += 2;
                }
                layer = next;
            }
            let hex: String = layer[0].iter().map(|b| format!("{:02x}", b)).collect();
            Ok(Val::Text(format!("sha256:{}", hex)))
        }
        Builtin::CompressGzip => {
            if args.len() != 1 { bail!("ERROR_BADARG compress.gzip expects 1 arg"); }
            let data = match &args[0] {
                Val::Text(s) => s.as_bytes().to_vec(),
                Val::Bytes(b) => b.clone(),
                _ => bail!("ERROR_BADARG compress.gzip expects text or bytes"),
            };
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data).map_err(|e| anyhow!("ERROR_COMPRESS gzip: {}", e))?;
            let compressed = encoder.finish().map_err(|e| anyhow!("ERROR_COMPRESS gzip finish: {}", e))?;
            let hex: String = compressed.iter().map(|b| format!("{:02x}", b)).collect();
            Ok(Val::Text(hex))
        }
        Builtin::CompressGunzip => {
            if args.len() != 1 { bail!("ERROR_BADARG compress.gunzip expects 1 arg"); }
            let hex = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG compress.gunzip expects text hex") };
            let mut m = BTreeMap::new();
            match hex_decode(&hex) {
                Err(e) => { m.insert("e".to_string(), Val::Text(format!("hex decode: {}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                Ok(compressed) => {
                    use flate2::read::GzDecoder;
                    use std::io::Read;
                    let mut decoder = GzDecoder::new(std::io::Cursor::new(compressed));
                    let mut out = Vec::new();
                    match decoder.read_to_end(&mut out) {
                        Ok(_) => {
                            let text = String::from_utf8_lossy(&out).to_string();
                            m.insert("ok".to_string(), Val::Text(text));
                            m.insert("t".to_string(), Val::Text("ok".to_string()));
                        }
                        Err(e) => { m.insert("e".to_string(), Val::Text(format!("gunzip: {}", e))); m.insert("t".to_string(), Val::Text("err".to_string())); }
                    }
                }
            }
            Ok(Val::Record(m))
        }

        Builtin::GraphOf => {
            // graph.of(run_id) -> {ok: {nodes, edges}} | {err: text}
            if args.len() != 1 { bail!("ERROR_BADARG graph.of expects 1 arg"); }
            let root_id = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG graph.of expects text") };
            let mut queue: Vec<String> = vec![root_id.clone()];
            let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut nodes: Vec<Val> = Vec::new();
            let mut edges: Vec<Val> = Vec::new();
            let mut err_m = BTreeMap::new();
            let mut had_err = false;
            while let Some(run_id) = queue.pop() {
                if visited.contains(&run_id) { continue; }
                visited.insert(run_id.clone());
                let hex = run_id.strip_prefix("sha256:").unwrap_or(&run_id);
                let receipt_path = format!("receipts/sha256_{}.json", hex);
                let bytes = match std::fs::read(&receipt_path) {
                    Ok(b) => b,
                    Err(_) => { err_m.insert("e".to_string(), Val::Text(format!("receipt not found: {}", run_id))); err_m.insert("t".to_string(), Val::Text("err".to_string())); had_err = true; break; }
                };
                let receipt = match json_from_slice(&bytes) {
                    Ok(r) => r,
                    Err(e) => { err_m.insert("e".to_string(), Val::Text(format!("malformed receipt {}: {}", run_id, e))); err_m.insert("t".to_string(), Val::Text("err".to_string())); had_err = true; break; }
                };
                // Build node record
                let output = match &receipt { J::Object(rm) => rm.get("output").cloned().unwrap_or(J::Null), _ => J::Null };
                let mut node = BTreeMap::new();
                node.insert("run_id".to_string(), Val::Text(run_id.clone()));
                node.insert("output".to_string(), jval_to_val(&output));
                nodes.push(Val::Record(node));
                // Enqueue and record edges from derived_from
                if let J::Object(rm) = &receipt {
                    if let Some(J::Array(deps)) = rm.get("derived_from") {
                        for dep in deps {
                            if let J::Str(dep_id) = dep {
                                let mut edge = BTreeMap::new();
                                edge.insert("from".to_string(), Val::Text(run_id.clone()));
                                edge.insert("to".to_string(), Val::Text(dep_id.clone()));
                                edges.push(Val::Record(edge));
                                queue.push(dep_id.clone());
                            }
                        }
                    }
                }
            }
            if had_err { return Ok(Val::Record(err_m)); }
            let mut graph = BTreeMap::new();
            graph.insert("edges".to_string(), Val::List(edges));
            graph.insert("nodes".to_string(), Val::List(nodes));
            let mut m = BTreeMap::new();
            m.insert("ok".to_string(), Val::Record(graph));
            m.insert("t".to_string(), Val::Text("ok".to_string()));
            Ok(Val::Record(m))
        }
        Builtin::GraphAncestors => {
            // graph.ancestors(run_id) -> list of all ancestor run_ids (excluding root)
            if args.len() != 1 { bail!("ERROR_BADARG graph.ancestors expects 1 arg"); }
            let root_id = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG graph.ancestors expects text") };
            let mut queue: Vec<String> = vec![root_id.clone()];
            let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut ancestors: Vec<Val> = Vec::new();
            while let Some(run_id) = queue.pop() {
                if visited.contains(&run_id) { continue; }
                visited.insert(run_id.clone());
                if run_id != root_id { ancestors.push(Val::Text(run_id.clone())); }
                let hex = run_id.strip_prefix("sha256:").unwrap_or(&run_id);
                let receipt_path = format!("receipts/sha256_{}.json", hex);
                if let Ok(bytes) = std::fs::read(&receipt_path) {
                    if let Ok(J::Object(rm)) = json_from_slice(&bytes) {
                        if let Some(J::Array(deps)) = rm.get("derived_from") {
                            for dep in deps { if let J::Str(dep_id) = dep { queue.push(dep_id.clone()); } }
                        }
                    }
                }
            }
            Ok(Val::List(ancestors))
        }
        Builtin::GraphLeaves => {
            // graph.leaves(run_id) -> list of root run_ids (nodes with no derived_from)
            if args.len() != 1 { bail!("ERROR_BADARG graph.leaves expects 1 arg"); }
            let root_id = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG graph.leaves expects text") };
            let mut queue: Vec<String> = vec![root_id.clone()];
            let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut leaves: Vec<Val> = Vec::new();
            while let Some(run_id) = queue.pop() {
                if visited.contains(&run_id) { continue; }
                visited.insert(run_id.clone());
                let hex = run_id.strip_prefix("sha256:").unwrap_or(&run_id);
                let receipt_path = format!("receipts/sha256_{}.json", hex);
                if let Ok(bytes) = std::fs::read(&receipt_path) {
                    if let Ok(J::Object(rm)) = json_from_slice(&bytes) {
                        let deps = rm.get("derived_from").and_then(|d| if let J::Array(a) = d { Some(a.clone()) } else { None }).unwrap_or_default();
                        if deps.is_empty() {
                            leaves.push(Val::Text(run_id.clone()));
                        } else {
                            for dep in deps { if let J::Str(dep_id) = dep { queue.push(dep_id.clone()); } }
                        }
                    }
                } else {
                    leaves.push(Val::Text(run_id.clone()));
                }
            }
            Ok(Val::List(leaves))
        }
        Builtin::GraphToDot => {
            // graph.to_dot(graph_record) -> dot string
            if args.len() != 1 { bail!("ERROR_BADARG graph.to_dot expects 1 arg"); }
            let graph = match &args[0] { Val::Record(m) => m.clone(), _ => bail!("ERROR_BADARG graph.to_dot expects record from graph.of") };
            let nodes = match graph.get("nodes") { Some(Val::List(l)) => l.clone(), _ => Vec::new() };
            let edges = match graph.get("edges") { Some(Val::List(l)) => l.clone(), _ => Vec::new() };
            let mut dot = String::from("digraph witness {
  rankdir=BT;
  node [shape=box fontname=monospace];
");
            for node in &nodes {
                if let Val::Record(nr) = node {
                    let run_id = match nr.get("run_id") { Some(Val::Text(s)) => s.clone(), _ => continue };
                    let short = &run_id[run_id.len().saturating_sub(12)..];
                    dot.push_str(&format!("  \"{}\" [label=\"{}\"];\n", run_id, short));
                }
            }
            for edge in &edges {
                if let Val::Record(er) = edge {
                    let from = match er.get("from") { Some(Val::Text(s)) => s.clone(), _ => continue };
                    let to   = match er.get("to")   { Some(Val::Text(s)) => s.clone(), _ => continue };
                    dot.push_str(&format!("  \"{}\" -> \"{}\";\n", from, to));
                }
            }
            dot.push_str("}
");
            Ok(Val::Text(dot))
        }

        Builtin::HashSha256Bytes => {
            if args.len() != 1 { bail!("ERROR_BADARG hash.sha256_bytes expects 1 arg"); }
            match &args[0] {
                Val::Text(ss) => Ok(Val::Bytes(sha256_raw(ss.as_bytes()))),
                Val::Bytes(bs) => Ok(Val::Bytes(sha256_raw(bs))),
                Val::List(xs) => {
                    let mut v = Vec::with_capacity(xs.len());
                    for x in xs { match x { Val::Int(n) => v.push((*n & 0xff) as u8), _ => bail!("ERROR_BADARG sha256_bytes list must contain ints") } }
                    Ok(Val::Bytes(sha256_raw(&v)))
                }
                _ => bail!("ERROR_BADARG hash.sha256_bytes expects str, bytes, or list"),
            }
        }
        Builtin::CliArgs => {
            // cli.args() -> list of raw string args passed to program
            Ok(Val::List(
                PROGRAM_ARGS.with(|a| a.borrow().iter().map(|s| Val::Text(s.clone())).collect())
            ))
        }
        Builtin::CliGet => {
            // cli.get(parsed, name) -> string value or unit
            if args.len() != 2 { bail!("ERROR_BADARG cli.get expects (parsed, name)"); }
            let name = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG cli.get name must be string") };
            match &args[0] {
                Val::Record(m) => Ok(m.get(&name).cloned().unwrap_or(Val::Unit)),
                _ => bail!("ERROR_BADARG cli.get expects record"),
            }
        }
        Builtin::CliGetInt => {
            if args.len() != 2 { bail!("ERROR_BADARG cli.get_int expects (parsed, name)"); }
            let name = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG cli.get_int name must be string") };
            match &args[0] {
                Val::Record(m) => match m.get(&name) {
                    Some(Val::Text(s)) => Ok(Val::Int(s.parse::<i64>().map_err(|_| anyhow!("ERROR_BADARG cli.get_int: '{}' is not an int", s))?)),
                    Some(Val::Int(n)) => Ok(Val::Int(*n)),
                    Some(Val::Unit) | None => bail!("ERROR_BADARG cli.get_int: '{}' not found", name),
                    _ => bail!("ERROR_BADARG cli.get_int type error"),
                },
                _ => bail!("ERROR_BADARG cli.get_int expects record"),
            }
        }
        Builtin::CliGetFloat => {
            if args.len() != 2 { bail!("ERROR_BADARG cli.get_float expects (parsed, name)"); }
            let name = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG cli.get_float name must be string") };
            match &args[0] {
                Val::Record(m) => match m.get(&name) {
                    Some(Val::Text(s)) => Ok(Val::Float(s.parse::<f64>().map_err(|_| anyhow!("ERROR_BADARG cli.get_float: '{}' is not a float", s))?)),
                    Some(Val::Float(f)) => Ok(Val::Float(*f)),
                    Some(Val::Int(n)) => Ok(Val::Float(*n as f64)),
                    Some(Val::Unit) | None => bail!("ERROR_BADARG cli.get_float: '{}' not found", name),
                    _ => bail!("ERROR_BADARG cli.get_float type error"),
                },
                _ => bail!("ERROR_BADARG cli.get_float expects record"),
            }
        }
        Builtin::CliGetBool => {
            if args.len() != 2 { bail!("ERROR_BADARG cli.get_bool expects (parsed, name)"); }
            let name = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG cli.get_bool name must be string") };
            match &args[0] {
                Val::Record(m) => match m.get(&name) {
                    Some(Val::Bool(b)) => Ok(Val::Bool(*b)),
                    Some(Val::Text(s)) => Ok(Val::Bool(s == "true")),
                    Some(Val::Unit) | None => Ok(Val::Bool(false)),
                    _ => bail!("ERROR_BADARG cli.get_bool type error"),
                },
                _ => bail!("ERROR_BADARG cli.get_bool expects record"),
            }
        }
        Builtin::CliHas => {
            if args.len() != 2 { bail!("ERROR_BADARG cli.has expects (parsed, name)"); }
            let name = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG cli.has name must be string") };
            match &args[0] {
                Val::Record(m) => Ok(Val::Bool(matches!(m.get(&name), Some(v) if !matches!(v, Val::Unit)))),
                _ => bail!("ERROR_BADARG cli.has expects record"),
            }
        }
        Builtin::MutexNew => match args.as_slice() {
            [val] => Ok(Val::Mtx(Arc::new(Mutex::new(val.clone())))),
            _ => bail!("ERROR_BADARG mutex.new expects 1 arg (initial value)"),
        }
        Builtin::MutexLock => match args.as_slice() {
            [Val::Mtx(m)] => Ok(m.lock().unwrap().clone()),
            _ => bail!("ERROR_BADARG mutex.lock expects mutex"),
        }
        Builtin::MutexUnlock => match args.as_slice() {
            [Val::Mtx(m), val] => {
                *m.lock().unwrap() = val.clone();
                Ok(Val::Bool(true))
            }
            _ => bail!("ERROR_BADARG mutex.unlock expects (mutex, val)"),
        }
        Builtin::MutexWithLock => match args.as_slice() {
            [Val::Mtx(m), f] => {
                let current = m.lock().unwrap().clone();
                let result = call(f.clone(), vec![current], tracer, loader)?;
                Ok(result)
            }
            _ => bail!("ERROR_BADARG mutex.with_lock expects (mutex, fn)"),
        }
        Builtin::ChanNew => {
            let q: Arc<Mutex<std::collections::VecDeque<Val>>> = Arc::new(Mutex::new(std::collections::VecDeque::new()));
            let closed: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
            Ok(Val::Chan(q, closed))
        }
        Builtin::ChanSend => match args.as_slice() {
            [Val::Chan(q, closed), val] => {
                if *closed.lock().unwrap() { bail!("chan.send: channel is closed"); }
                q.lock().unwrap().push_back(val.clone());
                Ok(Val::Bool(true))
            }
            _ => bail!("ERROR_BADARG chan.send expects (chan, val)"),
        }
        Builtin::ChanRecv => match args.as_slice() {
            [Val::Chan(q, closed)] => {
                loop {
                    if let Some(v) = q.lock().unwrap().pop_front() {
                        return Ok(Val::Record({
                            let mut m = BTreeMap::new();
                            m.insert("t".to_string(), Val::Text("some".to_string()));
                            m.insert("v".to_string(), v);
                            m
                        }));
                    }
                    if *closed.lock().unwrap() {
                        return Ok(Val::Unit);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            _ => bail!("ERROR_BADARG chan.recv expects chan"),
        }
        Builtin::ChanTryRecv => match args.as_slice() {
            [Val::Chan(q, _)] => {
                match q.lock().unwrap().pop_front() {
                    Some(v) => Ok(Val::Record({
                        let mut m = BTreeMap::new();
                        m.insert("t".to_string(), Val::Text("some".to_string()));
                        m.insert("v".to_string(), v);
                        m
                    })),
                    None => Ok(Val::Unit),
                }
            }
            _ => bail!("ERROR_BADARG chan.try_recv expects chan"),
        }
        Builtin::ChanClose => match args.as_slice() {
            [Val::Chan(_, closed)] => {
                *closed.lock().unwrap() = true;
                Ok(Val::Bool(true))
            }
            _ => bail!("ERROR_BADARG chan.close expects chan"),
        }
        Builtin::IoReadStdinLines => {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            let lines: Vec<Val> = stdin.lock().lines()
                .map(|l| Val::Text(l.unwrap_or_default()))
                .collect();
            Ok(Val::List(lines))
        }
        Builtin::IoReadStdin => {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
                .map_err(|e| anyhow::anyhow!("io.read_stdin: {}", e))?;
            Ok(Val::Text(buf))
        }
        Builtin::IoListDir => match args.as_slice() {
            [Val::Text(path)] => {
                let entries = std::fs::read_dir(path)
                    .map_err(|e| anyhow::anyhow!("io.list_dir: {}", e))?;
                let mut names = Vec::new();
                for entry in entries {
                    let e = entry.map_err(|e| anyhow::anyhow!("io.list_dir entry: {}", e))?;
                    names.push(Val::Text(e.file_name().to_string_lossy().to_string()));
                }
                Ok(Val::List(names))
            }
            _ => bail!("ERROR_BADARG io.list_dir expects text"),
        }
        Builtin::IoMakeDir => match args.as_slice() {
            [Val::Text(path)] => {
                std::fs::create_dir_all(path)
                    .map_err(|e| anyhow::anyhow!("io.make_dir: {}", e))?;
                Ok(Val::Bool(true))
            }
            _ => bail!("ERROR_BADARG io.make_dir expects text"),
        }
        Builtin::IoReadFile => {
            if args.len() != 1 { bail!("ERROR_BADARG io.read_file expects 1 arg"); }
            let path = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.read_file expects string path") };
            match std::fs::read_to_string(&path) {
                Ok(s)  => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("ok".to_string(), Val::Text(s)); m })),
                Err(e) => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("err".to_string(), Val::Text(e.to_string())); m })),
            }
        }
        Builtin::IoWriteFile => {
            if args.len() != 2 { bail!("ERROR_BADARG io.write_file expects 2 args"); }
            let path    = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.write_file path must be string") };
            let content = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.write_file content must be string") };
            let content_with_newline = if content.ends_with('\n') { content } else { format!("{}\n", content) };
            match std::fs::write(&path, content_with_newline.as_bytes()) {
                Ok(_)  => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("ok".to_string(), Val::Unit); m })),
                Err(e) => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("err".to_string(), Val::Text(e.to_string())); m })),
            }
        }
        Builtin::IoAppendFile => {
            if args.len() != 2 { bail!("ERROR_BADARG io.append_file expects 2 args"); }
            let path = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.append_file path must be string") };
            let line = match &args[1] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.append_file content must be string") };
            use std::io::Write;
            match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                Ok(mut file) => {
                    let content = format!("{}
", line);
                    match file.write_all(content.as_bytes()) {
                        Ok(_)  => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("ok".to_string(), Val::Unit); m })),
                        Err(e) => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("err".to_string(), Val::Text(e.to_string())); m })),
                    }
                },
                Err(e) => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("err".to_string(), Val::Text(e.to_string())); m })),
            }
        }
        Builtin::IoReadLines => {
            if args.len() != 1 { bail!("ERROR_BADARG io.read_lines expects 1 arg"); }
            let path = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.read_lines expects string path") };
            match std::fs::read_to_string(&path) {
                Ok(s)  => {
                    let lines: Vec<Val> = s.lines().map(|l| Val::Text(l.to_string())).collect();
                    Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("ok".to_string(), Val::List(lines)); m }))
                },
                Err(e) => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("err".to_string(), Val::Text(e.to_string())); m })),
            }
        }
        Builtin::IoFileExists => {
            if args.len() != 1 { bail!("ERROR_BADARG io.file_exists expects 1 arg"); }
            let path = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.file_exists expects string path") };
            Ok(Val::Bool(std::path::Path::new(&path).exists()))
        }
        Builtin::IoDeleteFile => {
            if args.len() != 1 { bail!("ERROR_BADARG io.delete_file expects 1 arg"); }
            let path = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG io.delete_file expects string path") };
            match std::fs::remove_file(&path) {
                Ok(_)  => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("ok".to_string(), Val::Unit); m })),
                Err(e) => Ok(Val::Record({ let mut m = BTreeMap::new(); m.insert("err".to_string(), Val::Text(e.to_string())); m })),
            }
        }
        Builtin::BytesConcat => {
            if args.len() != 2 { bail!("ERROR_BADARG bytes.concat expects 2 args"); }
            let mut a = match &args[0] { Val::Bytes(b) => b.clone(), _ => bail!("ERROR_BADARG bytes.concat arg0 must be bytes") };
            let b = match &args[1] { Val::Bytes(b) => b.clone(), _ => bail!("ERROR_BADARG bytes.concat arg1 must be bytes") };
            a.extend_from_slice(&b);
            Ok(Val::Bytes(a))
        }
        Builtin::BytesLen => {
            if args.len() != 1 { bail!("ERROR_BADARG bytes.len expects 1 arg"); }
            match &args[0] {
                Val::Bytes(b) => Ok(Val::Int(b.len() as i64)),
                _ => bail!("ERROR_BADARG bytes.len expects bytes"),
            }
        }
        Builtin::BytesGet => {
            if args.len() != 2 { bail!("ERROR_BADARG bytes.get expects 2 args"); }
            match (&args[0], &args[1]) {
                (Val::Bytes(b), Val::Int(i)) => {
                    let idx = *i as usize;
                    if idx >= b.len() { bail!("ERROR_BOUNDS bytes.get index {} out of range", i) }
                    Ok(Val::Int(b[idx] as i64))
                }
                _ => bail!("ERROR_BADARG bytes.get expects (bytes, int)"),
            }
        }
        Builtin::BytesOfList => {
            if args.len() != 1 { bail!("ERROR_BADARG bytes.of_list expects 1 arg"); }
            match &args[0] {
                Val::List(xs) => {
                    let mut v = Vec::with_capacity(xs.len());
                    for x in xs { match x { Val::Int(n) => v.push((*n & 0xff) as u8), _ => bail!("ERROR_BADARG bytes.of_list list must contain ints") } }
                    Ok(Val::Bytes(v))
                }
                _ => bail!("ERROR_BADARG bytes.of_list expects list"),
            }
        }
        Builtin::BytesToList => {
            if args.len() != 1 { bail!("ERROR_BADARG bytes.to_list expects 1 arg"); }
            match &args[0] {
                Val::Bytes(b) => Ok(Val::List(b.iter().map(|x| Val::Int(*x as i64)).collect())),
                _ => bail!("ERROR_BADARG bytes.to_list expects bytes"),
            }
        }
        Builtin::BytesOfStr => {
            if args.len() != 1 { bail!("ERROR_BADARG bytes.of_str expects 1 arg"); }
            match &args[0] {
                Val::Text(s) => Ok(Val::Bytes(s.as_bytes().to_vec())),
                _ => bail!("ERROR_BADARG bytes.of_str expects string"),
            }
        }
        Builtin::BytesToStr => {
            if args.len() != 1 { bail!("ERROR_BADARG bytes.to_str expects 1 arg"); }
            match &args[0] {
                Val::Bytes(bs) => {
                    match String::from_utf8(bs.clone()) {
                        Ok(s)  => Ok(Val::Text(s)),
                        Err(_) => bail!("ERROR_BADARG bytes.to_str: bytes are not valid UTF-8"),
                    }
                }
                Val::Text(s) => Ok(Val::Text(s.clone())),
                _ => bail!("ERROR_BADARG bytes.to_str expects bytes"),
            }
        }
        Builtin::BytesMerkleRoot => {
            // merkle_root(list_of_bytes) -> bytes
            if args.len() != 1 { bail!("ERROR_BADARG bytes.merkle_root expects 1 arg"); }
            match &args[0] {
                Val::List(xs) => {
                    let leaves: Result<Vec<[u8;32]>, _> = xs.iter().map(|x| match x {
                        Val::Bytes(b) => b.as_slice().try_into().map_err(|_| anyhow!("ERROR_BADARG merkle_root each leaf must be 32 bytes")),
                        _ => Err(anyhow!("ERROR_BADARG merkle_root expects list of bytes")),
                    }).collect();
                    let leaves = leaves?;
                    Ok(Val::Bytes(merkle_root_bytes(&leaves).to_vec()))
                }
                _ => bail!("ERROR_BADARG bytes.merkle_root expects list"),
            }
        }
        Builtin::CodecHexEncode => {
            if args.len() != 1 { bail!("ERROR_BADARG codec.hex_encode expects 1 arg"); }
            match &args[0] {
                Val::Bytes(bs) => Ok(Val::Text(hex_lower(bs))),
                Val::Text(ss) => Ok(Val::Text(hex_lower(ss.as_bytes()))),
                _ => bail!("ERROR_BADARG codec.hex_encode expects str or bytes"),
            }
        }
        Builtin::CodecHexDecode => {
            if args.len() != 1 { bail!("ERROR_BADARG codec.hex_decode expects 1 arg"); }
            let s = match &args[0] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_BADARG type") };
            let bytes = hex_decode(s.as_str())?;
            Ok(Val::Text(String::from_utf8(bytes)?))
        }
        Builtin::StrSplit => {
            if args.len() != 2 { bail!("ERROR_BADARG str.split expects 2 args"); }
            let s = match &args[0] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_BADARG str.split arg0 must be string") };
            let delim = match &args[1] { Val::Text(ss) => ss.clone(), _ => bail!("ERROR_BADARG str.split arg1 must be string") };
            let parts: Vec<Val> = s.split(delim.as_str()).map(|p| Val::Text(p.to_string())).collect();
            Ok(Val::List(parts))
        }
        Builtin::StrUpper => {
            match args.first() {
                Some(Val::Text(s)) => Ok(Val::Text(s.to_uppercase())),
                _ => bail!("ERROR_BADARG str.upper expects string"),
            }
        }
        Builtin::StrContains => {
            if args.len() != 2 { bail!("ERROR_ARITY str.contains"); }
            match (&args[0], &args[1]) {
                (Val::Text(s), Val::Text(sub)) => Ok(Val::Bool(s.contains(sub.as_str()))),
                _ => bail!("ERROR_BADARG str.contains expects string, string"),
            }
        }
        Builtin::StrStartsWith => {
            if args.len() != 2 { bail!("ERROR_ARITY str.starts_with"); }
            match (&args[0], &args[1]) {
                (Val::Text(s), Val::Text(pre)) => Ok(Val::Bool(s.starts_with(pre.as_str()))),
                _ => bail!("ERROR_BADARG str.starts_with expects string, string"),
            }
        }
        Builtin::StrEndsWith => {
            if args.len() != 2 { bail!("ERROR_ARITY str.ends_with"); }
            match (&args[0], &args[1]) {
                (Val::Text(s), Val::Text(suf)) => Ok(Val::Bool(s.ends_with(suf.as_str()))),
                _ => bail!("ERROR_BADARG str.ends_with expects string, string"),
            }
        }
        Builtin::StrReplace => {
            if args.len() != 3 { bail!("ERROR_ARITY str.replace expects 3 args"); }
            match (&args[0], &args[1], &args[2]) {
                (Val::Text(s), Val::Text(from), Val::Text(to)) => Ok(Val::Text(s.replace(from.as_str(), to.as_str()))),
                _ => bail!("ERROR_BADARG str.replace expects string, string, string"),
            }
        }
        Builtin::StrSlice => {
            if args.len() != 3 { bail!("ERROR_ARITY str.slice expects 3 args"); }
            match (&args[0], &args[1], &args[2]) {
                (Val::Text(s), Val::Int(start), Val::Int(end)) => {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len() as i64;
                    let s2 = (*start).max(0).min(len) as usize;
                    let e2 = (*end).max(0).min(len) as usize;
                    Ok(Val::Text(chars[s2..e2.max(s2)].iter().collect()))
                }
                _ => bail!("ERROR_BADARG str.slice expects string, int, int"),
            }
        }
        Builtin::StrFormat => {
            // str.format(template, rec) — replaces {key} with rec.key
            if args.len() != 2 { bail!("ERROR_ARITY str.format expects 2 args"); }
            match (&args[0], &args[1]) {
                (Val::Text(tmpl), Val::Record(m)) => {
                    let mut out = tmpl.clone();
                    for (k, v) in m {
                        let placeholder = format!("{{{}}}", k);
                        let val_str = match v {
                            Val::Text(s) => s.clone(),
                            Val::Int(n) => n.to_string(),
                            Val::Bool(b) => b.to_string(),
                            Val::Bytes(b) if b.len() == 8 => {
                                let arr: [u8;8] = b.as_slice().try_into().unwrap_or([0u8;8]);
                                f64::from_le_bytes(arr).to_string()
                            }
                            _ => format!("{:?}", v),
                        };
                        out = out.replace(&placeholder, &val_str);
                    }
                    Ok(Val::Text(out))
                }
                _ => bail!("ERROR_BADARG str.format expects string, record"),
            }
        }
        Builtin::StrFromInt => {
            match args.first() {
                Some(Val::Int(n)) => Ok(Val::Text(n.to_string())),
                _ => bail!("ERROR_BADARG str.from_int expects int"),
            }
        }
        Builtin::StrFromFloat => {
            match args.first() {
                Some(Val::Bytes(b)) if b.len() == 8 => {
                    let arr: [u8;8] = b.as_slice().try_into().unwrap_or([0u8;8]);
                    Ok(Val::Text(f64::from_le_bytes(arr).to_string()))
                }
                Some(Val::Int(n)) => Ok(Val::Text((*n as f64).to_string())),
                Some(Val::Float(f)) => Ok(Val::Text(f.to_string())),
                _ => bail!("ERROR_BADARG str.from_float expects float"),
            }
        }
        Builtin::StrPadLeft => {
            if args.len() != 3 { bail!("ERROR_ARITY str.pad_left"); }
            match (&args[0], &args[1], &args[2]) {
                (Val::Text(s), Val::Int(width), Val::Text(pad)) => {
                    let w = (*width).max(0) as usize;
                    let pc: char = pad.chars().next().unwrap_or(' ');
                    let chars: Vec<char> = s.chars().collect();
                    if chars.len() >= w { return Ok(Val::Text(s.clone())); }
                    let padding: String = std::iter::repeat(pc).take(w - chars.len()).collect();
                    Ok(Val::Text(format!("{}{}", padding, s)))
                }
                _ => bail!("ERROR_BADARG str.pad_left expects string, int, string"),
            }
        }
        Builtin::StrPadRight => {
            if args.len() != 3 { bail!("ERROR_ARITY str.pad_right"); }
            match (&args[0], &args[1], &args[2]) {
                (Val::Text(s), Val::Int(width), Val::Text(pad)) => {
                    let w = (*width).max(0) as usize;
                    let pc: char = pad.chars().next().unwrap_or(' ');
                    let chars: Vec<char> = s.chars().collect();
                    if chars.len() >= w { return Ok(Val::Text(s.clone())); }
                    let padding: String = std::iter::repeat(pc).take(w - chars.len()).collect();
                    Ok(Val::Text(format!("{}{}", s, padding)))
                }
                _ => bail!("ERROR_BADARG str.pad_right expects string, int, string"),
            }
        }
        Builtin::StrRepeat => {
            if args.len() != 2 { bail!("ERROR_ARITY str.repeat"); }
            match (&args[0], &args[1]) {
                (Val::Text(s), Val::Int(n)) => Ok(Val::Text(s.repeat((*n).max(0) as usize))),
                _ => bail!("ERROR_BADARG str.repeat expects string, int"),
            }
        }
        Builtin::StrIndexOf => {
            if args.len() != 2 { bail!("ERROR_ARITY str.index_of"); }
            match (&args[0], &args[1]) {
                (Val::Text(s), Val::Text(sub)) => {
                    Ok(Val::Int(s.find(sub.as_str()).map(|i| i as i64).unwrap_or(-1)))
                }
                _ => bail!("ERROR_BADARG str.index_of expects string, string"),
            }
        }
        Builtin::StrChars => {
            match args.first() {
                Some(Val::Text(s)) => Ok(Val::List(s.chars().map(|c| Val::Text(c.to_string())).collect())),
                _ => bail!("ERROR_BADARG str.chars expects string"),
            }
        }
        Builtin::FsReadText => {
            if args.len() != 1 { bail!("ERROR_ARITY fs.read_text"); }
            match &args[0] {
                Val::Text(path) => {
                    let content = std::fs::read_to_string(path.as_str())
                        .map_err(|e| anyhow!("ERROR_IO fs.read_text {}: {}", path, e))?;
                    Ok(Val::Text(content))
                }
                _ => bail!("ERROR_BADARG fs.read_text expects string path"),
            }
        }

        Builtin::FsWriteText => {
            if args.len() != 2 { bail!("ERROR_ARITY fs.write_text expects 2 args"); }
            let path = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG fs.write_text path must be text"),
            };
            let content = match &args[1] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG fs.write_text content must be text"),
            };
            fs_sandbox_check(&path)?;
            if let Some(parent) = std::path::Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| anyhow!("ERROR_IO fs.write_text mkdir {}: {}", path, e))?;
                }
            }
            std::fs::write(&path, content.as_bytes())
                .map_err(|e| anyhow!("ERROR_IO fs.write_text {}: {}", path, e))?;
            Ok(Val::Unit)
        }
        Builtin::FsExists => {
            if args.len() != 1 { bail!("ERROR_ARITY fs.exists expects 1 arg"); }
            let path = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG fs.exists path must be text"),
            };
            Ok(Val::Bool(std::path::Path::new(&path).exists()))
        }
        Builtin::FsReadDir => {
            if args.len() != 1 { bail!("ERROR_ARITY fs.read_dir expects 1 arg"); }
            let path = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG fs.read_dir path must be text"),
            };
            let entries = std::fs::read_dir(&path)
                .map_err(|e| anyhow!("ERROR_IO fs.read_dir {}: {}", path, e))?;
            let mut names: Vec<Val> = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| anyhow!("ERROR_IO fs.read_dir entry: {}", e))?;
                let name = entry.file_name().to_string_lossy().to_string();
                names.push(Val::Text(name));
            }
            names.sort_by(|a, b| match (a, b) {
                (Val::Text(x), Val::Text(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            });
            Ok(Val::List(names))
        }
        Builtin::FsStat => {
            if args.len() != 1 { bail!("ERROR_ARITY fs.stat expects 1 arg"); }
            let path = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG fs.stat path must be text"),
            };
            let meta = std::fs::metadata(&path)
                .map_err(|e| anyhow!("ERROR_IO fs.stat {}: {}", path, e))?;
            let mut m = BTreeMap::new();
            m.insert("is_file".to_string(), Val::Bool(meta.is_file()));
            m.insert("is_dir".to_string(), Val::Bool(meta.is_dir()));
            m.insert("size".to_string(), Val::Int(meta.len() as i64));
            Ok(Val::Record(m))
        }
        Builtin::FsDelete => {
            if args.len() != 1 { bail!("ERROR_ARITY fs.delete expects 1 arg"); }
            let path = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG fs.delete path must be text"),
            };
            fs_sandbox_check(&path)?;
            let p = std::path::Path::new(&path);
            if p.is_dir() {
                std::fs::remove_dir_all(&path)
                    .map_err(|e| anyhow!("ERROR_IO fs.delete {}: {}", path, e))?;
            } else {
                std::fs::remove_file(&path)
                    .map_err(|e| anyhow!("ERROR_IO fs.delete {}: {}", path, e))?;
            }
            Ok(Val::Unit)
        }
        Builtin::FsMakeDir => {
            if args.len() != 1 { bail!("ERROR_ARITY fs.make_dir expects 1 arg"); }
            let path = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG fs.make_dir path must be text"),
            };
            fs_sandbox_check(&path)?;
            std::fs::create_dir_all(&path)
                .map_err(|e| anyhow!("ERROR_IO fs.make_dir {}: {}", path, e))?;
            Ok(Val::Unit)
        }
        Builtin::StrSplitLines => {
            if args.len() != 1 {
                bail!("ERROR_BADARG str.split_lines expects 1 arg");
            }
            let s = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG str.split_lines arg0 must be string"),
            };
            // .lines() drops trailing empty line and handles \r\n
            let parts: Vec<Val> = s.lines().map(|x| Val::Text(x.to_string())).collect();
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
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG import_artifact arg must be string"),
            };
            let disk = tracer.out_dir.join("artifacts").join(&p);
            let bytes = match fs::read(&disk) {
                Ok(b) => b,
                Err(e) => {
                    return Ok(mk_result_err(Val::Text(format!(
                        "ERROR_IO cannot read artifact: {p} ({e})"
                    ))));
                }
            };
            let cid = sha256_bytes(&bytes);
            tracer.artifact_in(&p, &cid)?;
            let text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    return Ok(mk_result_err(Val::Text(format!(
                        "ERROR_UTF8 invalid utf8 in artifact: {p} ({e})"
                    ))));
                }
            };
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("text".to_string(), Val::Text(text));
            rec.insert("cid".to_string(), Val::Text(cid));
            Ok(mk_result_ok(Val::Record(rec)))
        }

        Builtin::ImportArtifactNamed => {
            if args.len() != 2 {
                bail!("ERROR_BADARG import_artifact_named expects 2 args");
            }
            let name = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG import_artifact_named name must be string"),
            };
            let p = match &args[1] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG import_artifact_named path must be string"),
            };

            let bytes = match fs::read(&p) {
                Ok(b) => b,
                Err(e) => {
                    return Ok(mk_result_err(Val::Text(format!(
                        "ERROR_IO cannot read artifact: {p} ({e})"
                    ))));
                }
            };
            let cid = sha256_bytes(&bytes);
            tracer.artifact_in_named(&name, &p, &cid)?;

            let text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    return Ok(mk_result_err(Val::Text(format!(
                        "ERROR_UTF8 invalid utf8 in artifact: {p} ({e})"
                    ))));
                }
            };

            let mut rec = std::collections::BTreeMap::new();
            rec.insert("text".to_string(), Val::Text(text));
            rec.insert("cid".to_string(), Val::Text(cid));
            Ok(mk_result_ok(Val::Record(rec)))
        }
        Builtin::EmitArtifact => {
            if args.len() != 2 {
                bail!("ERROR_BADARG emit_artifact expects 2 args");
            }
            let name = match &args[0] {
                Val::Text(s) => s.clone(),
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
                                return Ok(mk_result_err(Val::Text(
                                    "ERROR_BADARG emit_artifact bytes must be ints".to_string(),
                                )));
                            }
                        };
                        if n < 0 || n > 255 {
                            return Ok(mk_result_err(Val::Text(
                                "ERROR_BADARG emit_artifact byte out of range".to_string(),
                            )));
                        }
                        out.push(n as u8);
                    }
                    out
                }
                Val::Record(m) => match m.get("text") {
                    Some(Val::Text(s)) => s.as_bytes().to_vec(),
                    _ => {
                        return Ok(mk_result_err(Val::Text(
                            "ERROR_BADARG emit_artifact expects bytes:list[int] or {text:string}"
                                .to_string(),
                        )));
                    }
                },
                _ => {
                    return Ok(mk_result_err(Val::Text(
                        "ERROR_BADARG emit_artifact expects bytes:list[int] or {text:string}"
                            .to_string(),
                    )));
                }
            };
            let cid = sha256_bytes(&bytes);
            // tracer.artifact_out writes to out_dir/artifacts/<name> and traces it
            if let Err(e) = tracer.artifact_out(&name, &cid, &bytes) {
                return Ok(mk_result_err(Val::Text(format!(
                    "ERROR_IO cannot write artifact: {name} ({e})"
                ))));
            }
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("name".to_string(), Val::Text(name));
            rec.insert("cid".to_string(), Val::Text(cid));
            Ok(mk_result_ok(Val::Record(rec)))
        }
        Builtin::EmitArtifactDerived => {
            if args.len() != 4 {
                bail!("ERROR_BADARG emit_artifact_derived expects 4 args");
            }

            let name = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG emit_artifact_derived name must be string"),
            };

            let filename = match &args[1] {
                Val::Text(s) => s.clone(),
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
                                return Ok(mk_result_err(Val::Text(
                                    "ERROR_BADARG emit_artifact_derived bytes must be ints"
                                        .to_string(),
                                )));
                            }
                        };
                        if n < 0 || n > 255 {
                            return Ok(mk_result_err(Val::Text(
                                "ERROR_BADARG emit_artifact_derived byte out of range".to_string(),
                            )));
                        }
                        out.push(n as u8);
                    }
                    out
                }
                Val::Record(m) => {
                    if let Some(Val::Text(s)) = m.get("text") {
                        s.as_bytes().to_vec()
                    } else {
                        let j = match args[2].to_json() {
                            Some(j) => j,
                            None => {
                                return Ok(mk_result_err(Val::Text(
                                    "ERROR_BADARG emit_artifact_derived value must be jsonable"
                                        .to_string(),
                                )));
                            }
                        };
                        match Ok::<Vec<u8>, anyhow::Error>(json_to_string(&j).into_bytes()) {
                            Ok(b) => b,
                            Err(e) => {
                                return Ok(mk_result_err(Val::Text(format!(
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
                            return Ok(mk_result_err(Val::Text(
                                "ERROR_BADARG emit_artifact_derived value must be jsonable"
                                    .to_string(),
                            )));
                        }
                    };
                    match Ok::<Vec<u8>, anyhow::Error>(json_to_string(&j).into_bytes()) {
                        Ok(b) => b,
                        Err(e) => {
                            return Ok(mk_result_err(Val::Text(format!(
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
                            Val::Text(s) => out.push(s.clone()),
                            _ => {
                                return Ok(mk_result_err(Val::Text(
                "ERROR_BADARG emit_artifact_derived parents must be list[string]".to_string(),
              )));
                            }
                        }
                    }
                    out
                }
                _ => {
                    return Ok(mk_result_err(Val::Text(
                        "ERROR_BADARG emit_artifact_derived parents must be list[string]"
                            .to_string(),
                    )));
                }
            };

            if parent_names.is_empty() {
                return Ok(mk_result_err(Val::Text(
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
            rec.insert("name".to_string(), Val::Text(name));
            rec.insert("cid".to_string(), Val::Text(cid));
            Ok(mk_result_ok(Val::Record(rec)))
        }

        Builtin::Emit => {
            if args.len() != 1 {
                bail!("emit arity");
            }
            let j = args[0]
                .to_json()
                .ok_or_else(|| anyhow!("emit arg must be jsonable"))?;
            tracer.emit(&j)?;
            Ok(Val::Unit)
        }
        Builtin::Len => {
            if args.len() != 1 {
                bail!("len arity");
            }
            match &args[0] {
                Val::List(xs) => Ok(Val::Int(xs.len() as i64)),
                Val::Text(s) => Ok(Val::Int(s.as_bytes().len() as i64)),
                _ => bail!("len expects list or string"),
            }
        }
        Builtin::IntParse => {
            if args.len() != 1 {
                bail!("ERROR_BADARG int.parse expects 1 arg");
            }
            let s = match &args[0] {
                Val::Text(s) => s.clone(),
                _ => bail!("ERROR_BADARG int.parse arg0 must be string"),
            };
            match s.trim().parse::<i64>() {
                Ok(n) => Ok(mk_result_ok(Val::Int(n))),
                Err(e) => Ok(mk_result_err(Val::Text(format!(
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
                out_list.push(Val::Record(rec));
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
                Val::Record(m) => {
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
                    Val::Unit => break,
                    Val::Record(m) => {
                        let next_seed = if let Some(v) = m.get("seed").cloned() {
                            v
                        } else if let Some(v) = m.get("i").cloned() {
                            let mut mm = BTreeMap::new();
                            mm.insert("i".to_string(), v);
                            Val::Record(mm)
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
                            Val::Record(m) => m,
                            _ => bail!("unfold step list must contain record"),
                        };
                        let next_seed = if let Some(v) = m.get("seed").cloned() {
                            v
                        } else if let Some(v) = m.get("i").cloned() {
                            let mut mm = BTreeMap::new();
                            mm.insert("i".to_string(), v);
                            Val::Record(mm)
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
                Some(Val::Text(s)) => match s.parse::<f64>() {
                    Ok(v) => Ok(Val::Bytes(v.to_le_bytes().to_vec())),
                    Err(_) => bail!("ERROR_PARSE float.from_text: {}", s),
                },
                _ => bail!("ERROR_BADARG float.from_text"),
            }
        }
        Builtin::FloatToText => {
            let f = fb64_1(&args)?;
            Ok(Val::Text(format!("{}", f)))
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
        Builtin::ListSet => {
            if args.len() != 3 { bail!("ERROR_BADARG list.set expects (list, idx, val)"); }
            match (&args[0], &args[1]) {
                (Val::List(xs), Val::Int(i)) => {
                    let mut v = xs.clone();
                    let idx = *i as usize;
                    if idx >= v.len() { bail!("ERROR_BOUNDS list.set index {} out of range (len {})", i, v.len()); }
                    v[idx] = args[2].clone();
                    Ok(Val::List(v))
                }
                _ => bail!("ERROR_BADARG list.set expects (list, int, val)"),
            }
        }
        Builtin::StrJoin => match args.as_slice() {
            [Val::List(items), Val::Text(sep)] => {
                let mut out = String::new();
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { out.push_str(sep); }
                    match item {
                        Val::Text(s) => out.push_str(s),
                        _ => bail!("ERROR_BADARG str.join: list items must be text"),
                    }
                }
                Ok(Val::Text(out))
            }
            _ => bail!("ERROR_BADARG str.join expects (list, text)"),
        }
        Builtin::ListAny => match args.as_slice() {
            [Val::List(items), Val::Func(_)|Val::Builtin(_)|Val::BoundMethod(_,_)] => {
                let f = args[1].clone();
                for item in items {
                    let r = call(f.clone(), vec![item.clone()], tracer, loader)?;
                    if matches!(r, Val::Bool(true)) { return Ok(Val::Bool(true)); }
                }
                Ok(Val::Bool(false))
            }
            _ => bail!("ERROR_BADARG list.any expects (list, fn)"),
        }
        Builtin::ListAll => match args.as_slice() {
            [Val::List(items), Val::Func(_)|Val::Builtin(_)|Val::BoundMethod(_,_)] => {
                let f = args[1].clone();
                for item in items {
                    let r = call(f.clone(), vec![item.clone()], tracer, loader)?;
                    if matches!(r, Val::Bool(false)) { return Ok(Val::Bool(false)); }
                }
                Ok(Val::Bool(true))
            }
            _ => bail!("ERROR_BADARG list.all expects (list, fn)"),
        }
        Builtin::ListFind => match args.as_slice() {
            [Val::List(items), Val::Func(_)|Val::Builtin(_)|Val::BoundMethod(_,_)] => {
                let f = args[1].clone();
                for item in items {
                    let r = call(f.clone(), vec![item.clone()], tracer, loader)?;
                    if matches!(r, Val::Bool(true)) {
                        let mut m = BTreeMap::new();
                        m.insert("some".to_string(), item.clone());
                        return Ok(Val::Record(m));
                    }
                }
                let mut m = BTreeMap::new();
                m.insert("none".to_string(), Val::Unit);
                Ok(Val::Record(m))
            }
            _ => bail!("ERROR_BADARG list.find expects (list, fn)"),
        }
        Builtin::ListFindIndex => match args.as_slice() {
            [Val::List(items), Val::Func(_)|Val::Builtin(_)|Val::BoundMethod(_,_)] => {
                let f = args[1].clone();
                for (i, item) in items.iter().enumerate() {
                    let r = call(f.clone(), vec![item.clone()], tracer, loader)?;
                    if matches!(r, Val::Bool(true)) { return Ok(Val::Int(i as i64)); }
                }
                Ok(Val::Int(-1))
            }
            _ => bail!("ERROR_BADARG list.find_index expects (list, fn)"),
        }
        Builtin::ListTake => match args.as_slice() {
            [Val::List(items), Val::Int(n)] => {
                let n = (*n).max(0) as usize;
                Ok(Val::List(items.iter().take(n).cloned().collect()))
            }
            _ => bail!("ERROR_BADARG list.take expects (list, int)"),
        }
        Builtin::ListDrop => match args.as_slice() {
            [Val::List(items), Val::Int(n)] => {
                let n = (*n).max(0) as usize;
                Ok(Val::List(items.iter().skip(n).cloned().collect()))
            }
            _ => bail!("ERROR_BADARG list.drop expects (list, int)"),
        }
        Builtin::ListFlatMap => match args.as_slice() {
            [Val::List(items), Val::Func(_)|Val::Builtin(_)|Val::BoundMethod(_,_)] => {
                let f = args[1].clone();
                let mut out = Vec::new();
                for item in items {
                    let r = call(f.clone(), vec![item.clone()], tracer, loader)?;
                    match r {
                        Val::List(sub) => out.extend(sub),
                        other => out.push(other),
                    }
                }
                Ok(Val::List(out))
            }
            _ => bail!("ERROR_BADARG list.flat_map expects (list, fn)"),
        }
        Builtin::CellNew => match args.as_slice() {
            [v] => Ok(Val::List(vec![v.clone()])),  // cell is a single-element list as mutable box
            _ => bail!("cell.new expects 1 arg"),
        }
        Builtin::CellGet => match args.as_slice() {
            [Val::List(v)] if v.len() == 1 => Ok(v[0].clone()),
            _ => bail!("cell.get expects a cell"),
        }
        Builtin::CellSet => match args.as_slice() {
            [Val::List(v), new_val] if v.len() == 1 => Ok(Val::List(vec![new_val.clone()])),
            _ => bail!("cell.set expects (cell, value)"),
        }
        Builtin::FardEval => match args.as_slice() {
            [Val::Text(code)] => {
                let mut p = Parser::from_src(code, "<eval>")?;
                let items = p.parse_module()?;
                let mut child_env = Env::new();
                let last = loader.eval_items(items, &mut child_env, tracer, std::path::Path::new("."))?;
                Ok(last)
            }
            _ => bail!("ERROR_BADARG eval expects text"),
        }
        Builtin::ListParMap => match args.as_slice() {
            [Val::List(items), f] => {
                let items = items.clone();
                let f = f.clone();
                // Use thread-per-item for pure functions (no IO/module access)
                let handles: Vec<_> = items.into_iter().map(|item| {
                    let f2 = f.clone();
                    std::thread::spawn(move || {
                        let tmp = std::path::Path::new("/tmp");
                        let null_path = tmp.join(format!("fard_par_{}.ndjson", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos()));
                        let mut tracer = Tracer::new(tmp, &null_path).unwrap();
                        let mut loader = ModuleLoader::new(tmp);
                        call(f2, vec![item], &mut tracer, &mut loader)
                    })
                }).collect();
                let mut results = Vec::new();
                for h in handles {
                    results.push(h.join().map_err(|_| anyhow::anyhow!("par_map thread panicked"))??);
                }
                Ok(Val::List(results))
            }
            _ => bail!("ERROR_BADARG list.par_map expects (list, fn)"),
        }
        Builtin::AstParse => match args.as_slice() {
            [Val::Text(code)] => {
                let file = "<ast>".to_string();
                let mut parser = Parser::from_src(code, &file)
                    .map_err(|e| anyhow::anyhow!("ast.parse: {}", e))?;
                let items = parser.parse_module()
                    .map_err(|e| anyhow::anyhow!("ast.parse: {}", e))?;
                fn expr_to_val(e: &Expr) -> Val {
                    match e {
                        Expr::Int(n) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("int".to_string())); m.insert("v".to_string(), Val::Int(*n)); Val::Record(m) }
                        Expr::Bool(b) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("bool".to_string())); m.insert("v".to_string(), Val::Bool(*b)); Val::Record(m) }
                        Expr::Str(s) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("str".to_string())); m.insert("v".to_string(), Val::Text(s.clone())); Val::Record(m) }
                        Expr::Var(n) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("var".to_string())); m.insert("name".to_string(), Val::Text(n.clone())); Val::Record(m) }
                        Expr::Null => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("null".to_string())); Val::Record(m) }
                        Expr::Bin(op, l, r) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("bin".to_string())); m.insert("op".to_string(), Val::Text(op.clone())); m.insert("l".to_string(), expr_to_val(l)); m.insert("r".to_string(), expr_to_val(r)); Val::Record(m) }
                        Expr::Call(f, args) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("call".to_string())); m.insert("f".to_string(), expr_to_val(f)); m.insert("args".to_string(), Val::List(args.iter().map(expr_to_val).collect())); Val::Record(m) }
                        Expr::If(c, t, f) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("if".to_string())); m.insert("cond".to_string(), expr_to_val(c)); m.insert("then".to_string(), expr_to_val(t)); m.insert("else".to_string(), expr_to_val(f)); Val::Record(m) }
                        Expr::Let(n, v, body) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("let".to_string())); m.insert("name".to_string(), Val::Text(n.clone())); m.insert("val".to_string(), expr_to_val(v)); m.insert("body".to_string(), expr_to_val(body)); Val::Record(m) }
                        Expr::List(xs) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("list".to_string())); m.insert("items".to_string(), Val::List(xs.iter().map(expr_to_val).collect())); Val::Record(m) }
                        _ => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("other".to_string())); Val::Record(m) }
                    }
                }
                fn item_to_val(item: &Item) -> Val {
                    match item {
                        Item::Expr(e, _) => expr_to_val(e),
                        Item::Let(n, e, _) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("let".to_string())); m.insert("name".to_string(), Val::Text(n.clone())); m.insert("val".to_string(), expr_to_val(e)); Val::Record(m) }
                        Item::Fn(n, _, _, body) => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("fn".to_string())); m.insert("name".to_string(), Val::Text(n.clone())); m.insert("body".to_string(), expr_to_val(body)); Val::Record(m) }
                        _ => { let mut m = BTreeMap::new(); m.insert("t".to_string(), Val::Text("item".to_string())); Val::Record(m) }
                    }
                }
                Ok(Val::List(items.iter().map(item_to_val).collect()))
            }
            _ => bail!("ast.parse expects text"),
        }
        Builtin::PromiseSpawn => match args.as_slice() {
            [f] => {
                let fv = f.clone();
                let spawn_id = format!("spawn_{}", uuid::Uuid::new_v4());
                let tmp = std::env::temp_dir();
                let trace_path = tmp.join(format!("promise_trace_{}.ndjson", &spawn_id));
                let trace_path2 = trace_path.clone();
                let slot: Arc<Mutex<Option<Result<Val, String>>>> = Arc::new(Mutex::new(None));
                let slot2 = slot.clone();
                let child_vm_fns = VM_FNS.with(|cell| cell.borrow().clone());
                let child_self_slots = VM_SELF_SLOTS.with(|cell| cell.borrow().clone());
                tracer.child_spawn(&spawn_id)?;
                std::thread::spawn(move || {
                    VM_FNS.with(|cell| { *cell.borrow_mut() = child_vm_fns; });
                    VM_SELF_SLOTS.with(|cell| { *cell.borrow_mut() = child_self_slots; });
                    let trace_file = fs::File::create(&trace_path2).unwrap_or_else(|_| {
                        fs::File::create(std::env::temp_dir().join("fallback_trace.ndjson")).unwrap()
                    });
                    let mut child_tracer = Tracer {
                        first_event: true,
                        artifact_cids: std::collections::BTreeMap::new(),
                        w: trace_file,
                        out_dir: tmp.clone(),
                    };
                    let mut loader = ModuleLoader::new(&tmp);
                    let result = call(fv, vec![], &mut child_tracer, &mut loader)
                        .map_err(|e| e.to_string());
                    *slot2.lock().unwrap() = Some(result);
                });
                Ok(Val::Promise(slot, spawn_id, trace_path))
            }
            _ => bail!("promise.spawn expects a function"),
        }

        Builtin::PromiseSpawnOrdered => {
            // spawn_ordered(fns) — spawn a list of fns, await ALL in spawn order.
            // Returns a list of results in deterministic order regardless of completion order.
            // Every run on the same inputs produces the same result list and the same trace.
            match args.as_slice() {
                [Val::List(fns)] => {
                    let fns = fns.clone();
                    let tmp = std::env::temp_dir();
                    let mut handles: Vec<(
                        String,  // spawn_id
                        std::path::PathBuf,  // trace_path
                        Arc<Mutex<Option<Result<Val, String>>>>,
                    )> = Vec::new();

                    // Spawn all in order
                    for fv in &fns {
                        let fv = fv.clone();
                        if !matches!(fv, Val::Func(_) | Val::Builtin(_) | Val::VmFunc(_)) {
                            bail!("promise.spawn_ordered: all elements must be functions");
                        }
                        let spawn_id = format!("spawn_{}", uuid::Uuid::new_v4());
                        let trace_path = tmp.join(format!("promise_trace_{}.ndjson", &spawn_id));
                        let trace_path2 = trace_path.clone();
                        let slot: Arc<Mutex<Option<Result<Val, String>>>> = Arc::new(Mutex::new(None));
                        let slot2 = slot.clone();
                        let child_vm_fns = VM_FNS.with(|cell| cell.borrow().clone());
                        let child_self_slots = VM_SELF_SLOTS.with(|cell| cell.borrow().clone());
                        tracer.child_spawn(&spawn_id)?;
                        let tmp2 = tmp.clone();
                        std::thread::spawn(move || {
                            VM_FNS.with(|cell| { *cell.borrow_mut() = child_vm_fns; });
                            VM_SELF_SLOTS.with(|cell| { *cell.borrow_mut() = child_self_slots; });
                            let trace_file = fs::File::create(&trace_path2).unwrap_or_else(|_| {
                                fs::File::create(std::env::temp_dir().join("fallback_trace.ndjson")).unwrap()
                            });
                            let mut child_tracer = Tracer {
                                first_event: true,
                                artifact_cids: std::collections::BTreeMap::new(),
                                w: trace_file,
                                out_dir: tmp2,
                            };
                            let mut loader = ModuleLoader::new(&std::env::temp_dir());
                            let result = call(fv, vec![], &mut child_tracer, &mut loader)
                                .map_err(|e| e.to_string());
                            *slot2.lock().unwrap() = Some(result);
                        });
                        handles.push((spawn_id, trace_path, slot));
                    }

                    // Await ALL in spawn order (deterministic join)
                    let mut results: Vec<Val> = Vec::new();
                    for (spawn_id, trace_path, slot) in &handles {
                        // Spin-wait for this specific promise in order
                        loop {
                            let done = slot.lock().unwrap().is_some();
                            if done { break; }
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        let result = slot.lock().unwrap().take().unwrap();
                        let val = result.map_err(|e| anyhow::anyhow!("{}", e))?;

                        // Compute digests
                        let run_digest = if trace_path.exists() {
                            let trace_bytes = fs::read(trace_path).unwrap_or_default();
                            let digest = format!("sha256:{}", hex_lower(&{
                                let mut h = NativeSha256::new();
                                h.update(&trace_bytes);
                                h.finalize()
                            }));
                            let _ = fs::remove_file(trace_path);
                            digest
                        } else {
                            "sha256:no-trace".to_string()
                        };
                        let result_digest = if let Some(j) = val.to_json() {
                            let s = json_to_string(&j);
                            format!("sha256:{}", hex_lower(&{
                                let mut h = NativeSha256::new();
                                h.update(s.as_bytes());
                                h.finalize()
                            }))
                        } else {
                            "sha256:no-result".to_string()
                        };
                        tracer.child_receipt(spawn_id, &run_digest, &result_digest)?;
                        results.push(val);
                    }

                    // Emit ordered receipt event — proof that results are in spawn order
                    let _ = tracer.emit_raw(&format!(
                        r#"{{"t":"spawn_ordered_complete","count":{}}}"#,
                        results.len()
                    ));

                    Ok(Val::List(results))
                }
                _ => bail!("promise.spawn_ordered expects a list of functions"),
            }
        }
        Builtin::PromiseAwait => match args.as_slice() {
            [Val::Promise(slot, spawn_id, trace_path)] => {
                let spawn_id = spawn_id.clone();
                let trace_path = trace_path.clone();
                loop {
                    let done = slot.lock().unwrap().is_some();
                    if done {
                        let result = slot.lock().unwrap().take().unwrap();
                        let val = result.map_err(|e| anyhow::anyhow!("{}", e))?;
                        // Read child trace and compute its digest
                        let run_digest = if trace_path.exists() {
                            let trace_bytes = fs::read(&trace_path).unwrap_or_default();
                            let digest = format!("sha256:{}", hex_lower(&{
                                let mut h = NativeSha256::new();
                                h.update(&trace_bytes);
                                h.finalize()
                            }));
                            let _ = fs::remove_file(&trace_path);
                            digest
                        } else {
                            "sha256:no-trace".to_string()
                        };
                        // Compute result digest
                        let result_digest = if let Some(j) = val.to_json() {
                            let s = json_to_string(&j);
                            format!("sha256:{}", hex_lower(&{
                                let mut h = NativeSha256::new();
                                h.update(s.as_bytes());
                                h.finalize()
                            }))
                        } else {
                            "sha256:no-result".to_string()
                        };
                        // Record child receipt in parent trace
                        tracer.child_receipt(&spawn_id, &run_digest, &result_digest)?;
                        return Ok(val);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            _ => bail!("promise.await expects a promise"),
        }
        Builtin::BigFromInt => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Big(Box::new(BigInt::from(*n)))),
            _ => bail!("bigint.from_int expects int"),
        }
        Builtin::BigFromStr => match args.as_slice() {
            [Val::Text(s)] => {
                let b: BigInt = s.parse().map_err(|_| anyhow::anyhow!("bigint.from_str: invalid {s}"))?;
                Ok(Val::Big(Box::new(b)))
            }
            _ => bail!("bigint.from_str expects text"),
        }
        Builtin::BigAdd => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => Ok(Val::Big(Box::new(*a.clone() + *b.clone()))),
            _ => bail!("bigint.add expects (bigint, bigint)"),
        }
        Builtin::BigSub => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => Ok(Val::Big(Box::new(*a.clone() - *b.clone()))),
            _ => bail!("bigint.sub expects (bigint, bigint)"),
        }
        Builtin::BigMul => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => Ok(Val::Big(Box::new(*a.clone() * *b.clone()))),
            _ => bail!("bigint.mul expects (bigint, bigint)"),
        }
        Builtin::BigDiv => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => {
                if b.as_ref() == &BigInt::zero() { bail!("bigint.div: division by zero"); }
                Ok(Val::Big(Box::new(*a.clone() / *b.clone())))
            }
            _ => bail!("bigint.div expects (bigint, bigint)"),
        }
        Builtin::BigMod => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => {
                if b.as_ref() == &BigInt::zero() { bail!("bigint.mod: division by zero"); }
                Ok(Val::Big(Box::new(*a.clone() % *b.clone())))
            }
            _ => bail!("bigint.mod expects (bigint, bigint)"),
        }
        Builtin::BigPow => match args.as_slice() {
            [Val::Big(a), Val::Int(n)] => {
                if *n < 0 { bail!("bigint.pow: negative exponent"); }
                Ok(Val::Big(Box::new(a.pow(*n as u32))))
            }
            _ => bail!("bigint.pow expects (bigint, int)"),
        }
        Builtin::BigToStr => match args.as_slice() {
            [Val::Big(a)] => Ok(Val::Text(a.to_string())),
            _ => bail!("bigint.to_str expects bigint"),
        }
        Builtin::BigEq => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => Ok(Val::Bool(a == b)),
            _ => bail!("bigint.eq expects (bigint, bigint)"),
        }
        Builtin::BigLt => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => Ok(Val::Bool(a < b)),
            _ => bail!("bigint.lt expects (bigint, bigint)"),
        }
        Builtin::BigGt => match args.as_slice() {
            [Val::Big(a), Val::Big(b)] => Ok(Val::Bool(a > b)),
            _ => bail!("bigint.gt expects (bigint, bigint)"),
        }
        Builtin::IntToStrPadded => match args.as_slice() {
            [Val::Int(n), Val::Int(width), Val::Text(pad)] => {
                let s = n.to_string();
                let w = *width as usize;
                let c = pad.chars().next().unwrap_or(' ');
                if s.len() >= w {
                    Ok(Val::Text(s))
                } else {
                    let padding: String = std::iter::repeat(c).take(w - s.len()).collect();
                    Ok(Val::Text(format!("{}{}", padding, s)))
                }
            }
            _ => bail!("ERROR_BADARG int.to_str_padded expects (int, width, pad_char)"),
        }
        Builtin::UuidV4 => {
            Ok(Val::Text(uuid::Uuid::new_v4().to_string()))
        }
        Builtin::UuidValidate => match args.as_slice() {
            [Val::Text(s)] => Ok(Val::Bool(uuid::Uuid::parse_str(s).is_ok())),
            _ => bail!("ERROR_BADARG uuid.validate expects text"),
        }
        Builtin::DateTimeNow => {
            Ok(Val::Int(chrono::Utc::now().timestamp()))
        }
        Builtin::DateTimeFormat => match args.as_slice() {
            [Val::Int(ts), Val::Text(fmt)] => {
                let dt = chrono::DateTime::from_timestamp(*ts, 0)
                    .ok_or_else(|| anyhow::anyhow!("datetime.format: invalid timestamp"))?;
                Ok(Val::Text(dt.format(fmt).to_string()))
            }
            _ => bail!("ERROR_BADARG datetime.format expects (int, text)"),
        }
        Builtin::DateTimeParse => match args.as_slice() {
            [Val::Text(s), Val::Text(fmt)] => {
                let dt = chrono::NaiveDateTime::parse_from_str(s, fmt)
                    .map_err(|e| anyhow::anyhow!("datetime.parse: {}", e))?;
                Ok(Val::Int(dt.and_utc().timestamp()))
            }
            _ => bail!("ERROR_BADARG datetime.parse expects (text, fmt)"),
        }
        Builtin::DateTimeAdd => match args.as_slice() {
            [Val::Int(ts), Val::Text(unit), Val::Int(n)] => {
                let dt = chrono::DateTime::from_timestamp(*ts, 0)
                    .ok_or_else(|| anyhow::anyhow!("datetime.add: invalid timestamp"))?;
                let result = match unit.as_str() {
                    "seconds" => dt + chrono::Duration::seconds(*n),
                    "minutes" => dt + chrono::Duration::minutes(*n),
                    "hours"   => dt + chrono::Duration::hours(*n),
                    "days"    => dt + chrono::Duration::days(*n),
                    _ => bail!("datetime.add: unknown unit {unit}"),
                };
                Ok(Val::Int(result.timestamp()))
            }
            _ => bail!("ERROR_BADARG datetime.add expects (ts, unit, n)"),
        }
        Builtin::DateTimeSub => match args.as_slice() {
            [Val::Int(a), Val::Int(b)] => Ok(Val::Int(a - b)),
            _ => bail!("ERROR_BADARG datetime.diff expects (ts, ts)"),
        }
        Builtin::DateTimeField => match args.as_slice() {
            [Val::Int(ts), Val::Text(field)] => {
                use chrono::Datelike;
                use chrono::Timelike;
                let dt = chrono::DateTime::from_timestamp(*ts, 0)
                    .ok_or_else(|| anyhow::anyhow!("datetime.field: invalid timestamp"))?;
                match field.as_str() {
                    "year"   => Ok(Val::Int(dt.year() as i64)),
                    "month"  => Ok(Val::Int(dt.month() as i64)),
                    "day"    => Ok(Val::Int(dt.day() as i64)),
                    "hour"   => Ok(Val::Int(dt.hour() as i64)),
                    "minute" => Ok(Val::Int(dt.minute() as i64)),
                    "second" => Ok(Val::Int(dt.second() as i64)),
                    _ => bail!("datetime.field: unknown field {field}"),
                }
            }
            _ => bail!("ERROR_BADARG datetime.field expects (ts, field)"),
        }
        Builtin::FloatToStrFixed => match args.as_slice() {
            [Val::Float(f), Val::Int(decimals)] => {
                Ok(Val::Text(format!("{:.prec$}", f, prec = *decimals as usize)))
            }
            [Val::Int(n), Val::Int(decimals)] => {
                Ok(Val::Text(format!("{:.prec$}", *n as f64, prec = *decimals as usize)))
            }
            _ => bail!("ERROR_BADARG float.to_str_fixed expects (float, int)"),
        }
        Builtin::MathAsin => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.asin())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).asin())),
            _ => bail!("ERROR_BADARG math.asin expects number"),
        }
        Builtin::MathAcos => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.acos())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).acos())),
            _ => bail!("ERROR_BADARG math.acos expects number"),
        }
        Builtin::MathAtan => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.atan())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).atan())),
            _ => bail!("ERROR_BADARG math.atan expects number"),
        }
        Builtin::MathLog10 => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.log10())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).log10())),
            _ => bail!("ERROR_BADARG math.log10 expects number"),
        }
        Builtin::ListZipWith => match args.as_slice() {
            [Val::List(a), Val::List(b), f] => {
                let len = a.len().min(b.len());
                let mut out = Vec::new();
                for i in 0..len {
                    out.push(call(f.clone(), vec![a[i].clone(), b[i].clone()], tracer, loader)?);
                }
                Ok(Val::List(out))
            }
            _ => bail!("ERROR_BADARG list.zip_with expects (list, list, fn)"),
        }
        Builtin::ListChunk => match args.as_slice() {
            [Val::List(items), Val::Int(n)] if *n > 0 => {
                let chunks: Vec<Val> = items.chunks(*n as usize)
                    .map(|c| Val::List(c.to_vec()))
                    .collect();
                Ok(Val::List(chunks))
            }
            _ => bail!("ERROR_BADARG list.chunk expects (list, int>0)"),
        }
        Builtin::ListSortBy => match args.as_slice() {
            [Val::List(items), f] => {
                let mut items2 = items.clone();
                let f = f.clone();
                let mut err: Option<anyhow::Error> = None;
                items2.sort_by(|a, b| {
                    if err.is_some() { return std::cmp::Ordering::Equal; }
                    let mut t = Tracer::new(std::path::Path::new("/tmp"), &std::path::Path::new("/tmp/fard_sort.ndjson")).unwrap();
                    let mut l = ModuleLoader::new(std::path::Path::new("/tmp"));
                    match call(f.clone(), vec![a.clone(), b.clone()], &mut t, &mut l) {
                        Ok(Val::Int(n)) => n.cmp(&0),
                        Ok(_) => { err = Some(anyhow::anyhow!("sort_by fn must return int")); std::cmp::Ordering::Equal }
                        Err(e) => { err = Some(e); std::cmp::Ordering::Equal }
                    }
                });
                if let Some(e) = err { return Err(e); }
                Ok(Val::List(items2))
            }
            _ => bail!("ERROR_BADARG list.sort_by expects (list, fn)"),
        }
        Builtin::SetNew => Ok(Val::List(vec![])),
        Builtin::SetAdd => match args.as_slice() {
            [Val::List(s), v] => {
                let mut s2 = s.clone();
                if !s2.iter().any(|x| val_eq(x, v)) { s2.push(v.clone()); s2.sort_by(|a,b| format!("{:?}",a).cmp(&format!("{:?}",b))); }
                Ok(Val::List(s2))
            }
            _ => bail!("ERROR_BADARG set.add expects (set, val)"),
        }
        Builtin::SetRemove => match args.as_slice() {
            [Val::List(s), v] => {
                Ok(Val::List(s.iter().filter(|x| !val_eq(x, v)).cloned().collect()))
            }
            _ => bail!("ERROR_BADARG set.remove expects (set, val)"),
        }
        Builtin::SetHas => match args.as_slice() {
            [Val::List(s), v] => Ok(Val::Bool(s.iter().any(|x| val_eq(x, v)))),
            _ => bail!("ERROR_BADARG set.has expects (set, val)"),
        }
        Builtin::SetUnion => match args.as_slice() {
            [Val::List(a), Val::List(b)] => {
                let mut s = a.clone();
                for v in b { if !s.iter().any(|x| val_eq(x, v)) { s.push(v.clone()); } }
                s.sort_by(|a,b| format!("{:?}",a).cmp(&format!("{:?}",b)));
                Ok(Val::List(s))
            }
            _ => bail!("ERROR_BADARG set.union expects (set, set)"),
        }
        Builtin::SetIntersect => match args.as_slice() {
            [Val::List(a), Val::List(b)] => {
                Ok(Val::List(a.iter().filter(|v| b.iter().any(|x| val_eq(x, v))).cloned().collect()))
            }
            _ => bail!("ERROR_BADARG set.intersect expects (set, set)"),
        }
        Builtin::SetDiff => match args.as_slice() {
            [Val::List(a), Val::List(b)] => {
                Ok(Val::List(a.iter().filter(|v| !b.iter().any(|x| val_eq(x, v))).cloned().collect()))
            }
            _ => bail!("ERROR_BADARG set.diff expects (set, set)"),
        }
        Builtin::SetToList => match args.as_slice() {
            [Val::List(s)] => Ok(Val::List(s.clone())),
            _ => bail!("ERROR_BADARG set.to_list expects set"),
        }
        Builtin::SetFromList => match args.as_slice() {
            [Val::List(items)] => {
                let mut s: Vec<Val> = Vec::new();
                for v in items { if !s.iter().any(|x| val_eq(x, v)) { s.push(v.clone()); } }
                s.sort_by(|a,b| format!("{:?}",a).cmp(&format!("{:?}",b)));
                Ok(Val::List(s))
            }
            _ => bail!("ERROR_BADARG set.from_list expects list"),
        }
        Builtin::SetSize => match args.as_slice() {
            [Val::List(s)] => Ok(Val::Int(s.len() as i64)),
            _ => bail!("ERROR_BADARG set.size expects set"),
        }
        Builtin::MapDelete => match args.as_slice() {
            [Val::Record(m), Val::Text(k)] => {
                let mut m2 = m.clone();
                m2.remove(k);
                Ok(Val::Record(m2))
            }
            _ => bail!("ERROR_BADARG map.delete expects (map, key)"),
        }
        Builtin::MapEntries => match args.as_slice() {
            [Val::Record(m)] => {
                let entries: Vec<Val> = m.iter().map(|(k, v)| {
                    let mut rec = BTreeMap::new();
                    rec.insert("key".to_string(), Val::Text(k.clone()));
                    rec.insert("value".to_string(), v.clone());
                    Val::Record(rec)
                }).collect();
                Ok(Val::List(entries))
            }
            _ => bail!("ERROR_BADARG map.entries expects a map"),
        }
        Builtin::Base64Encode => match args.as_slice() {
            [Val::Bytes(b)] => {
                use base64::Engine;
                Ok(Val::Text(base64::engine::general_purpose::STANDARD.encode(b)))
            }
            [Val::Text(s)] => {
                use base64::Engine;
                Ok(Val::Text(base64::engine::general_purpose::STANDARD.encode(s.as_bytes())))
            }
            _ => bail!("ERROR_BADARG base64.encode expects bytes or text"),
        }
        Builtin::Base64Decode => match args.as_slice() {
            [Val::Text(s)] => {
                use base64::Engine;
                let b = base64::engine::general_purpose::STANDARD.decode(s.as_bytes())
                    .map_err(|e| anyhow::anyhow!("base64.decode: {}", e))?;
                Ok(Val::Bytes(b))
            }
            _ => bail!("ERROR_BADARG base64.decode expects text"),
        }
        Builtin::CsvParse => match args.as_slice() {
            [Val::Text(s)] => {
                let mut rdr = csv::ReaderBuilder::new().has_headers(false).from_reader(s.as_bytes());
                let mut rows: Vec<Val> = Vec::new();
                for result in rdr.records() {
                    let record = result.map_err(|e| anyhow::anyhow!("csv.parse: {}", e))?;
                    let fields: Vec<Val> = record.iter().map(|f| Val::Text(f.to_string())).collect();
                    rows.push(Val::List(fields));
                }
                Ok(Val::List(rows))
            }
            _ => bail!("ERROR_BADARG csv.parse expects text"),
        }
        Builtin::CsvEncode => match args.as_slice() {
            [Val::List(rows)] => {
                let mut wtr = csv::WriterBuilder::new().from_writer(vec![]);
                for row in rows {
                    match row {
                        Val::List(fields) => {
                            let strs: Vec<String> = fields.iter().map(|f| match f {
                                Val::Text(s) => s.clone(),
                                Val::Int(n) => n.to_string(),
                                Val::Float(f) => f.to_string(),
                                Val::Bool(b) => b.to_string(),
                                _ => String::new(),
                            }).collect();
                            wtr.write_record(&strs).map_err(|e| anyhow::anyhow!("csv.encode: {}", e))?;
                        }
                        _ => bail!("csv.encode: each row must be a list"),
                    }
                }
                let data = wtr.into_inner().map_err(|e| anyhow::anyhow!("csv.encode: {}", e))?;
                Ok(Val::Text(String::from_utf8_lossy(&data).to_string()))
            }
            _ => bail!("ERROR_BADARG csv.encode expects list of lists"),
        }
        Builtin::ReMatch => match args.as_slice() {
            [Val::Text(pattern), Val::Text(text)] => {
                let re = regex::Regex::new(pattern).map_err(|e| anyhow::anyhow!("re.match: {}", e))?;
                Ok(Val::Bool(re.is_match(text)))
            }
            _ => bail!("ERROR_BADARG re.match expects (pattern, text)"),
        }
        Builtin::ReFind => match args.as_slice() {
            [Val::Text(pattern), Val::Text(text)] => {
                let re = regex::Regex::new(pattern).map_err(|e| anyhow::anyhow!("re.find: {}", e))?;
                match re.find(text) {
                    Some(m) => { let mut rec = BTreeMap::new(); rec.insert("some".to_string(), Val::Text(m.as_str().to_string())); Ok(Val::Record(rec)) }
                    None    => { let mut rec = BTreeMap::new(); rec.insert("none".to_string(), Val::Unit); Ok(Val::Record(rec)) }
                }
            }
            _ => bail!("ERROR_BADARG re.find expects (pattern, text)"),
        }
        Builtin::ReFindAll => match args.as_slice() {
            [Val::Text(pattern), Val::Text(text)] => {
                let re = regex::Regex::new(pattern).map_err(|e| anyhow::anyhow!("re.find_all: {}", e))?;
                let matches: Vec<Val> = re.find_iter(text).map(|m| Val::Text(m.as_str().to_string())).collect();
                Ok(Val::List(matches))
            }
            _ => bail!("ERROR_BADARG re.find_all expects (pattern, text)"),
        }
        Builtin::ReSplit => match args.as_slice() {
            [Val::Text(pattern), Val::Text(text)] => {
                let re = regex::Regex::new(pattern).map_err(|e| anyhow::anyhow!("re.split: {}", e))?;
                let parts: Vec<Val> = re.split(text).map(|s| Val::Text(s.to_string())).collect();
                Ok(Val::List(parts))
            }
            _ => bail!("ERROR_BADARG re.split expects (pattern, text)"),
        }
        Builtin::ReReplace => match args.as_slice() {
            [Val::Text(pattern), Val::Text(text), Val::Text(replacement)] => {
                let re = regex::Regex::new(pattern).map_err(|e| anyhow::anyhow!("re.replace: {}", e))?;
                Ok(Val::Text(re.replace_all(text, replacement.as_str()).to_string()))
            }
            _ => bail!("ERROR_BADARG re.replace expects (pattern, text, replacement)"),
        }
        Builtin::EnvGet => match args.as_slice() {
            [Val::Text(key)] => {
                match std::env::var(key.as_str()) {
                    Ok(v) => { let mut m = BTreeMap::new(); m.insert("some".to_string(), Val::Text(v)); Ok(Val::Record(m)) }
                    Err(_) => { let mut m = BTreeMap::new(); m.insert("none".to_string(), Val::Unit); Ok(Val::Record(m)) }
                }
            }
            _ => bail!("ERROR_BADARG env.get expects text key"),
        }
        Builtin::EnvArgs => {
            let args_list: Vec<Val> = std::env::args().map(|a| Val::Text(a)).collect();
            Ok(Val::List(args_list))
        }
        Builtin::ProcessSpawn => {
            if args.len() < 2 { bail!("ERROR_BADARG process.spawn expects (text, list) or (text, list, stdin_text)"); }
            let cmd = match &args[0] { Val::Text(s) => s.clone(), _ => bail!("ERROR_BADARG process.spawn: cmd must be text") };
            let cmd_args = match &args[1] { Val::List(l) => l.clone(), _ => bail!("ERROR_BADARG process.spawn: args must be list") };
            let stdin_text: Option<String> = if args.len() >= 3 {
                match &args[2] { Val::Text(s) => Some(s.clone()), Val::Unit => None, _ => None }
            } else { None };
            let str_args: Vec<String> = cmd_args.iter().map(|a| match a {
                Val::Text(s) => Ok(s.clone()),
                _ => Err(anyhow::anyhow!("process.spawn: args must be text")),
            }).collect::<Result<Vec<_>>>()?;
            let mut child = std::process::Command::new(cmd.as_str())
                .args(&str_args)
                .stdin(if stdin_text.is_some() { std::process::Stdio::piped() } else { std::process::Stdio::null() })
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| anyhow::anyhow!("ERROR_IO process.spawn: {}", e))?;
            if let Some(text) = stdin_text {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(text.as_bytes());
                }
            }
            let out = child.wait_with_output().map_err(|e| anyhow::anyhow!("ERROR_IO process.spawn wait: {}", e))?;
            let mut m = BTreeMap::new();
            m.insert("stdout".to_string(), Val::Text(String::from_utf8_lossy(&out.stdout).to_string()));
            m.insert("stderr".to_string(), Val::Text(String::from_utf8_lossy(&out.stderr).to_string()));
            m.insert("code".to_string(), Val::Int(out.status.code().unwrap_or(-1) as i64));
            Ok(Val::Record(m))
        }
        Builtin::ProcessExit => match args.as_slice() {
            [Val::Int(code)] => { std::process::exit(*code as i32); }
            _ => { std::process::exit(0); }
        }
        Builtin::MathSin => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.sin())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).sin())),
            _ => bail!("ERROR_BADARG math.sin expects number"),
        }
        Builtin::MathCos => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.cos())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).cos())),
            _ => bail!("ERROR_BADARG math.cos expects number"),
        }
        Builtin::MathTan => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Float(f.tan())),
            [Val::Int(n)] => Ok(Val::Float((*n as f64).tan())),
            _ => bail!("ERROR_BADARG math.tan expects number"),
        }
        Builtin::MathAtan2 => match args.as_slice() {
            [Val::Float(y), Val::Float(x)] => Ok(Val::Float(y.atan2(*x))),
            [Val::Int(y), Val::Int(x)] => Ok(Val::Float((*y as f64).atan2(*x as f64))),
            _ => bail!("ERROR_BADARG math.atan2 expects (y, x)"),
        }
        Builtin::IntToHex => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Text(format!("{:x}", n))),
            _ => bail!("ERROR_BADARG int.to_hex expects int"),
        }
        Builtin::IntToBin => match args.as_slice() {
            [Val::Int(n)] => Ok(Val::Text(format!("{:b}", n))),
            _ => bail!("ERROR_BADARG int.to_bin expects int"),
        }
        Builtin::FloatIsInf => match args.as_slice() {
            [Val::Float(f)] => Ok(Val::Bool(f.is_infinite())),
            _ => bail!("ERROR_BADARG float.is_inf expects float"),
        }
        Builtin::TypeOf => match args.as_slice() {
            [Val::Unit]      => Ok(Val::Text("unit".to_string())),
            [Val::Bool(_)]   => Ok(Val::Text("bool".to_string())),
            [Val::Int(_)]    => Ok(Val::Text("int".to_string())),
            [Val::Float(_)]  => Ok(Val::Text("float".to_string())),
            [Val::Text(_)]   => Ok(Val::Text("text".to_string())),
            [Val::Bytes(_)]  => Ok(Val::Text("bytes".to_string())),
            [Val::List(_)]   => Ok(Val::Text("list".to_string())),
            [Val::Record(_)] => Ok(Val::Text("record".to_string())),
            [Val::Func(_)]   => Ok(Val::Text("func".to_string())),
            [Val::Builtin(_)]=> Ok(Val::Text("func".to_string())),
            [Val::BoundMethod(_,_)] => Ok(Val::Text("func".to_string())),
            _ => bail!("ERROR_BADARG type_of expects 1 arg"),
        }
        Builtin::CastText => match args.as_slice() {
            [Val::Int(n)] => {
                // Convert unicode codepoint to single-char string
                let c = char::from_u32(*n as u32).unwrap_or('?');
                Ok(Val::Text(c.to_string()))
            }
            [Val::Float(f)] => Ok(Val::Text(f.to_string())),
            [Val::Bool(b)] => Ok(Val::Text(b.to_string())),
            [Val::Text(s)] => Ok(Val::Text(s.clone())),
            _ => bail!("ERROR_BADARG cast.text expects one arg"),
        }
        Builtin::CastFloat => {
            if args.len() != 1 { bail!("ERROR_BADARG float() expects 1 arg"); }
            match &args[0] {
                Val::Float(f) => Ok(Val::Float(*f)),
                Val::Int(n)   => Ok(Val::Float(*n as f64)),
                Val::Text(s)  => Ok(Val::Float(s.parse::<f64>().map_err(|_| anyhow!("ERROR_BADARG float() cannot parse '{}'", s))?)),
                _ => bail!("ERROR_BADARG float() expects int, float, or string"),
            }
        }
        Builtin::CastInt => {
            if args.len() != 1 { bail!("ERROR_BADARG int() expects 1 arg"); }
            match &args[0] {
                Val::Int(n)   => Ok(Val::Int(*n)),
                Val::Float(f) => Ok(Val::Int(*f as i64)),
                Val::Text(s)  => Ok(Val::Int(s.parse::<i64>().map_err(|_| anyhow!("ERROR_BADARG int() cannot parse '{}'", s))?)),
                _ => bail!("ERROR_BADARG int() expects int, float, or string"),
            }
        }
        Builtin::LinalgRelu => {
            if args.len() != 1 { bail!("ERROR_BADARG linalg.relu expects 1 arg"); }
            match &args[0] {
                Val::List(xs) => {
                    let v: Result<Vec<Val>> = xs.iter().map(|x| {
                        let f = val_to_f64_linalg(x)?;
                        Ok(fv(f.max(0.0)))
                    }).collect();
                    Ok(Val::List(v?))
                }
                _ => bail!("ERROR_BADARG relu expects list"),
            }
        }
        Builtin::LinalgSoftmax => {
            if args.len() != 1 { bail!("ERROR_BADARG linalg.softmax expects 1 arg"); }
            match &args[0] {
                Val::List(xs) => {
                    let fs: Result<Vec<f64>> = xs.iter().map(|x| val_to_f64_linalg(x)).collect();
                    let fs = fs?;
                    let max = fs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let exps: Vec<f64> = fs.iter().map(|&x| (x - max).exp()).collect();
                    let sum: f64 = exps.iter().sum();
                    Ok(Val::List(exps.iter().map(|&e| Val::Float(e / sum)).collect()))
                }
                _ => bail!("ERROR_BADARG softmax expects list"),
            }
        }
        Builtin::LinalgArgmax => {
            if args.len() != 1 { bail!("ERROR_BADARG linalg.argmax expects 1 arg"); }
            match &args[0] {
                Val::List(xs) => {
                    if xs.is_empty() { bail!("ERROR_BADARG argmax on empty list"); }
                    let mut best_i = 0usize;
                    let mut best_v = f64::NEG_INFINITY;
                    for (i, x) in xs.iter().enumerate() {
                        let v = val_to_f64_linalg(x)?;
                        if v > best_v { best_v = v; best_i = i; }
                    }
                    Ok(Val::Int(best_i as i64))
                }
                _ => bail!("ERROR_BADARG argmax expects list"),
            }
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
            let m = vl_to_mat(&args[0])?;
            let n = m.len();
            if n == 0 { let mut m = BTreeMap::new(); m.insert("vals".into(), Val::List(vec![])); m.insert("vecs".into(), Val::List(vec![])); return Ok(Val::Record(m)); }
            let flat: Vec<f64> = m.iter().flat_map(|r| r.iter().cloned()).collect();
            let (eigenvalues, eigenvecs) = valuecore::linalg::eigh(&flat, n);
            let vals: Vec<Val> = eigenvalues.iter().map(|&v| fv(v)).collect();
            let vecs: Vec<Val> = eigenvecs.iter().map(|row| Val::List(row.iter().map(|&v| fv(v)).collect())).collect();
            { let mut m = BTreeMap::new(); m.insert("vals".into(), Val::List(vals)); m.insert("vecs".into(), Val::List(vecs)); Ok(Val::Record(m)) }
        }
        Builtin::LinalgVecExp => {
            let xs: Vec<f64> = vl_to_vec(&args[0])?;
            Ok(Val::List(xs.iter().map(|x| fv(x.exp())).collect()))
        }
        Builtin::LinalgVecLog => {
            let xs: Vec<f64> = vl_to_vec(&args[0])?;
            Ok(Val::List(xs.iter().map(|x| fv(x.ln())).collect()))
        }
        Builtin::LinalgVecSum => {
            let xs: Vec<f64> = vl_to_vec(&args[0])?;
            Ok(fv(xs.iter().sum()))
        }
        Builtin::LinalgVecMax => {
            let xs: Vec<f64> = vl_to_vec(&args[0])?;
            if xs.is_empty() { bail!("ERROR_BADARG vec_max empty"); }
            Ok(fv(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)))
        }
        Builtin::LinalgVecMul => {
            let a: Vec<f64> = vl_to_vec(&args[0])?;
            let b: Vec<f64> = vl_to_vec(&args[1])?;
            if a.len() != b.len() { bail!("ERROR_BADARG vec_mul length mismatch"); }
            Ok(Val::List(a.iter().zip(b.iter()).map(|(x,y)| fv(x*y)).collect()))
        }
        Builtin::LinalgVecRelu => {
            let xs: Vec<f64> = vl_to_vec(&args[0])?;
            Ok(Val::List(xs.iter().map(|x| fv(x.max(0.0))).collect()))
        }
        Builtin::LinalgVecReluGrad => {
            let x:  Vec<f64> = vl_to_vec(&args[0])?;
            let dv: Vec<f64> = vl_to_vec(&args[1])?;
            if x.len() != dv.len() { bail!("ERROR_BADARG vec_relu_grad length mismatch"); }
            Ok(Val::List(x.iter().zip(dv.iter()).map(|(xi,di)| fv(if *xi > 0.0 { *di } else { 0.0 })).collect()))
        }
        Builtin::LinalgSoftmaxGrad => {
            let s: Vec<f64> = vl_to_vec(&args[0])?;
            let y = match &args[1] { Val::Int(i) => *i as usize, _ => bail!("ERROR_BADARG softmax_grad expects int label") };
            let mut g = s.clone();
            if y < g.len() { g[y] -= 1.0; }
            Ok(Val::List(g.iter().map(|x| fv(*x)).collect()))
        }
        Builtin::LinalgCrossEntropy => {
            let s: Vec<f64> = vl_to_vec(&args[0])?;
            let y = match &args[1] { Val::Int(i) => *i as usize, _ => bail!("ERROR_BADARG cross_entropy expects int label") };
            if y >= s.len() { bail!("ERROR_BADARG cross_entropy label out of range"); }
            Ok(fv(-(s[y].max(1e-15).ln())))
        }
        Builtin::LinalgOuter => {
            let a: Vec<f64> = vl_to_vec(&args[0])?;
            let b: Vec<f64> = vl_to_vec(&args[1])?;
            Ok(Val::List(a.iter().map(|ai| Val::List(b.iter().map(|bj| fv(ai*bj)).collect())).collect()))
        }
        Builtin::LinalgMatMulVecGrad => {
            let w = vl_to_mat(&args[0])?;
            let x: Vec<f64> = vl_to_vec(&args[1])?;
            let dout: Vec<f64> = vl_to_vec(&args[2])?;
            let dw: Vec<Val> = dout.iter().map(|di| Val::List(x.iter().map(|xj| fv(di*xj)).collect())).collect();
            let m = w.len(); let n = if m>0 { w[0].len() } else { 0 };
            let mut dx = vec![0.0f64; n];
            for i in 0..m { for j in 0..n { dx[j] += w[i][j] * dout[i]; } }
            let mut rec = BTreeMap::new();
            rec.insert("dW".into(), Val::List(dw));
            rec.insert("dx".into(), Val::List(dx.iter().map(|x| fv(*x)).collect()));
            Ok(Val::Record(rec))
        }
        Builtin::LinalgVecScalarAdd => {
            let v: Vec<f64> = vl_to_vec(&args[0])?;
            let s = val_to_f64_linalg(&args[1])?;
            Ok(Val::List(v.iter().map(|x| fv(x+s)).collect()))
        }
        Builtin::LinalgMatRowSum => {
            let m = vl_to_mat(&args[0])?;
            Ok(Val::List(m.iter().map(|row| fv(row.iter().sum())).collect()))
        }
    }
}

fn fv(f: f64) -> Val { Val::Bytes(f.to_le_bytes().to_vec()) }
fn val_to_f64_linalg(v: &Val) -> Result<f64> {
    match v {
        Val::Bytes(b) if b.len() == 8 => {
            let arr: [u8;8] = b.as_slice().try_into().unwrap();
            Ok(f64::from_le_bytes(arr))
        }
        Val::Float(f) => Ok(*f),
        Val::Int(n)   => Ok(*n as f64),
        _ => bail!("ERROR_BADARG linalg: expected float value, got {:?}", v),
    }
}

fn fb64_1(args: &[Val]) -> anyhow::Result<f64> {
    match args.first() {
        Some(Val::Bytes(b)) if b.len() == 8 => {
            let arr: [u8;8] = b.as_slice().try_into().unwrap();
            Ok(f64::from_le_bytes(arr))
        }
        Some(Val::Float(f)) => Ok(*f),
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

fn vl_to_vec(v: &Val) -> anyhow::Result<Vec<f64>> {
    match v {
        Val::List(xs) => xs.iter().map(|x| val_to_f64_linalg(x)).collect(),
        Val::Bytes(b) => {
            if b.len() % 8 != 0 { anyhow::bail!("ERROR_BADARG vec bytes not multiple of 8"); }
            b.chunks(8).map(|c| {
                let arr: [u8;8] = c.try_into().unwrap();
                Ok(f64::from_le_bytes(arr))
            }).collect()
        }
        _ => anyhow::bail!("ERROR_BADARG vl_to_vec: expected list"),
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
        let v = json_from_slice(&bytes)?;
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
            m.insert("id".to_string(), J::Int(n.id as i64));
            m.insert("spec".to_string(), J::Str(n.spec.clone()));
            m.insert(
                "kind".to_string(),
                J::Str(
                    match n.kind {
                        ModKind::Std => "std",
                        ModKind::Pkg => "pkg",
                        ModKind::Rel => "rel",
                    }
                    .to_string(),
                ),
            );
            if let Some(p) = &n.path {
                m.insert("path".to_string(), J::Str(p.clone()));
            }
            if let Some(d) = &n.digest {
                m.insert("digest".to_string(), J::Str(d.clone()));
            }
            ns.push(J::Object(m));
        }
        let mut es: Vec<J> = Vec::new();
        for e in &self.edges {
            let mut m = Map::new();
            m.insert("from".to_string(), J::Int(e.from as i64));
            m.insert("to".to_string(), J::Int(e.to as i64));
            m.insert("kind".to_string(), J::Str(e.kind.clone()));
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
    enforce_lockfile: bool,
    graph: ModuleGraph,
    current: Option<usize>,
    pkg_deps: HashMap<String, String>, // short name -> "name@version"
}
impl ModuleLoader {
    fn new(root: &Path) -> Self {
        Self {
            root_dir: root.to_path_buf(),
            registry_dir: None,
            cache: HashMap::new(),
            stack: Vec::new(),
            lock: None,
            enforce_lockfile: false,
            graph: ModuleGraph::new(),
            current: None,
            pkg_deps: HashMap::new(),
        }
    }
    fn load_fard_toml(&mut self, toml_path: &Path) {
        if let Ok(src) = fs::read_to_string(toml_path) {
            // Parse [deps] section: name = "name@version"
            let mut in_deps = false;
            for line in src.lines() {
                let line = line.trim();
                if line == "[deps]" { in_deps = true; continue; }
                if line.starts_with('[') { in_deps = false; continue; }
                if in_deps && line.contains('=') {
                    let mut parts = line.splitn(2, '=');
                    let k = parts.next().unwrap_or("").trim().to_string();
                    let v = parts.next().unwrap_or("").trim().trim_matches('"').to_string();
                    if !k.is_empty() && !v.is_empty() {
                        self.pkg_deps.insert(k, v);
                    }
                }
            }
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

        // Dialect detection: if source starts with `module `, route to fardlang evaluator.
        let trimmed = src.trim_start();
        if trimmed.starts_with("module ") || trimmed.starts_with("module
") {
            return self.eval_main_fardlang(&src, main_path, tracer);
        }

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

    fn eval_main_fardlang(&mut self, src: &str, main_path: &Path, _tracer: &mut Tracer) -> Result<Val> {
        let raw = src.as_bytes();
        let m = fardlang_parse_module(raw)
            .with_context(|| format!("ERROR_PARSE fardlang: {}", main_path.display()))?;
        fardlang_check_module(&m)
            .context("ERROR_CHECK fardlang")?;

        let mut fns: std::collections::BTreeMap<String, fardlang::ast::FnDecl> =
            std::collections::BTreeMap::new();
        for d in &m.fns {
            fns.insert(d.name.clone(), d.clone());
        }
        let main_decl = fns.get("main").cloned()
            .ok_or_else(|| anyhow!("ERROR_EVAL fardlang: missing main fn"))?;

        let mut env = FardlangEnv::with_fns(fns);
        apply_imports(&mut env, &m.imports);
        // Also handle string-path imports: import "std/list" as list
        // These land in source_imports; route std/* through std_aliases().
        {
            let table = fardlang::eval::std_aliases();
            for si in &m.source_imports {
                let path = si.path.trim_start_matches("std/");
                let alias = if si.alias.is_empty() { path } else { si.alias.as_str() };
                if let Some(mod_map) = table.get(path) {
                    for (fn_name, builtin) in mod_map {
                        env.aliases.insert(format!("{}.{}", alias, fn_name), builtin.clone());
                    }
                }
            }
        }
        let vcore = eval_block(&main_decl.body, &mut env)
            .context("ERROR_EVAL fardlang")?;

        // Convert valuecore::Val -> fardrun::Val directly (no v0 wire encoding)
        Ok(vcore_to_fardrun(vcore))
    }
    fn set_vm_fns_ptr() {
        VM_FNS.with(|cell| {
            let ptr: *const Vec<VmCompiledFn> = &*cell.borrow();
            VM_FNS_PTR.with(|p| p.set(ptr));
        });
    }

    fn eval_items(
        &mut self,
        items: Vec<Item>,
        env: &mut Env,
        tracer: &mut Tracer,
        here: &Path,
    ) -> Result<Val> {
        let mut exports: Option<Vec<String>> = None;
        let mut last: Val = Val::Unit;
        for it in items {
            match it {
                Item::Import(path, alias) => {
                    let ex = self.load_module(&path, here, tracer)?;
                    env.set(alias, Val::Record(ex));
                }
                Item::Artifact(name, run_id) => {
                    // Witness composition: load a prior verified run by RunID
                    let hex = run_id.strip_prefix("sha256:").unwrap_or(&run_id);
                    let receipt_path = format!("receipts/sha256_{}.json", hex);
                    let bytes = std::fs::read(&receipt_path).map_err(|_| {
                        anyhow!("ERROR_ARTIFACT run_id {} not found in receipts/", run_id)
                    })?;
                    let receipt = json_from_slice(&bytes)
                        .map_err(|e| anyhow!("ERROR_ARTIFACT malformed receipt: {}", e))?;
                    // Verify RunID matches
                    let stored_id = match &receipt {
                        J::Object(m) => m.get("run_id").and_then(|v| if let J::Str(s) = v { Some(s.as_str()) } else { None }).unwrap_or(""),
                        _ => "",
                    };
                    if stored_id != run_id {
                        bail!("ERROR_ARTIFACT run_id mismatch: stored={} requested={}", stored_id, run_id);
                    }
                    // Extract output field
                    let output = match &receipt {
                        J::Object(m) => m.get("output").cloned().ok_or_else(|| anyhow!("ERROR_ARTIFACT receipt missing output"))?,
                        _ => bail!("ERROR_ARTIFACT receipt is not an object"),
                    };
                    // Convert JsonVal to Val
                    let val = jval_to_val(&output);
                    // Record dependency in trace
                    tracer.note_artifact_dep(&run_id)?;
                    env.set(name, val);
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
                    let raw_params: Vec<Pat> = params.into_iter().map(|(p, _)| p).collect();
                    // Try to compile to VM bytecode for pure functions with simple bind params
                    let has_while = expr_contains_while(&body);
                    let all_bind = !has_while && raw_params.iter().all(|p| matches!(p, Pat::Bind(_)));
                    let vm_val = if all_bind {
                        let mut fns: Vec<VmCompiledFn> = Vec::new();
                        let mut compiler = VmCompiler::new();
                        // Pre-allocate param slots
                        for p in &raw_params {
                            if let Pat::Bind(n) = p { compiler.slot_of(n); }
                        }
                        // Pre-populate env slots so closures over outer vars work
                        let env_keys = env.keys();
                        for k in &env_keys {
                            compiler.slot_of(k);
                        }
                        if compiler.compile(&body, &mut fns).is_ok() {
                            compiler.emit(VmOp::Return);
                            // Add self-name slot so recursive calls work
                            let self_slot = compiler.slot_of(&name);
                            let n_slots = compiler.slot_names.len();
                            let slot_names = compiler.slot_names.clone();
                            // Build free_vars: only slots actually referenced as Var in the body
                            let n_params_local = raw_params.len();
                            let mut free_vars: Vec<(usize, Val)> = Vec::new();
                            for (i, sname) in slot_names.iter().enumerate() {
                                if i >= n_params_local && sname != &name {
                                    // Only include if the body actually uses this var
                                    if expr_contains_var(&body, sname) {
                                        if let Some(v) = env.get(sname) {
                                            if !matches!(v, Val::Unit) {
                                                free_vars.push((i, v));
                                            }
                                        }
                                    }
                                }
                            }
                            fns.push(VmCompiledFn {
                                code: compiler.code,
                                n_slots,
                                n_params: raw_params.len(),
                                slot_names,
                                self_slot: Some(self_slot),
                                free_vars,
                            });
                            let fn_idx = fns.len() - 1;
                            // Store compiled fns in a thread-local for the VM to access
                            VM_FNS.with(|cell| {
                                let mut v = cell.borrow_mut();
                                let base = v.len();
                                // Remap fn_indices and record self-slot
                                for mut f in fns {
                                    for op in f.code.iter_mut() {
                                        if let VmOp::MakeClosure(i) = op { *i += base; }
                                    }
                                    v.push(f);
                                }
                                let global_idx = base + fn_idx;
                                // Register self-slot so vm_exec_fn pre-populates it
                                VM_SELF_SLOTS.with(|ss| {
                                    ss.borrow_mut().insert(global_idx, (self_slot, Val::VmFunc(global_idx)));
                                });
                                Some(Val::VmFunc(global_idx))
                            })
                        } else {
                            None
                        }
                    } else { None };

                    let f = vm_val.unwrap_or_else(|| Val::Func(Func {
                        params: raw_params,
                        body,
                        env: env.clone(),
                    }));
                    env.set(name.clone(), f.clone());
                    // For VM fns: inject self-reference into the fn's own slot table
                    // so recursive calls (Var("fib") -> LoadSlot(N)) resolve correctly
                    if let Val::VmFunc(fn_idx) = &f {
                        let fi = *fn_idx;
                        VM_FNS.with(|cell| {
                            let fns = cell.borrow();
                            if let Some(compiled) = fns.get(fi) {
                                // find slot index for own name
                                if let Some(self_slot) = compiled.slot_names.iter().position(|s| s == &name) {
                                    drop(fns);
                                    // store initial_vals hint: slot N = VmFunc(fi)
                                    // We do this via a separate thread-local
                                    VM_SELF_SLOTS.with(|ss| {
                                        ss.borrow_mut().insert(fi, (self_slot, Val::VmFunc(fi)));
                                    });
                                }
                            }
                        });
                    }
                }
                Item::Export(ns) => exports = Some(ns),
                Item::Test(_, _, _) => {
                    // Test blocks are skipped during normal eval
                    // They are only executed by `fardrun test`
                }
                Item::TypeDef(type_name, kind) => {
                    match kind {
                        TypeDefKind::Record(fields) => {
                            // Constructor: Point { x: 3, y: 4 } is handled at call site
                            // Register a validator function: Point(record) -> record (validated)
                            let field_names: Vec<String> = fields.iter().map(|f| match f {
                                TypeField::Named(n, _) => n.clone(),
                            }).collect();
                            // Type-checking constructor: validates required fields at call time
                            let checker = Val::Builtin(Builtin::TypeCheck(
                                type_name.clone(),
                                field_names.clone(),
                            ));
                            env.set(type_name, checker);
                        }
                        TypeDefKind::Sum(variants) => {
                            // Register each variant as a constructor function
                            for (vname, fields) in variants {
                                let field_names: Vec<String> = fields.iter().map(|f| match f {
                                    TypeField::Named(n, _) => n.clone(),
                                }).collect();
                                // Type-checking constructor for sum variant
                                let ctor = Val::Builtin(Builtin::TypeCheck(
                                    format!("{}::{}", type_name, vname),
                                    field_names.clone(),
                                ));
                                env.set(vname, ctor);
                            }
                        }
                    }
                }
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
            return Ok(Val::Record(out));
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
                let spec = if let Some(s) = name.strip_prefix("pkg:") {
                    s
                } else if let Some(s) = name.strip_prefix("pkg/") {
                    s
                } else {
                    name
                };
                // Resolve short name via fard.toml [deps] if no @ present
                let resolved_spec: String = if !spec.contains('@') {
                    slf.pkg_deps.get(spec)
                        .cloned()
                        .ok_or_else(|| anyhow!("ERROR_RUNTIME pkg '{}' not found in fard.toml [deps]", spec))?
                } else {
                    spec.to_string()
                };
                let spec = resolved_spec.as_str();
                let (pkg, ver_and_mod) = spec
                    .split_once("@")
                    .ok_or_else(|| anyhow!("ERROR_RUNTIME bad pkg import: {name}"))?;
                // mod_id is optional: pkg:math@2026-03-08 or pkg:math@2026-03-08/utils
                let (ver, mod_id) = if let Some((v, m)) = ver_and_mod.split_once("/") {
                    (v, m)
                } else {
                    (ver_and_mod, "main")
                };
                // Resolve base dir: use --registry if provided, else fetch from network
                let base = if let Some(reg) = slf.registry_dir.as_ref() {
                    reg.join("pkgs").join(pkg).join(ver)
                } else {
                    fetch_package(pkg, ver)?
                };
                // Check for fard.toml (network package) or package.json (local registry)
                let fard_toml_path = base.join("fard.toml");
                let path = if fard_toml_path.exists() {
                    // Network package: files are directly in base dir
                    // Read entry from fard.toml if possible, else use mod_id.fard
                    let entry = if let Ok(toml_src) = fs::read_to_string(&fard_toml_path) {
                        toml_src.lines()
                            .find(|l| l.starts_with("entry"))
                            .and_then(|l| l.split('=').nth(1))
                            .map(|s| s.trim().trim_matches('"').to_string())
                            .unwrap_or_else(|| format!("{mod_id}.fard"))
                    } else {
                        format!("{mod_id}.fard")
                    };
                    base.join(entry)
                } else {
                    // Local registry layout: files/ subdir + package.json
                    let pkg_json_path = base.join("package.json");
                    let rel: String = if let Ok(pkg_json_bytes) = fs::read(&pkg_json_path) {
                        let pkg_json: J = json_from_slice(&pkg_json_bytes).map_err(|e| anyhow::anyhow!("{}", e))
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
                    base.join("files").join(&rel)
                };

                let p = path.to_string_lossy().to_string();
                let got = file_digest(&path).unwrap_or_else(|_| "sha256:unknown".to_string());
                if let Some(exp) = slf.lock.as_ref().and_then(|lk| lk.expected(name)) {
                    if got != exp {
                        bail!("ERROR_LOCK digest mismatch for {name}: expected {exp}, got {got}");
                    }
                }
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
                    Val::Record(m) => m,
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
                    Val::Record(m) => m,
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
                    Val::Record(m) => m,
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
            match lk.expected(module) {
                Some(exp) => {
                    if exp != got {
                        bail!("LOCK_MISMATCH lock mismatch for module {module}: expected {exp}, got {got}");
                    }
                }
                None => {
                    if self.enforce_lockfile {
                        bail!("ERROR_LOCK module not in lockfile: {module}");
                    }
                }
            }
        } else if self.enforce_lockfile {
            bail!("ERROR_LOCK --enforce-lockfile set but no lockfile loaded");
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
                m.insert("group_by".to_string(), Val::Builtin(Builtin::ListGroupBy));
                m.insert("group_by".to_string(), Val::Builtin(Builtin::ListGroupBy));
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
                m.insert("set".to_string(), Val::Builtin(Builtin::ListSet));
                m.insert("any".to_string(), Val::Builtin(Builtin::ListAny));
                m.insert("all".to_string(), Val::Builtin(Builtin::ListAll));
                m.insert("find".to_string(), Val::Builtin(Builtin::ListFind));
                m.insert("find_index".to_string(), Val::Builtin(Builtin::ListFindIndex));
                m.insert("take".to_string(), Val::Builtin(Builtin::ListTake));
                m.insert("drop".to_string(), Val::Builtin(Builtin::ListDrop));
                m.insert("flat_map".to_string(), Val::Builtin(Builtin::ListFlatMap));
                m.insert("par_map".to_string(), Val::Builtin(Builtin::ListParMap));
                m.insert("zip_with".to_string(), Val::Builtin(Builtin::ListZipWith));
                m.insert("chunk".to_string(), Val::Builtin(Builtin::ListChunk));
                m.insert("sort_by".to_string(), Val::Builtin(Builtin::ListSortBy));
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
                m.insert("and_then".to_string(), Val::Builtin(Builtin::ResultAndThen));
                m.insert("unwrap_ok".to_string(), Val::Builtin(Builtin::ResultUnwrapOk));
                m.insert("unwrap_err".to_string(), Val::Builtin(Builtin::ResultUnwrapErr));
                m.insert("is_ok".to_string(), Val::Builtin(Builtin::ResultIsOk));
                m.insert("is_err".to_string(), Val::Builtin(Builtin::ResultIsErr));
                m.insert("map".to_string(), Val::Builtin(Builtin::ResultMap));
                m.insert("map_err".to_string(), Val::Builtin(Builtin::ResultMapErr));
                m.insert("or_else".to_string(), Val::Builtin(Builtin::ResultOrElse));
                m.insert("unwrap".to_string(), Val::Builtin(Builtin::ResultUnwrapOk));
                m.insert("unwrap_or".to_string(), Val::Builtin(Builtin::ResultUnwrapOr));
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
                m.insert("upper".to_string(), Val::Builtin(Builtin::StrUpper));
                m.insert("contains".to_string(), Val::Builtin(Builtin::StrContains));
                m.insert("starts_with".to_string(), Val::Builtin(Builtin::StrStartsWith));
                m.insert("ends_with".to_string(), Val::Builtin(Builtin::StrEndsWith));
                m.insert("replace".to_string(), Val::Builtin(Builtin::StrReplace));
                m.insert("slice".to_string(), Val::Builtin(Builtin::StrSlice));
                m.insert("format".to_string(), Val::Builtin(Builtin::StrFormat));
                m.insert("from_int".to_string(), Val::Builtin(Builtin::StrFromInt));
                m.insert("join".to_string(), Val::Builtin(Builtin::StrJoin));
                m.insert("from_float".to_string(), Val::Builtin(Builtin::StrFromFloat));
                m.insert("pad_left".to_string(), Val::Builtin(Builtin::StrPadLeft));
                m.insert("pad_right".to_string(), Val::Builtin(Builtin::StrPadRight));
                m.insert("repeat".to_string(), Val::Builtin(Builtin::StrRepeat));
                m.insert("index_of".to_string(), Val::Builtin(Builtin::StrIndexOf));
                m.insert("chars".to_string(), Val::Builtin(Builtin::StrChars));
                Ok(m)
            }
            "std/ast" => {
                let mut m = BTreeMap::new();
                m.insert("parse".to_string(), Val::Builtin(Builtin::AstParse));
                Ok(m)
            }
            "std/promise" => {
                let mut m = BTreeMap::new();
                m.insert("spawn".to_string(), Val::Builtin(Builtin::PromiseSpawn));
                m.insert("await".to_string(), Val::Builtin(Builtin::PromiseAwait));
                m.insert("spawn_ordered".to_string(), Val::Builtin(Builtin::PromiseSpawnOrdered));
                Ok(m)
            }
            "std/bigint" => {
                let mut m = BTreeMap::new();
                m.insert("from_int".to_string(), Val::Builtin(Builtin::BigFromInt));
                m.insert("from_str".to_string(), Val::Builtin(Builtin::BigFromStr));
                m.insert("add".to_string(), Val::Builtin(Builtin::BigAdd));
                m.insert("sub".to_string(), Val::Builtin(Builtin::BigSub));
                m.insert("mul".to_string(), Val::Builtin(Builtin::BigMul));
                m.insert("div".to_string(), Val::Builtin(Builtin::BigDiv));
                m.insert("mod".to_string(), Val::Builtin(Builtin::BigMod));
                m.insert("pow".to_string(), Val::Builtin(Builtin::BigPow));
                m.insert("to_str".to_string(), Val::Builtin(Builtin::BigToStr));
                m.insert("eq".to_string(), Val::Builtin(Builtin::BigEq));
                m.insert("lt".to_string(), Val::Builtin(Builtin::BigLt));
                m.insert("gt".to_string(), Val::Builtin(Builtin::BigGt));
                Ok(m)
            }
            "std/mutex" => {
                let mut m = BTreeMap::new();
                m.insert("new".to_string(), Val::Builtin(Builtin::MutexNew));
                m.insert("lock".to_string(), Val::Builtin(Builtin::MutexLock));
                m.insert("unlock".to_string(), Val::Builtin(Builtin::MutexUnlock));
                m.insert("with_lock".to_string(), Val::Builtin(Builtin::MutexWithLock));
                Ok(m)
            }
            "std/chan" => {
                let mut m = BTreeMap::new();
                m.insert("new".to_string(), Val::Builtin(Builtin::ChanNew));
                m.insert("send".to_string(), Val::Builtin(Builtin::ChanSend));
                m.insert("recv".to_string(), Val::Builtin(Builtin::ChanRecv));
                m.insert("try_recv".to_string(), Val::Builtin(Builtin::ChanTryRecv));
                m.insert("close".to_string(), Val::Builtin(Builtin::ChanClose));
                Ok(m)
            }
            "std/uuid" => {
                let mut m = BTreeMap::new();
                m.insert("v4".to_string(), Val::Builtin(Builtin::UuidV4));
                m.insert("validate".to_string(), Val::Builtin(Builtin::UuidValidate));
                Ok(m)
            }
            "std/datetime" => {
                let mut m = BTreeMap::new();
                m.insert("now".to_string(), Val::Builtin(Builtin::DateTimeNow));
                m.insert("format".to_string(), Val::Builtin(Builtin::DateTimeFormat));
                m.insert("parse".to_string(), Val::Builtin(Builtin::DateTimeParse));
                m.insert("add".to_string(), Val::Builtin(Builtin::DateTimeAdd));
                m.insert("diff".to_string(), Val::Builtin(Builtin::DateTimeSub));
                m.insert("field".to_string(), Val::Builtin(Builtin::DateTimeField));
                Ok(m)
            }
            "std/set" => {
                let mut m = BTreeMap::new();
                m.insert("new".to_string(), Val::Builtin(Builtin::SetNew));
                m.insert("add".to_string(), Val::Builtin(Builtin::SetAdd));
                m.insert("remove".to_string(), Val::Builtin(Builtin::SetRemove));
                m.insert("has".to_string(), Val::Builtin(Builtin::SetHas));
                m.insert("union".to_string(), Val::Builtin(Builtin::SetUnion));
                m.insert("intersect".to_string(), Val::Builtin(Builtin::SetIntersect));
                m.insert("diff".to_string(), Val::Builtin(Builtin::SetDiff));
                m.insert("to_list".to_string(), Val::Builtin(Builtin::SetToList));
                m.insert("from_list".to_string(), Val::Builtin(Builtin::SetFromList));
                m.insert("size".to_string(), Val::Builtin(Builtin::SetSize));
                Ok(m)
            }
            "std/map" => {
                let mut m = BTreeMap::new();
                m.insert("get".to_string(), Val::Builtin(Builtin::MapGet));
                m.insert("set".to_string(), Val::Builtin(Builtin::MapSet));
                m.insert("keys".to_string(), Val::Builtin(Builtin::RecKeys));
                m.insert("values".to_string(), Val::Builtin(Builtin::RecValues));
                m.insert("has".to_string(), Val::Builtin(Builtin::RecHas));
                m.insert("delete".to_string(), Val::Builtin(Builtin::MapDelete));
                m.insert("entries".to_string(), Val::Builtin(Builtin::MapEntries));
                m.insert("new".to_string(), Val::Builtin(Builtin::RecEmpty));
                m.insert("from_entries".to_string(), Val::Builtin(Builtin::RecEmpty));
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

            "std/type" => {
                let mut m = BTreeMap::new();
                m.insert("of".to_string(), Val::Builtin(Builtin::TypeOf));
                Ok(m)
            }
            "std/cast" =>
            {
                let mut m = BTreeMap::new();
                m.insert("float".to_string(), Val::Builtin(Builtin::CastFloat));
                m.insert("int".to_string(),   Val::Builtin(Builtin::CastInt));
                m.insert("text".to_string(), Val::Builtin(Builtin::CastText));
                Ok(m)
            }
            "std/int" => {
                let mut m = BTreeMap::new();
                m.insert("add".to_string(), Val::Builtin(Builtin::IntAdd));
                m.insert("eq".to_string(), Val::Builtin(Builtin::IntEq));
                m.insert("parse".to_string(), Val::Builtin(Builtin::IntParse));
                m.insert("pow".to_string(), Val::Builtin(Builtin::IntPow));
                m.insert("to_hex".to_string(), Val::Builtin(Builtin::IntToHex));
                m.insert("to_bin".to_string(), Val::Builtin(Builtin::IntToBin));
                m.insert("mul".to_string(), Val::Builtin(Builtin::IntMul));
                m.insert("div".to_string(), Val::Builtin(Builtin::IntDiv));
                m.insert("sub".to_string(), Val::Builtin(Builtin::IntSub));
                m.insert("abs".to_string(), Val::Builtin(Builtin::IntAbs));
                m.insert("min".to_string(), Val::Builtin(Builtin::IntMin));
                m.insert("max".to_string(), Val::Builtin(Builtin::IntMax));
                m.insert("to_text".to_string(), Val::Builtin(Builtin::IntToText));
                m.insert("from_text".to_string(), Val::Builtin(Builtin::IntFromText));
                m.insert("neg".to_string(), Val::Builtin(Builtin::IntNeg));
                m.insert("clamp".to_string(), Val::Builtin(Builtin::IntClamp));
                m.insert("mod".to_string(), Val::Builtin(Builtin::IntMod));
                m.insert("lt".to_string(), Val::Builtin(Builtin::IntLt));
                m.insert("gt".to_string(), Val::Builtin(Builtin::IntGt));
                m.insert("le".to_string(), Val::Builtin(Builtin::IntLe));
                m.insert("ge".to_string(), Val::Builtin(Builtin::IntGe));
                                m.insert("to_str_padded".to_string(), Val::Builtin(Builtin::IntToStrPadded));
Ok(m)
            }
            "std/fs" => {
                let mut m = BTreeMap::new();
                m.insert("read_text".to_string(), Val::Builtin(Builtin::FsReadText));
                m.insert("write_text".to_string(), Val::Builtin(Builtin::FsWriteText));
                m.insert("exists".to_string(), Val::Builtin(Builtin::FsExists));
                m.insert("read_dir".to_string(), Val::Builtin(Builtin::FsReadDir));
                m.insert("stat".to_string(), Val::Builtin(Builtin::FsStat));
                m.insert("delete".to_string(), Val::Builtin(Builtin::FsDelete));
                m.insert("make_dir".to_string(), Val::Builtin(Builtin::FsMakeDir));
                Ok(m)
            }
            "std/option" => {
                let mut m = BTreeMap::new();
                m.insert("none".to_string(), Val::Builtin(Builtin::OptionNone));
                m.insert("None".to_string(), Val::Builtin(Builtin::OptionNone));
                m.insert("some".to_string(), Val::Builtin(Builtin::OptionSome));
                m.insert("Some".to_string(), Val::Builtin(Builtin::OptionSome));
                m.insert("is_none".to_string(), Val::Builtin(Builtin::OptionIsNone));
                m.insert("isNone".to_string(), Val::Builtin(Builtin::OptionIsNone));
                m.insert("is_some".to_string(), Val::Builtin(Builtin::OptionIsSome));
                m.insert("isSome".to_string(), Val::Builtin(Builtin::OptionIsSome));
                m.insert("from_nullable".to_string(), Val::Builtin(Builtin::OptionFromNullable));
                m.insert("fromNullable".to_string(), Val::Builtin(Builtin::OptionFromNullable));
                m.insert("to_nullable".to_string(), Val::Builtin(Builtin::OptionToNullable));
                m.insert("toNullable".to_string(), Val::Builtin(Builtin::OptionToNullable));
                m.insert("map".to_string(), Val::Builtin(Builtin::OptionMap));
                m.insert("and_then".to_string(), Val::Builtin(Builtin::OptionAndThen));
                m.insert("andThen".to_string(), Val::Builtin(Builtin::OptionAndThen));
                m.insert("unwrap_or".to_string(), Val::Builtin(Builtin::OptionUnwrapOr));
                m.insert("unwrapOr".to_string(), Val::Builtin(Builtin::OptionUnwrapOr));
                m.insert("unwrap_or_else".to_string(), Val::Builtin(Builtin::OptionUnwrapOrElse));
                m.insert("unwrapOrElse".to_string(), Val::Builtin(Builtin::OptionUnwrapOrElse));
                m.insert("to_result".to_string(), Val::Builtin(Builtin::OptionToResult));
                m.insert("toResult".to_string(), Val::Builtin(Builtin::OptionToResult));
                Ok(m)
            }
            "std/bits" => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("band".to_string(),     Val::Builtin(Builtin::BitAnd));
                m.insert("bor".to_string(),      Val::Builtin(Builtin::BitOr));
                m.insert("bxor".to_string(),     Val::Builtin(Builtin::BitXor));
                m.insert("bnot".to_string(),     Val::Builtin(Builtin::BitNot));
                m.insert("bshl".to_string(),     Val::Builtin(Builtin::BitShl));
                m.insert("bshr".to_string(),     Val::Builtin(Builtin::BitShr));
                m.insert("popcount".to_string(), Val::Builtin(Builtin::BitPopcount));
                Ok(m)
            }
            "std/math" => {
                let mut m = BTreeMap::new();
                m.insert("abs".to_string(), Val::Builtin(Builtin::MathAbs));
                m.insert("min".to_string(), Val::Builtin(Builtin::MathMin));
                m.insert("max".to_string(), Val::Builtin(Builtin::MathMax));
                m.insert("pow".to_string(), Val::Builtin(Builtin::MathPow));
                m.insert("sqrt".to_string(), Val::Builtin(Builtin::MathSqrt));
                m.insert("floor".to_string(), Val::Builtin(Builtin::MathFloor));
                m.insert("ceil".to_string(), Val::Builtin(Builtin::MathCeil));
                m.insert("round".to_string(), Val::Builtin(Builtin::MathRound));
                m.insert("log".to_string(), Val::Builtin(Builtin::MathLog));
                m.insert("log2".to_string(), Val::Builtin(Builtin::MathLog2));
                m.insert("sin".to_string(), Val::Builtin(Builtin::MathSin));
                m.insert("cos".to_string(), Val::Builtin(Builtin::MathCos));
                m.insert("tan".to_string(), Val::Builtin(Builtin::MathTan));
                m.insert("atan2".to_string(), Val::Builtin(Builtin::MathAtan2));
                m.insert("exp".to_string(), Val::Builtin(Builtin::MathExp));
                m.insert("pi".to_string(), Val::Float(std::f64::consts::PI));
                m.insert("e".to_string(), Val::Float(std::f64::consts::E));
                m.insert("inf".to_string(), Val::Float(f64::INFINITY));
                m.insert("asin".to_string(), Val::Builtin(Builtin::MathAsin));
                m.insert("acos".to_string(), Val::Builtin(Builtin::MathAcos));
                m.insert("atan".to_string(), Val::Builtin(Builtin::MathAtan));
                m.insert("log10".to_string(), Val::Builtin(Builtin::MathLog10));
                Ok(m)
            }
            "std/null" => {
                let mut m = BTreeMap::new();
                m.insert("isNull".to_string(), Val::Builtin(Builtin::NullIsNull));
                m.insert("coalesce".to_string(), Val::Builtin(Builtin::NullCoalesce));
                m.insert("guardNotNull".to_string(), Val::Builtin(Builtin::NullGuard));
                Ok(m)
            }
            "std/path" => {
                let mut m = BTreeMap::new();
                m.insert("base".to_string(), Val::Builtin(Builtin::PathBase));
                m.insert("dir".to_string(), Val::Builtin(Builtin::PathDir));
                m.insert("ext".to_string(), Val::Builtin(Builtin::PathExt));
                m.insert("isAbs".to_string(), Val::Builtin(Builtin::PathIsAbs));
                m.insert("join".to_string(), Val::Builtin(Builtin::PathJoin));
                m.insert("joinAll".to_string(), Val::Builtin(Builtin::PathJoinAll));
                m.insert("normalize".to_string(), Val::Builtin(Builtin::PathNormalize));
                Ok(m)
            }
            "std/time" => {
                let mut m = BTreeMap::new();
                m.insert("now".to_string(), Val::Builtin(Builtin::TimeNow));
                m.insert("parse".to_string(), Val::Builtin(Builtin::TimeParse));
                m.insert("format".to_string(), Val::Builtin(Builtin::TimeFormat));
                m.insert("add".to_string(), Val::Builtin(Builtin::TimeAdd));
                m.insert("sub".to_string(), Val::Builtin(Builtin::TimeSub));
                let mut d = BTreeMap::new();
                d.insert("ms".to_string(), Val::Builtin(Builtin::TimeDurationMs));
                d.insert("sec".to_string(), Val::Builtin(Builtin::TimeDurationSec));
                d.insert("min".to_string(), Val::Builtin(Builtin::TimeDurationMin));
                m.insert("Duration".to_string(), Val::Record(d));
                Ok(m)
            }
            "std/trace" => {
                let mut m = BTreeMap::new();
                m.insert("emit".to_string(), Val::Builtin(Builtin::Emit));
                m.insert("info".to_string(), Val::Builtin(Builtin::TraceInfo));
                m.insert("warn".to_string(), Val::Builtin(Builtin::TraceWarn));
                m.insert("error".to_string(), Val::Builtin(Builtin::TraceError));
                m.insert("span".to_string(), Val::Builtin(Builtin::TraceSpan));
                Ok(m)
            }
            "std/sembit" => {
                let mut m = BTreeMap::new();
                m.insert("partition".to_string(), Val::Builtin(Builtin::SembitPartition));
                Ok(m)
            }
            "std/artifact" => {
                let mut m = BTreeMap::new();
                m.insert("import".to_string(), Val::Builtin(Builtin::ImportArtifact));
                m.insert("emit".to_string(), Val::Builtin(Builtin::EmitArtifact));
                m.insert("ref".to_string(), Val::Builtin(Builtin::Unimplemented("std/trace.ref")));
                m.insert("derive".to_string(), Val::Builtin(Builtin::Unimplemented("std/trace.derive")));
                Ok(m)
            }
            "std/cli" => {
                let mut m = BTreeMap::new();
                m.insert("args".to_string(),      Val::Builtin(Builtin::CliArgs));
                m.insert("get".to_string(),       Val::Builtin(Builtin::CliGet));
                m.insert("get_int".to_string(),   Val::Builtin(Builtin::CliGetInt));
                m.insert("get_float".to_string(), Val::Builtin(Builtin::CliGetFloat));
                m.insert("get_bool".to_string(),  Val::Builtin(Builtin::CliGetBool));
                m.insert("has".to_string(),       Val::Builtin(Builtin::CliHas));
                // cli.parse(spec) is implemented in FARD stdlib, not as a builtin
                Ok(m)
            }
            "std/io" => {
                let mut m = BTreeMap::new();
                m.insert("read_file".to_string(),   Val::Builtin(Builtin::IoReadFile));
                m.insert("write_file".to_string(),  Val::Builtin(Builtin::IoWriteFile));
                m.insert("append_file".to_string(), Val::Builtin(Builtin::IoAppendFile));
                m.insert("read_lines".to_string(),  Val::Builtin(Builtin::IoReadLines));
                m.insert("file_exists".to_string(), Val::Builtin(Builtin::IoFileExists));
                m.insert("delete_file".to_string(), Val::Builtin(Builtin::IoDeleteFile));
                m.insert("read_stdin".to_string(),  Val::Builtin(Builtin::IoReadStdin));
                m.insert("read_stdin_lines".to_string(), Val::Builtin(Builtin::IoReadStdinLines));
                m.insert("list_dir".to_string(),    Val::Builtin(Builtin::IoListDir));
                m.insert("make_dir".to_string(),    Val::Builtin(Builtin::IoMakeDir));
                Ok(m)
            }
            "std/bytes" => {
                let mut m = BTreeMap::new();
                m.insert("concat".to_string(),      Val::Builtin(Builtin::BytesConcat));
                m.insert("to_str".to_string(),       Val::Builtin(Builtin::BytesToStr));
                m.insert("len".to_string(),          Val::Builtin(Builtin::BytesLen));
                m.insert("get".to_string(),          Val::Builtin(Builtin::BytesGet));
                m.insert("of_list".to_string(),      Val::Builtin(Builtin::BytesOfList));
                m.insert("to_list".to_string(),      Val::Builtin(Builtin::BytesToList));
                m.insert("of_str".to_string(),       Val::Builtin(Builtin::BytesOfStr));
                m.insert("merkle_root".to_string(),  Val::Builtin(Builtin::BytesMerkleRoot));
                Ok(m)
            }

            "std/codec" => {
                let mut m = BTreeMap::new();
                m.insert("base64url_encode".to_string(), Val::Builtin(Builtin::CodecBase64UrlEncode));
                m.insert("base64url_encode_hex".to_string(), Val::Builtin(Builtin::CodecBase64UrlEncodeHex));
                m.insert("base64url_decode".to_string(), Val::Builtin(Builtin::CodecBase64UrlDecode));
                m.insert("hex_encode".to_string(), Val::Builtin(Builtin::CodecHexEncode));
                m.insert("hex_decode".to_string(), Val::Builtin(Builtin::CodecHexDecode));
                Ok(m)
            }
            "std/cell" => {
                let mut m = BTreeMap::new();
                m.insert("new".to_string(), Val::Builtin(Builtin::CellNew));
                m.insert("get".to_string(), Val::Builtin(Builtin::CellGet));
                m.insert("set".to_string(), Val::Builtin(Builtin::CellSet));
                Ok(m)
            }
            "std/base64" => {
                let mut m = BTreeMap::new();
                m.insert("encode".to_string(), Val::Builtin(Builtin::Base64Encode));
                m.insert("decode".to_string(), Val::Builtin(Builtin::Base64Decode));
                Ok(m)
            }
            "std/csv" => {
                let mut m = BTreeMap::new();
                m.insert("parse".to_string(), Val::Builtin(Builtin::CsvParse));
                m.insert("encode".to_string(), Val::Builtin(Builtin::CsvEncode));
                Ok(m)
            }
            "std/eval" => {
                let mut m = BTreeMap::new();
                m.insert("eval".to_string(), Val::Builtin(Builtin::FardEval));
                Ok(m)
            }
            "std/witness" => {
                let mut m = BTreeMap::new();
                m.insert("self_digest".to_string(), Val::Builtin(Builtin::WitnessSelfDigest));
                m.insert("deps".to_string(), Val::Builtin(Builtin::WitnessDeps));
                m.insert("verify".to_string(), Val::Builtin(Builtin::WitnessVerify));
                m.insert("verify_chain".to_string(), Val::Builtin(Builtin::WitnessVerifyChain));
                Ok(m)
            }
            "std/ffi" => {
                let mut m = BTreeMap::new();
                m.insert("open".to_string(),  Val::Builtin(Builtin::FfiOpen));
                m.insert("call".to_string(),  Val::Builtin(Builtin::FfiCall));
                m.insert("call_pure".to_string(), Val::Builtin(Builtin::FfiCallPure));
                m.insert("call_str".to_string(), Val::Builtin(Builtin::FfiCallStr));
                m.insert("call_checked".to_string(), Val::Builtin(Builtin::FfiCallChecked));
                m.insert("load".to_string(),  Val::Builtin(Builtin::FfiOpen));
                m.insert("close".to_string(), Val::Builtin(Builtin::FfiClose));
                Ok(m)
            }
            "std/re" => {
                let mut m = BTreeMap::new();
                m.insert("is_match".to_string(), Val::Builtin(Builtin::ReMatch));
                m.insert("find".to_string(), Val::Builtin(Builtin::ReFind));
                m.insert("find_all".to_string(), Val::Builtin(Builtin::ReFindAll));
                m.insert("split".to_string(), Val::Builtin(Builtin::ReSplit));
                m.insert("replace".to_string(), Val::Builtin(Builtin::ReReplace));
                Ok(m)
            }
            "std/env" => {
                let mut m = BTreeMap::new();
                m.insert("get".to_string(), Val::Builtin(Builtin::EnvGet));
                m.insert("args".to_string(), Val::Builtin(Builtin::EnvArgs));
                Ok(m)
            }
            "std/process" => {
                let mut m = BTreeMap::new();
                m.insert("spawn".to_string(), Val::Builtin(Builtin::ProcessSpawn));
                m.insert("exit".to_string(), Val::Builtin(Builtin::ProcessExit));
                Ok(m)
            }
            "std/hash" => {
                let mut m = BTreeMap::new();
                m.insert("sha256_bytes".to_string(), Val::Builtin(Builtin::HashSha256Bytes));
                m.insert("sha256_text".to_string(), Val::Builtin(Builtin::HashSha256Text));
                Ok(m)
            }
            "std/http" => {
                let mut m = BTreeMap::new();
                m.insert("get".to_string(), Val::Builtin(Builtin::HttpGet));
                m.insert("post".to_string(), Val::Builtin(Builtin::HttpPost));
                m.insert("request".to_string(), Val::Builtin(Builtin::HttpRequest));
                Ok(m)
            }
            "std/net" => {
                let mut m = BTreeMap::new();
                m.insert("serve".to_string(), Val::Builtin(Builtin::NetServe));
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

            "std/compress" => {
                let mut m = BTreeMap::new();
                m.insert("gzip".to_string(),   Val::Builtin(Builtin::CompressGzip));
                m.insert("gunzip".to_string(),  Val::Builtin(Builtin::CompressGunzip));
                Ok(m)
            }
            "std/graph" => {
                let mut m = BTreeMap::new();
                m.insert("of".to_string(),        Val::Builtin(Builtin::GraphOf));
                m.insert("ancestors".to_string(),  Val::Builtin(Builtin::GraphAncestors));
                m.insert("leaves".to_string(),     Val::Builtin(Builtin::GraphLeaves));
                m.insert("to_dot".to_string(),     Val::Builtin(Builtin::GraphToDot));
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
                m.insert("sha512".to_string(), Val::Builtin(Builtin::CryptoSha512));
                m.insert("aes_encrypt".to_string(), Val::Builtin(Builtin::CryptoAesEncrypt));
                m.insert("aes_decrypt".to_string(), Val::Builtin(Builtin::CryptoAesDecrypt));
                m.insert("merkle_root".to_string(), Val::Builtin(Builtin::CryptoMerkleRoot));
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
                m.insert("to_str_fixed".to_string(), Val::Builtin(Builtin::FloatToStrFixed));
                m.insert("is_inf".to_string(), Val::Builtin(Builtin::FloatIsInf));
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
                m.insert("relu".to_string(), Val::Builtin(Builtin::LinalgRelu));
                m.insert("softmax".to_string(), Val::Builtin(Builtin::LinalgSoftmax));
                m.insert("argmax".to_string(), Val::Builtin(Builtin::LinalgArgmax));
                m.insert("matmul".to_string(), Val::Builtin(Builtin::LinalgMatmul));
                m.insert("transpose".to_string(), Val::Builtin(Builtin::LinalgTranspose));
                m.insert("eigh".to_string(), Val::Builtin(Builtin::LinalgEigh));
                m.insert("vec_add".to_string(), Val::Builtin(Builtin::LinalgVecAdd));
                m.insert("vec_sub".to_string(), Val::Builtin(Builtin::LinalgVecSub));
                m.insert("vec_scale".to_string(), Val::Builtin(Builtin::LinalgVecScale));
                m.insert("mat_add".to_string(), Val::Builtin(Builtin::LinalgMatAdd));
                m.insert("mat_scale".to_string(), Val::Builtin(Builtin::LinalgMatScale));
                m.insert("vec_exp".to_string(), Val::Builtin(Builtin::LinalgVecExp));
                m.insert("vec_log".to_string(), Val::Builtin(Builtin::LinalgVecLog));
                m.insert("vec_sum".to_string(), Val::Builtin(Builtin::LinalgVecSum));
                m.insert("vec_max".to_string(), Val::Builtin(Builtin::LinalgVecMax));
                m.insert("vec_mul".to_string(), Val::Builtin(Builtin::LinalgVecMul));
                m.insert("vec_relu".to_string(), Val::Builtin(Builtin::LinalgVecRelu));
                m.insert("vec_relu_grad".to_string(), Val::Builtin(Builtin::LinalgVecReluGrad));
                m.insert("softmax_grad".to_string(), Val::Builtin(Builtin::LinalgSoftmaxGrad));
                m.insert("cross_entropy".to_string(), Val::Builtin(Builtin::LinalgCrossEntropy));
                m.insert("outer".to_string(), Val::Builtin(Builtin::LinalgOuter));
                m.insert("mat_mul_vec_grad".to_string(), Val::Builtin(Builtin::LinalgMatMulVecGrad));
                m.insert("vec_scalar_add".to_string(), Val::Builtin(Builtin::LinalgVecScalarAdd));
                m.insert("mat_row_sum".to_string(), Val::Builtin(Builtin::LinalgMatRowSum));
                Ok(m)
            }
            _ => bail!("unknown std module: {name}"),
        }
    }
    fn builtin_digest(&self, name: &str) -> String {
        if name == "std/record" {
            return self.builtin_digest("std/rec");
        }

        let mut h = NativeSha256::new();
        h.update(format!("builtin:{name}:v0.5").as_bytes());
        format!("sha256:{}", hex_lower(&h.finalize()))
    }

    fn stdlib_root_digest(&self) -> String {
        let names: [&str; 29] = [
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
            "std/sembit",
            "std/trace",
            "std/png",
            "std/rec",
            "std/crypto",
            "std/rand",
            "std/ffi",
            "std/witness",
            "std/net",
            "std/compress",
            "std/graph",
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
    e.set("unit".to_string(), Val::Unit);
    e.set("true".to_string(), Val::Bool(true));
    e.set("false".to_string(), Val::Bool(false));
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
fn jval_to_val(j: &J) -> Val {
    match j {
        J::Null => Val::Unit,
        J::Bool(b) => Val::Bool(*b),
        J::Int(n) => Val::Int(*n),
        J::Float(f) => Val::Float(*f),
        J::Str(s) => Val::Text(s.clone()),
        J::Array(items) => Val::List(items.iter().map(jval_to_val).collect()),
        J::Object(m) => {
            // Typed encoding: {"t":"int","v":42} etc
            if let (Some(J::Str(t)), Some(v)) = (m.get("t"), m.get("v")) {
                match t.as_str() {
                    "bool"  => { if let J::Bool(b)  = v { return Val::Bool(*b); } }
                    "int"   => { if let J::Int(n)   = v { return Val::Int(*n); } }
                    "float" => { if let J::Float(f) = v { return Val::Float(*f); } }
                    "text"  => { if let J::Str(s)   = v { return Val::Text(s.clone()); } }
                    "unit"  => { return Val::Unit; }
                    "list"  => { if let J::Array(xs) = v {
                        return Val::List(xs.iter().map(jval_to_val).collect());
                    }}
                    "map"   => { if let J::Array(pairs) = v {
                        let mut kvs: BTreeMap<String, Val> = pairs.iter().filter_map(|p| {
                            if let J::Array(kv) = p {
                                if kv.len() == 2 {
                                    if let J::Str(k) = &kv[0] {
                                        return Some((k.clone(), jval_to_val(&kv[1])));
                                    }
                                }
                            }
                            None
                        }).collect();
                        return Val::Record(kvs);
                    }}
                    _ => {}
                }
            }
            // Plain object -> record
            let kvs: BTreeMap<String, Val> = m.iter()
                .map(|(k,v)| (k.clone(), jval_to_val(v))).collect();
            Val::Record(kvs)
        }
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut h = NativeSha256::new();
    h.update(bytes);
    format!("sha256:{}", hex_lower(&h.finalize()))
}
fn file_digest(p: &Path) -> Result<String> {
    let b = fs::read(p)?;
    let mut h = NativeSha256::new();
    h.update(&b);
    Ok(format!("sha256:{}", hex_lower(&h.finalize())))
}

fn canonical_json_string(v: &J) -> String { String::from_utf8(canonical_json_bytes(v)).unwrap_or_default() }

fn canonical_json_bytes(v: &J) -> Vec<u8> {
    json_to_string(v).into_bytes()
}
