use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde_json::{json, Value as J};

fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{:x}", h.finalize())
}

fn read_json(p: &Path) -> Result<J> {
    let b = fs::read(p).with_context(|| format!("cannot read: {}", p.display()))?;
    Ok(serde_json::from_slice(&b).with_context(|| format!("bad json: {}", p.display()))?)
}

fn write_json(p: &Path, v: &J) -> Result<()> {
    let s = serde_json::to_vec_pretty(v)?;
    fs::create_dir_all(p.parent().unwrap())?;
    fs::write(p, s)?;
    Ok(())
}

fn usage() -> ! {
    eprintln!("usage: fardlock gen --root <app_root> --registry <registry_dir> --out <out_dir>");
    std::process::exit(2);
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 1 {
        usage();
    }
    if args[0] != "gen" {
        usage();
    }

    let mut root: Option<PathBuf> = None;
    let mut registry: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;

    let mut i = 1;
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
    let registry = registry.ok_or_else(|| anyhow!("missing --registry"))?;
    let out = out.ok_or_else(|| anyhow!("missing --out"))?;

    fs::remove_dir_all(&out).ok();
    fs::create_dir_all(&out)?;

    let appm = read_json(&root.join("fard.app.json"))?;
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

    // parse imports of the form: import("pkg:name@ver/mod") as X
    let re = Regex::new(r#"import\("pkg:([a-zA-Z0-9_\-]+)@([0-9]+\.[0-9]+\.[0-9]+)/([^"]+)"\)"#)?;
    let mut modules: BTreeMap<String, String> = BTreeMap::new();
    let mut packages: BTreeMap<String, String> = BTreeMap::new();

    for cap in re.captures_iter(&src) {
        let name = cap.get(1).unwrap().as_str();
        let ver = cap.get(2).unwrap().as_str();
        let mod_id = cap.get(3).unwrap().as_str(); // e.g. std/math

        let base = registry.join("pkgs").join(name).join(ver);
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

        modules.insert(format!("pkg:{name}@{ver}/{mod_id}"), want.to_string());
    }

    // registry root digest (commit to the digests map deterministically)
    let reg_commit = json!({"schema":"fard.registry_commit.v0_1","packages":packages});
    let reg_digest = sha256_bytes(&serde_json::to_vec(&reg_commit)?);

    write_json(
        &out.join("fard.lock.json"),
        &json!({
            "schema": "fard.lock.v0_1",
            "app_entry": entry,
            "registry_root_digest": reg_digest,
            "packages": reg_commit.get("packages").cloned().unwrap_or(json!({})),
            "modules": modules
        }),
    )?;

    Ok(())
}
