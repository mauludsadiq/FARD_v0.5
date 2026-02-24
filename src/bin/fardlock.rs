use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use valuecore::json::{JsonVal as J, escape_string, from_slice, to_string, to_string_pretty};

fn sha256_bytes(bytes: &[u8]) -> String {
        let mut h = valuecore::Sha256::new();
    h.update(bytes);
    format!("sha256:{}", valuecore::hex_lower(&h.finalize()))
}

fn read_json(p: &Path) -> Result<J> {
    let b = fs::read(p).with_context(|| format!("cannot read: {}", p.display()))?;
    from_slice(&b).with_context(|| format!("bad json: {}", p.display()))
}

fn write_json(p: &Path, v: &J) -> Result<()> {
    let s = to_string_pretty(v);
    fs::create_dir_all(p.parent().unwrap())?;
    fs::write(p, s.as_bytes())?;
    Ok(())
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
            J::Float(f) => {
                let s = format!("{}", f);
                if s.contains('+') {
                    bail!("M5_CANON_NUM_PLUS");
                }
                if s.ends_with(".0") {
                    bail!("M5_CANON_NUM_DOT0");
                }
                out.push_str(&s);
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
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&escape_string(k));
                    out.push(':');
                    canon_value(&m[*k], out)?;
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

fn usage() -> ! {
    eprintln!("usage:");
    eprintln!("  fardlock gen --root <app_root> --registry <registry_dir> --out <out_dir>");
    eprintln!("  fardlock show-preimage --out <run_out_dir>");
    std::process::exit(2);
}

fn get_out(args: &[String]) -> Result<PathBuf> {
    let mut out: Option<PathBuf> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| anyhow!("missing --out value"))?;
                out = Some(PathBuf::from(v));
            }
            _ => {}
        }
        i += 1;
    }
    out.ok_or_else(|| anyhow!("missing --out"))
}

fn cmd_show_preimage(args: &[String]) -> Result<()> {
    let outdir = get_out(args)?;
    let dig = read_json(&outdir.join("digests.json"))?;
    let dobj = dig
        .as_object()
        .ok_or_else(|| anyhow!("DIGESTS_NOT_OBJECT"))?;

    let files = dobj
        .get("files")
        .ok_or_else(|| anyhow!("M5_MISSING_files"))?
        .clone();
    let ok = dobj
        .get("ok")
        .ok_or_else(|| anyhow!("M5_MISSING_ok"))?
        .clone();
    let runtime_version = dobj
        .get("runtime_version")
        .ok_or_else(|| anyhow!("M5_MISSING_runtime_version"))?
        .clone();
    let stdlib_root_digest = dobj
        .get("stdlib_root_digest")
        .ok_or_else(|| anyhow!("M5_MISSING_stdlib_root_digest"))?
        .clone();
    let trace_format_version = dobj
        .get("trace_format_version")
        .ok_or_else(|| anyhow!("M5_MISSING_trace_format_version"))?
        .clone();

    let preimage = {
        let mut m = std::collections::BTreeMap::new();
        m.insert("files".to_string(), files);
        m.insert("ok".to_string(), ok);
        m.insert("runtime_version".to_string(), runtime_version);
        m.insert("stdlib_root_digest".to_string(), stdlib_root_digest);
        m.insert("trace_format_version".to_string(), trace_format_version);
        J::Object(m)
    };

    let canon = canon_json(&preimage)?;
    print!("{}", canon);
    Ok(())
}

