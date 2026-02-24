use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use valuecore::json::{JsonVal as J, from_slice, to_string_pretty};
use std::collections::BTreeMap as JMap;

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
    let schema = m.get("schema").and_then(|x| x.as_str()).unwrap_or("");
    if schema != "fard.package.v0_1" && schema != "fard.pkg.v0_1" {
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
    let eps_val = m
        .get("entrypoints")
        .ok_or_else(|| anyhow!("missing entrypoints"))?;
    let eps = eps_val.as_object()
        .ok_or_else(|| anyhow!("entrypoints must be object"))?;
    let exports = m
        .get("exports")
        .cloned()
        .unwrap_or(J::Object(std::collections::BTreeMap::new()));

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
    let digests_json = {
        let mut m = JMap::new();
        m.insert("modules".to_string(), J::Object(digests.into_iter().map(|(k,v)| (k, J::Str(v))).collect()));
        m.insert("package".to_string(), J::Str(format!("{name}@{ver}")));
        m.insert("schema".to_string(), J::Str("fard.pkg_digests.v0_1".to_string()));
        J::Object(m)
    };
    let dig_bytes = valuecore::json::to_string(&digests_json).into_bytes();
    let pkg_digest = sha256_bytes(&dig_bytes);

    // write registry files
    write_json(
        &base.join("package.json"),
        &{
            let mut obj = JMap::new();
            obj.insert("entrypoints".to_string(), m.get("entrypoints").cloned().unwrap_or(J::Object(JMap::new())));
            obj.insert("exports".to_string(), exports);
            obj.insert("name".to_string(), J::Str(name.to_string()));
            obj.insert("package_digest".to_string(), J::Str(pkg_digest.clone()));
            obj.insert("schema".to_string(), J::Str("fard.package_record.v0_1".to_string()));
            obj.insert("version".to_string(), J::Str(ver.to_string()));
            J::Object(obj)
        },
    )?;
    write_json(&base.join("digests.json"), &digests_json)?;

    // write out summary
    write_json(
        &out.join("publish.json"),
        &{
            let mut obj = JMap::new();
            obj.insert("copied_files".to_string(), J::Array(copied.into_iter().map(J::Str).collect()));
            obj.insert("name".to_string(), J::Str(name.to_string()));
            obj.insert("ok".to_string(), J::Bool(true));
            obj.insert("package_digest".to_string(), J::Str(pkg_digest.clone()));
            obj.insert("schema".to_string(), J::Str("fard.publish_out.v0_1".to_string()));
            obj.insert("version".to_string(), J::Str(ver.to_string()));
            J::Object(obj)
        },
    )?;

    Ok(())
}
