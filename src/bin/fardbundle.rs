use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use valuecore::json::{JsonVal as Value, from_slice, to_string, to_string_pretty};

const HELP: &str = r#"fardbundle

USAGE:
  fardbundle build  --root <dir> --entry <relpath> --out <dir>
  fardbundle verify --bundle <bundle.json> --out <dir>
  fardbundle run    --bundle <bundle.json> --out <dir>

OUTPUT LAYOUT (matches tests/lang_gates_v1/run_g15_g21.sh):
  build writes:
    <OUT>/bundle/bundle.json
    <OUT>/bundle/imports.lock.json
    <OUT>/bundle/files/<relpath> (verbatim sources)

  verify reads bundle + (optional) sibling imports.lock.json
    exits 0 on success, nonzero on failure
    on failure writes <OUT>/error.json

  run reads bundle + (optional) sibling imports.lock.json
    extracts sources to <OUT>/extract/<relpath>
    runs: fardrun run <OUT>/extract/<entry> --out <OUT>
    on failure writes <OUT>/error.json

DIAGNOSTICS CONTRACT:
  error.json = { ok:false, code, message, details? }
  stderr includes grep-friendly tokens:
    LOCK_MISMATCH
    entry missing in manifest
    unsafe file path
"#;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print!("{}", HELP);
        std::process::exit(2);
    }

    let cmd = args[1].as_str();
    let rest = &args[2..];

    let r = match cmd {
        "build" => cmd_build(rest),
        "verify" => cmd_verify(rest),
        "run" => cmd_run(rest),
        "--help" | "-h" | "help" => {
            print!("{}", HELP);
            Ok(())
        }
        _ => Err(FardBundleError::new(
            "ERROR_BUNDLE_CLI",
            format!("unknown subcommand: {cmd}"),
        )),
    };

    match r {
        Ok(()) => {}
        Err(e) => {
            if let Some(out_dir) = e.out_dir.as_ref() {
                let _ = write_error_json(out_dir, &e.code, &e.message, e.details.as_ref());
            }
            eprintln!("{}", e.message);
            std::process::exit(1);
        }
    }
}

#[derive(Debug)]
struct FardBundleError {
    code: String,
    message: String,
    out_dir: Option<PathBuf>,
    details: Option<Value>,
}

impl FardBundleError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            out_dir: None,
            details: None,
        }
    }
    fn with_out(mut self, out: &Path) -> Self {
        self.out_dir = Some(out.to_path_buf());
        self
    }
    fn with_details(mut self, v: Value) -> Self {
        self.details = Some(v);
        self
    }
}

fn write_error_json(
    out_dir: &Path,
    code: &str,
    message: &str,
    details: Option<&Value>,
) -> io::Result<()> {
    fs::create_dir_all(out_dir)?;
    let err_path = out_dir.join("error.json");
    let payload = {
        let mut m = std::collections::BTreeMap::new();
        m.insert("code".to_string(), Value::Str(code.to_string()));
        m.insert("message".to_string(), Value::Str(message.to_string()));
        m.insert("ok".to_string(), Value::Bool(false));
        if let Some(d) = details {
            m.insert("details".to_string(), d.clone());
        }
        Value::Object(m)
    };
    let bytes = to_string_pretty(&payload).into_bytes();
    fs::write(err_path, bytes)?;
    Ok(())
}

fn parse_flag(args: &[String], name: &str) -> Option<String> {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == name {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            } else {
                return None;
            }
        }
        i += 1;
    }
    None
}

fn must_flag(args: &[String], name: &str, out: Option<&Path>) -> Result<String, FardBundleError> {
    parse_flag(args, name).ok_or_else(|| {
        let mut e =
            FardBundleError::new("ERROR_BUNDLE_CLI", format!("missing required flag: {name}"));
        if let Some(o) = out {
            e = e.with_out(o);
        }
        e
    })
}

