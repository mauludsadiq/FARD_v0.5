use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
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
    eprintln!("usage: fardpkg publish --root <pkg_root> --registry <registry_dir> --out <out_dir>");
    std::process::exit(2);
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 1 {
        usage();
    }
    if args[0] != "publish" {
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

    let manifest_path = root.join("fard.pkg.json");
    let m = read_json(&manifest_path)?;
    if m.get("schema").and_then(|x| x.as_str()) != Some("fard.package.v0_1") {
        bail!("ERROR_PKG bad schema in fard.pkg.json");
    }
    let name = m
        .get("name")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("missing name"))?;
    let ver = m
        .get("version")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("missing version"))?;

    let base = registry.join("pkgs").join(name).join(ver);
    let files_dir = base.join("files");
    fs::create_dir_all(&files_dir)?;

    // entrypoints: module_id -> path
    let eps = m
        .get("entrypoints")
        .and_then(|x| x.as_object())
        .ok_or_else(|| anyhow!("missing entrypoints"))?;
    let exports = m
        .get("exports")
        .and_then(|x| x.as_object())
        .cloned()
        .unwrap_or_default();

    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    let mut copied: BTreeSet<String> = BTreeSet::new();

    for (mod_id, rel_path_v) in eps.iter() {
        let rel_path = rel_path_v
            .as_str()
            .ok_or_else(|| anyhow!("entrypoints must be strings"))?;
        let src_path = root.join(rel_path);
        let bytes = fs::read(&src_path)
            .with_context(|| format!("missing module file: {}", src_path.display()))?;
        let cid = sha256_bytes(&bytes);
        digests.insert(mod_id.to_string(), cid);

        // copy to registry under files/<rel_path> (preserve rel path)
        let dst_path = files_dir.join(rel_path);
        fs::create_dir_all(dst_path.parent().unwrap())?;
        fs::write(&dst_path, &bytes)?;
        copied.insert(rel_path.to_string());
    }

    // package digest = sha256 of deterministic JSON of digests map
    let digests_json = json!({
        "schema": "fard.pkg_digests.v0_1",
        "package": format!("{name}@{ver}"),
        "modules": digests
    });
    let dig_bytes = serde_json::to_vec(&digests_json)?;
    let pkg_digest = sha256_bytes(&dig_bytes);

    // write registry files
    write_json(
        &base.join("package.json"),
        &json!({
            "schema": "fard.package_record.v0_1",
            "name": name,
            "version": ver,
            "entrypoints": m.get("entrypoints").cloned().unwrap_or(json!({})),
            "exports": exports,
            "package_digest": pkg_digest
        }),
    )?;
    write_json(&base.join("digests.json"), &digests_json)?;

    // write out summary
    write_json(
        &out.join("publish.json"),
        &json!({
            "ok": true,
            "schema": "fard.publish_out.v0_1",
            "name": name,
            "version": ver,
            "package_digest": pkg_digest,
            "copied_files": copied.into_iter().collect::<Vec<_>>()
        }),
    )?;

    Ok(())
}