fn cmd_gen(args: &[String]) -> Result<()> {
    let mut root: Option<PathBuf> = None;
    let mut registry: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                root = Some(PathBuf::from(
                    args.get(i).ok_or_else(|| anyhow!("missing --root value"))?,
                ));
            }
            "--registry" => {
                i += 1;
                registry = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| anyhow!("missing --registry value"))?,
                ));
            }
            "--out" => {
                i += 1;
                out = Some(PathBuf::from(
                    args.get(i).ok_or_else(|| anyhow!("missing --out value"))?,
                ));
            }
            _ => bail!("unknown arg: {}", args[i]),
        }
        i += 1;
    }

    let root = root.ok_or_else(|| anyhow!("missing --root"))?;
    let out = out.ok_or_else(|| anyhow!("missing --out"))?;

    fs::remove_dir_all(&out).ok();
    fs::create_dir_all(&out)?;
    let registry_dir = registry.unwrap_or_else(|| root.join("registry"));

    let appm = read_json(&root.join("fard.app.json"))?;
    let app_pkg = appm
        .get("package")
        .and_then(|x| x.as_object())
        .ok_or_else(|| anyhow!("ERROR_LOCK missing package"))?;
    let app_pkg_name = app_pkg
        .get("name")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("ERROR_LOCK missing package.name"))?
        .to_string();
    let app_pkg_ver = app_pkg
        .get("version")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("ERROR_LOCK missing package.version"))?
        .to_string();
    if appm.get("schema").and_then(|x| x.as_str()) != Some("fard.app.v0_1") {
        bail!("ERROR_LOCK bad schema in fard.app.json");
    }
    let entry = appm
        .get("entry")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("missing entry"))?;
    let entry_path = root.join(entry);
    let src = fs::read_to_string(&entry_path)
        .with_context(|| format!("missing entry file: {}", entry_path.display()))?;

    let re = Regex::new(r#"import\("pkg:([a-zA-Z0-9_\-]+)@([0-9]+\.[0-9]+\.[0-9]+)/([^"]+)"\)"#)?;
    let mut modules: BTreeMap<String, J> = BTreeMap::new();
    let mut packages: BTreeMap<String, String> = BTreeMap::new();

    for cap in re.captures_iter(&src) {
        let name = cap.get(1).unwrap().as_str();
        let ver = cap.get(2).unwrap().as_str();
        let mod_id = cap.get(3).unwrap().as_str();

        let base = registry_dir.join("pkgs").join(name).join(ver);
        let pkg_record = read_json(&base.join("package.json"))
            .with_context(|| format!("missing package record for {name}@{ver}"))?;
        let pkg_digest = pkg_record
            .get("package_digest")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if pkg_digest.is_empty() {
            bail!("ERROR_LOCK missing package_digest for {name}@{ver}");
        }
        packages.insert(format!("{name}@{ver}"), pkg_digest);

        let dig = read_json(&base.join("digests.json"))
            .with_context(|| format!("missing digests for {name}@{ver}"))?;
        let ms = dig
            .get("modules")
            .and_then(|x| x.as_object())
            .ok_or_else(|| anyhow!("bad digests.json"))?;
        let want = ms.get(mod_id).and_then(|x| x.as_str()).unwrap_or("");
        if want.is_empty() {
            bail!("ERROR_LOCK missing digest for module {mod_id} in {name}@{ver}");
        }

        modules.insert(
            format!("pkg:{name}@{ver}/{mod_id}"),
{ let mut _m = std::collections::BTreeMap::new(); _m.insert("digest".to_string(), J::Str(want.to_string())); J::Object(_m) },
        );
    }

    let reg_commit = {
        let mut m = std::collections::BTreeMap::new();
        m.insert("packages".to_string(), J::Object(packages.into_iter().map(|(k,v)| (k, J::Str(v))).collect()));
        m.insert("schema".to_string(), J::Str("fard.registry_commit.v0_1".to_string()));
        J::Object(m)
    };
    let reg_digest = sha256_bytes(&to_string(&reg_commit).into_bytes());

    write_json(
        &out.join("fard.lock.json"),
        &{
            let mut pkg = std::collections::BTreeMap::new();
            pkg.insert("name".to_string(), J::Str(app_pkg_name.clone()));
            pkg.insert("version".to_string(), J::Str(app_pkg_ver.clone()));
            let mut m = std::collections::BTreeMap::new();
            m.insert("app_entry".to_string(), J::Str(entry.to_string()));
            m.insert("modules".to_string(), J::Object(modules));
            m.insert("package".to_string(), J::Object(pkg));
            m.insert("packages".to_string(), reg_commit.get("packages").cloned().unwrap_or(J::Object(std::collections::BTreeMap::new())));
            m.insert("registry_root_digest".to_string(), J::Str(reg_digest.clone()));
            m.insert("schema".to_string(), J::Str("fard.lock.v0_1".to_string()));
            J::Object(m)
        },
    )?;

    let lock_path = out.join("fard.lock.json");
    let lock_bytes = fs::read(&lock_path)
        .with_context(|| format!("missing lock file after write: {}", lock_path.display()))?;
    let lock_cid = sha256_bytes(&lock_bytes);
    fs::write(out.join("fard.lock.json.cid"), format!("{}\n", lock_cid))?;

    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    let sub = args[0].as_str();
    let rest = &args[1..];

    match sub {
        "gen" => cmd_gen(rest),
        "show-preimage" => cmd_show_preimage(rest),
        _ => {
            usage();
        }
    }
}