fn cmd_build(rest: &[String]) -> Result<(), FardBundleError> {
    let out_s = parse_flag(rest, "--out")
        .ok_or_else(|| FardBundleError::new("ERROR_BUNDLE_CLI", "missing required flag: --out"))?;
    let out_dir = PathBuf::from(out_s);

    let root_s = must_flag(rest, "--root", Some(&out_dir))?;
    let entry_s = must_flag(rest, "--entry", Some(&out_dir))?;

    let root = PathBuf::from(root_s);
    let entry = entry_s;

    if !root.is_dir() {
        return Err(
            FardBundleError::new("ERROR_BUNDLE_BUILD", "root is not a directory")
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("root".to_string(), Value::Str(format!("{}",root.to_string_lossy()))); Value::Object(_m) }),
        );
    }
    if !is_safe_rel_path(&entry) {
        return Err(
            FardBundleError::new("ERROR_BUNDLE_BUILD", "unsafe entry path")
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("entry".to_string(), Value::Str(format!("{}",entry))); Value::Object(_m) }),
        );
    }

    let bundle_dir = out_dir.join("bundle");
    let files_dir = bundle_dir.join("files");
    fs::create_dir_all(&files_dir).map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_BUILD",
            format!("failed to create out dirs: {e}"),
        )
        .with_out(&out_dir)
    })?;

    let mut rels: Vec<String> = Vec::new();
    collect_fard_files(&root, &mut rels).map_err(|e| {
        FardBundleError::new("ERROR_BUNDLE_BUILD", format!("file walk failed: {e}"))
            .with_out(&out_dir)
    })?;
    rels.sort();

    let entry_path = root.join(&entry);
    if !entry_path.is_file() {
        return Err(
            FardBundleError::new("ERROR_BUNDLE_BUILD", "entry does not exist under root")
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("entry".to_string(), Value::Str(format!("{}",entry))); _m.insert("root".to_string(), Value::Str(format!("{}",root.to_string_lossy()))); Value::Object(_m) }),
        );
    }

    let mut files: Vec<Value> = Vec::new();

    for rel in rels.iter() {
        if !is_safe_rel_path(rel) {
            return Err(
                FardBundleError::new("ERROR_BUNDLE_BUILD", "unsafe file path")
                    .with_out(&out_dir)
                    .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) }),
            );
        }
        let src = root.join(rel);
        if !src.is_file() {
            continue;
        }
        let bytes = fs::read(&src).map_err(|e| {
            FardBundleError::new("ERROR_BUNDLE_BUILD", format!("read failed: {e}"))
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) })
        })?;
        let d = sha256_hex(&bytes);

        let dst = files_dir.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                FardBundleError::new("ERROR_BUNDLE_BUILD", format!("mkdir failed: {e}"))
                    .with_out(&out_dir)
                    .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) })
            })?;
        }
        fs::write(&dst, &bytes).map_err(|e| {
            FardBundleError::new("ERROR_BUNDLE_BUILD", format!("write failed: {e}"))
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) })
        })?;

        let mut rec = BTreeMap::<String, Value>::new();
        rec.insert("path".to_string(), Value::Str(rel.to_string()));
        rec.insert("sha256".to_string(), Value::Str(format!("sha256:{d}")));
        rec.insert(
            "len".to_string(),
            Value::Int(bytes.len() as i64),
        );
        files.push(Value::Object(rec.into_iter().collect()));
    }

    if !files
        .iter()
        .any(|v| v.get("path").and_then(|x| x.as_str()) == Some(entry.as_str()))
    {
        let bytes = fs::read(&entry_path).map_err(|e| {
            FardBundleError::new("ERROR_BUNDLE_BUILD", format!("read entry failed: {e}"))
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("entry".to_string(), Value::Str(format!("{}",entry))); Value::Object(_m) })
        })?;
        let d = sha256_hex(&bytes);
        let dst = files_dir.join(&entry);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                FardBundleError::new("ERROR_BUNDLE_BUILD", format!("mkdir failed: {e}"))
                    .with_out(&out_dir)
                    .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("entry".to_string(), Value::Str(format!("{}",entry))); Value::Object(_m) })
            })?;
        }
        fs::write(&dst, &bytes).map_err(|e| {
            FardBundleError::new("ERROR_BUNDLE_BUILD", format!("write entry failed: {e}"))
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("entry".to_string(), Value::Str(format!("{}",entry))); Value::Object(_m) })
        })?;

        let mut rec = BTreeMap::<String, Value>::new();
        rec.insert("path".to_string(), Value::Str(entry.clone()));
        rec.insert("sha256".to_string(), Value::Str(format!("sha256:{d}")));
        rec.insert(
            "len".to_string(),
            Value::Int(bytes.len() as i64),
        );
        files.push(Value::Object(rec.into_iter().collect()));
    }

    files.sort_by(|a, b| {
        let pa = a.get("path").and_then(|x| x.as_str()).unwrap_or("");
        let pb = b.get("path").and_then(|x| x.as_str()).unwrap_or("");
        pa.cmp(pb)
    });

    let mut manifest = BTreeMap::<String, Value>::new();
    manifest.insert(
        "schema".to_string(),
        Value::Str("fard.bundle.v0_1".to_string()),
    );
    manifest.insert("entry".to_string(), Value::Str(entry.clone()));
    manifest.insert("files".to_string(), Value::Array(files.clone()));

    let manifest_bytes = to_string(&Value::Object(manifest.clone())).into_bytes();
    let bundle_digest = sha256_hex(&manifest_bytes);
    manifest.insert(
        "bundle_digest".to_string(),
        Value::Str(format!("sha256:{bundle_digest}")),
    );

    let bundle_json_path = bundle_dir.join("bundle.json");
    let bundle_bytes = to_string_pretty(&Value::Object(manifest.clone())).into_bytes();
    fs::write(&bundle_json_path, bundle_bytes).map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_BUILD",
            format!("write bundle.json failed: {e}"),
        )
        .with_out(&out_dir)
    })?;

    let lock = {
        let mut m = std::collections::BTreeMap::new();
        m.insert("bundle_digest".to_string(), Value::Str(format!("sha256:{bundle_digest}")));
        m.insert("schema".to_string(), Value::Str("fard.imports_lock.v0_1".to_string()));
        Value::Object(m)
    };
    let lock_path = bundle_dir.join("imports.lock.json");
    let lock_bytes = to_string_pretty(&lock).into_bytes();
    fs::write(&lock_path, lock_bytes).map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_BUILD",
            format!("write imports.lock.json failed: {e}"),
        )
        .with_out(&out_dir)
    })?;

    Ok(())
}

fn cmd_verify(rest: &[String]) -> Result<(), FardBundleError> {
    let out_s = parse_flag(rest, "--out")
        .ok_or_else(|| FardBundleError::new("ERROR_BUNDLE_CLI", "missing required flag: --out"))?;
    let out_dir = PathBuf::from(out_s);

    let bundle_arg = must_flag(rest, "--bundle", Some(&out_dir))?;
    let bundle_path = PathBuf::from(&bundle_arg);
    let bundle_abs = abs_path_string(&bundle_path);
    let v = load_json(&bundle_path).map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            format!("failed to read bundle.json: {e}"),
        )
        .with_out(&out_dir)
        .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("bundle_abs".to_string(), Value::Str(format!("{}",bundle_abs))); _m.insert("bundle_arg".to_string(), Value::Str(format!("{}",bundle_arg))); _m.insert("bundle_path".to_string(), Value::Str(format!("{}",bundle_path.to_string_lossy()))); Value::Object(_m) })
    })?;

    verify_bundle_value(&bundle_path, &v).map_err(|mut e| {
        e = e.with_out(&out_dir);
        e
    })?;

    if let Some(lock_path) = sibling_lock_path(&bundle_path) {
        if lock_path.exists() {
            let lock_v = load_json(&lock_path).map_err(|e| {
                FardBundleError::new(
                    "ERROR_BUNDLE_VERIFY",
                    format!("failed to read imports.lock.json: {e}"),
                )
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("lock".to_string(), Value::Str(format!("{}",lock_path.to_string_lossy()))); Value::Object(_m) })
            })?;
            verify_lock_value(&v, &lock_v).map_err(|mut e| {
                e = e.with_out(&out_dir);
                e
            })?;
        }
    }

    Ok(())
}

fn cmd_run(rest: &[String]) -> Result<(), FardBundleError> {
    let out_s = parse_flag(rest, "--out")
        .ok_or_else(|| FardBundleError::new("ERROR_BUNDLE_CLI", "missing required flag: --out"))?;
    let out_dir = PathBuf::from(out_s);

    let bundle_arg = must_flag(rest, "--bundle", Some(&out_dir))?;
    let bundle_path = PathBuf::from(&bundle_arg);
    let bundle_abs = abs_path_string(&bundle_path);
    let v = load_json(&bundle_path).map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            format!("failed to read bundle.json: {e}"),
        )
        .with_out(&out_dir)
        .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("bundle_abs".to_string(), Value::Str(format!("{}",bundle_abs))); _m.insert("bundle_arg".to_string(), Value::Str(format!("{}",bundle_arg))); _m.insert("bundle_path".to_string(), Value::Str(format!("{}",bundle_path.to_string_lossy()))); Value::Object(_m) })
    })?;

    verify_bundle_value(&bundle_path, &v).map_err(|mut e| {
        e = e.with_out(&out_dir);
        e
    })?;

    if let Some(lock_path) = sibling_lock_path(&bundle_path) {
        if lock_path.exists() {
            let lock_v = load_json(&lock_path).map_err(|e| {
                FardBundleError::new(
                    "ERROR_BUNDLE_VERIFY",
                    format!("failed to read imports.lock.json: {e}"),
                )
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("lock".to_string(), Value::Str(format!("{}",lock_path.to_string_lossy()))); Value::Object(_m) })
            })?;
            verify_lock_value(&v, &lock_v).map_err(|mut e| {
                e = e.with_out(&out_dir);
                e
            })?;
        }
    }

    let extract_root = out_dir.join("extract");
    if extract_root.exists() {
        let _ = fs::remove_dir_all(&extract_root);
    }
    fs::create_dir_all(&extract_root).map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_RUN",
            format!("failed to create extract dir: {e}"),
        )
        .with_out(&out_dir)
    })?;

    extract_bundle_sources(&bundle_path, &v, &extract_root).map_err(|mut e| {
        e = e.with_out(&out_dir);
        e
    })?;

    let entry = v
        .get("entry")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let entry_path = extract_root.join(&entry);

    let fardrun = sibling_exe("fardrun").ok_or_else(|| {
        FardBundleError::new(
            "ERROR_BUNDLE_RUN",
            "could not locate sibling fardrun executable",
        )
        .with_out(&out_dir)
        .with_details(
            { let mut _m = std::collections::BTreeMap::new(); _m.insert("hint".to_string(), Value::Str(format!("{}","build target/debug/fardrun and ensure it's next to fardbundle"))); Value::Object(_m) },
        )
    })?;

    let mut child = Command::new(fardrun)
        .arg("run")
        .arg(entry_path.as_os_str())
        .arg("--out")
        .arg(out_dir.as_os_str())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            FardBundleError::new("ERROR_BUNDLE_RUN", format!("failed to spawn fardrun: {e}"))
                .with_out(&out_dir)
        })?;

    let status = child.wait().map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_RUN",
            format!("failed to wait on fardrun: {e}"),
        )
        .with_out(&out_dir)
    })?;

    if !status.success() {
        return Err(
            FardBundleError::new("ERROR_BUNDLE_RUN", "fardrun returned nonzero status")
                .with_out(&out_dir)
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("status".to_string(), Value::Int(status.code().unwrap_or(-1) as i64)); Value::Object(_m) }),
        );
    }

    Ok(())
}

fn load_json(path: &Path) -> io::Result<Value> {
    let bytes = fs::read(path)?;
    let v = from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e}")))?;
    Ok(v)
}

fn abs_path_string(p: &Path) -> String {
    match fs::canonicalize(p) {
        Ok(ap) => ap.to_string_lossy().to_string(),
        Err(_) => p.to_string_lossy().to_string(),
    }
}
fn sibling_lock_path(bundle_json: &Path) -> Option<PathBuf> {
    let dir = bundle_json.parent()?.to_path_buf();
    Some(dir.join("imports.lock.json"))
}

fn verify_bundle_value(bundle_path: &Path, v: &Value) -> Result<(), FardBundleError> {
    let schema = v.get("schema").and_then(|x| x.as_str()).unwrap_or("");
    if schema != "fard.bundle.v0_1" {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            format!("schema mismatch: expected fard.bundle.v0_1 got {schema}"),
        ));
    }

    let entry = v.get("entry").and_then(|x| x.as_str()).unwrap_or("");
    if entry.is_empty() {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            "entry missing in manifest",
        ));
    }
    if !is_safe_rel_path(entry) {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            "unsafe file path",
        ));
    }

    let files = v
        .get("files")
        .and_then(|x| x.as_array())
        .ok_or_else(|| FardBundleError::new("ERROR_BUNDLE_VERIFY", "missing files array"))?;

    if !files
        .iter()
        .any(|f| f.get("path").and_then(|x| x.as_str()) == Some(entry))
    {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            "entry missing in manifest",
        ));
    }

    for f in files.iter() {
        let p = f.get("path").and_then(|x| x.as_str()).unwrap_or("");
        if !is_safe_rel_path(p) {
            return Err(FardBundleError::new(
                "ERROR_BUNDLE_VERIFY",
                "unsafe file path",
            ));
        }
    }

    let want = v
        .get("bundle_digest")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    if want.is_empty() {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            "missing bundle_digest",
        ));
    }

    let computed = compute_bundle_digest(v).map_err(|e| {
        FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            format!("bundle digest compute failed: {e}"),
        )
    })?;

    if want != computed {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            "DIGEST_MISMATCH: bundle_digest mismatch",
        )
        .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("computed".to_string(), Value::Str(format!("{}",computed))); _m.insert("expected".to_string(), Value::Str(format!("{}",want))); Value::Object(_m) }));
    }

    let bundle_dir = bundle_path.parent().ok_or_else(|| {
        FardBundleError::new("ERROR_BUNDLE_VERIFY", "bundle.json has no parent dir")
    })?;
    let files_root = bundle_dir.join("files");

    for f in files.iter() {
        let rel = f.get("path").and_then(|x| x.as_str()).unwrap_or("");
        let sha = f.get("sha256").and_then(|x| x.as_str()).unwrap_or("");
        if rel.is_empty() || sha.is_empty() {
            return Err(FardBundleError::new(
                "ERROR_BUNDLE_VERIFY",
                "file record missing path/sha256",
            ));
        }
        let src = files_root.join(rel);
        if !src.is_file() {
            return Err(FardBundleError::new(
                "ERROR_BUNDLE_VERIFY",
                "bundled file missing on disk",
            )
            .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("missing".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) }));
        }
        let bytes = fs::read(&src).map_err(|e| {
            FardBundleError::new(
                "ERROR_BUNDLE_VERIFY",
                format!("read bundled file failed: {e}"),
            )
            .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) })
        })?;
        let d = format!("sha256:{}", sha256_hex(&bytes));
        if d != sha {
            return Err(FardBundleError::new(
                "ERROR_BUNDLE_VERIFY",
                "DIGEST_MISMATCH: bundled file digest mismatch",
            )
            .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("computed".to_string(), Value::Str(format!("{}",d))); _m.insert("expected".to_string(), Value::Str(format!("{}",sha))); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) }));
        }
    }

    Ok(())
}

fn verify_lock_value(bundle_v: &Value, lock_v: &Value) -> Result<(), FardBundleError> {
    let schema = lock_v.get("schema").and_then(|x| x.as_str()).unwrap_or("");
    if schema != "fard.imports_lock.v0_1" {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            "imports.lock schema mismatch",
        ));
    }

    let want = lock_v
        .get("bundle_digest")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let bundle_digest = bundle_v
        .get("bundle_digest")
        .and_then(|x| x.as_str())
        .unwrap_or("");

    if want.is_empty() || bundle_digest.is_empty() {
        return Err(FardBundleError::new(
            "ERROR_BUNDLE_VERIFY",
            "missing bundle_digest in lock or bundle",
        ));
    }

    if want != bundle_digest {
        return Err(
            FardBundleError::new("ERROR_BUNDLE_LOCK_MISMATCH", "LOCK_MISMATCH").with_details(
                { let mut _m = std::collections::BTreeMap::new(); _m.insert("bundle_bundle_digest".to_string(), Value::Str(format!("{}",bundle_digest))); _m.insert("lock_bundle_digest".to_string(), Value::Str(format!("{}",want))); Value::Object(_m) },
            ),
        );
    }

    Ok(())
}

fn compute_bundle_digest(v: &Value) -> Result<String, String> {
    let schema = v.get("schema").cloned().unwrap_or(Value::Null);
    let entry = v.get("entry").cloned().unwrap_or(Value::Null);
    let files = v.get("files").cloned().unwrap_or(Value::Null);

    let mut m = BTreeMap::<String, Value>::new();
    m.insert("schema".to_string(), schema);
    m.insert("entry".to_string(), entry);
    m.insert("files".to_string(), files);

    let bytes = to_string(&Value::Object(m)).into_bytes();
    let d = sha256_hex(&bytes);
    Ok(format!("sha256:{d}"))
}

fn extract_bundle_sources(
    bundle_path: &Path,
    v: &Value,
    extract_root: &Path,
) -> Result<(), FardBundleError> {
    let bundle_dir = bundle_path.parent().ok_or_else(|| {
        FardBundleError::new("ERROR_BUNDLE_VERIFY", "bundle.json has no parent dir")
            .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("bundle".to_string(), Value::Str(format!("{}",bundle_path.to_string_lossy()))); Value::Object(_m) })
    })?;
    let files_root = bundle_dir.join("files");

    let files = v.get("files").and_then(|x| x.as_array()).ok_or_else(|| {
        FardBundleError::new("ERROR_BUNDLE_VERIFY", "files missing in manifest")
            .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("bundle".to_string(), Value::Str(format!("{}",bundle_path.to_string_lossy()))); Value::Object(_m) })
    })?;

    for rec in files.iter() {
        let rel = rec.get("path").and_then(|x| x.as_str()).unwrap_or("");
        if rel.is_empty() {
            return Err(
                FardBundleError::new("ERROR_BUNDLE_VERIFY", "file record missing path")
                    .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("bundle".to_string(), Value::Str(format!("{}",bundle_path.to_string_lossy()))); Value::Object(_m) }),
            );
        }
        if !is_safe_rel_path(rel) {
            return Err(
                FardBundleError::new("ERROR_BUNDLE_VERIFY", "unsafe file path")
                    .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) }),
            );
        }

        let src = files_root.join(rel);
        if !src.is_file() {
            return Err(
                FardBundleError::new("ERROR_BUNDLE_VERIFY", "entry missing in manifest")
                    .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("missing".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) }),
            );
        }

        let bytes = fs::read(&src).map_err(|e| {
            FardBundleError::new(
                "ERROR_BUNDLE_VERIFY",
                format!("failed to read bundled file: {e}"),
            )
            .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); _m.insert("src".to_string(), Value::Str(format!("{}",src.to_string_lossy()))); Value::Object(_m) })
        })?;

        let dst = extract_root.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                FardBundleError::new(
                    "ERROR_BUNDLE_RUN",
                    format!("failed to create extract subdir: {e}"),
                )
                .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) })
            })?;
        }
        fs::write(&dst, &bytes).map_err(|e| {
            FardBundleError::new(
                "ERROR_BUNDLE_RUN",
                format!("failed to write extracted file: {e}"),
            )
            .with_details({ let mut _m = std::collections::BTreeMap::new(); _m.insert("dst".to_string(), Value::Str(format!("{}",dst.to_string_lossy()))); _m.insert("path".to_string(), Value::Str(format!("{}",rel))); Value::Object(_m) })
        })?;
    }

    Ok(())
}

fn collect_fard_files(root: &Path, out: &mut Vec<String>) -> io::Result<()> {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for ent in fs::read_dir(&dir)? {
            let ent = ent?;
            let p = ent.path();
            if p.is_dir() {
                let name = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
                if name == "target" || name == ".git" {
                    continue;
                }
                stack.push(p);
            } else if p.is_file() {
                if p.extension().and_then(|x| x.to_str()) == Some("fard") {
                    if let Ok(rel) = p.strip_prefix(root) {
                        let rel_s = rel.to_string_lossy().replace('\\', "/");
                        out.push(rel_s);
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_safe_rel_path(p: &str) -> bool {
    if p.is_empty() {
        return false;
    }
    if p.starts_with('/') {
        return false;
    }
    if p.contains('\\') {
        return false;
    }
    if p.contains(':') {
        return false;
    }
    let parts: Vec<&str> = p.split('/').collect();
    for part in parts {
        if part.is_empty() {
            return false;
        }
        if part == "." || part == ".." {
            return false;
        }
    }
    true
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = valuecore::Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    to_hex(&out[..])
}

fn to_hex(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(LUT[(b >> 4) as usize] as char);
        s.push(LUT[(b & 0x0f) as usize] as char);
    }
    s
}

fn sibling_exe(name: &str) -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    let cand = dir.join(name);
    if cand.exists() {
        return Some(cand);
    }
    let cand2 = dir.join(format!("{name}.exe"));
    if cand2.exists() {
        return Some(cand2);
    }
    None
}
