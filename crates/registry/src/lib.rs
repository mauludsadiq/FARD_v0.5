use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::PathBuf;

fn parse_runid(runid: &str) -> Result<(&str, &str)> {
    // Expect: "sha256:<64hex>"
    let (alg, hex) = runid
        .split_once(':')
        .ok_or_else(|| anyhow!("ERROR_BADARG runid missing ':'"))?;
    if alg != "sha256" {
        bail!("ERROR_BADARG unsupported alg {}", alg);
    }
    if hex.len() != 64 || !hex.as_bytes().iter().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
        bail!("ERROR_BADARG runid must be 64 lowercase hex");
    }
    Ok((alg, hex))
}

fn store_root() -> Result<PathBuf> {
    // Minimal local registry store:
    // <repo_root>/_registry/sha256/<hex>.bin
    let root = std::env::var("FARD_REGISTRY_DIR").unwrap_or_else(|_| "_registry".to_string());
    Ok(PathBuf::from(root))
}

fn path_for(runid: &str) -> Result<PathBuf> {
    let (alg, hex) = parse_runid(runid)?;
    let mut p = store_root()?;
    p.push(alg);
    p.push(format!("{}.bin", hex));
    Ok(p)
}

pub fn put_bytes(runid: &str, bytes: &[u8]) -> Result<()> {
    let p = path_for(runid)?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).with_context(|| format!("ERROR_IO mkdir {}", dir.display()))?;
    }
    // Atomic-ish: write temp then rename
    let tmp = p.with_extension("bin.tmp");
    fs::write(&tmp, bytes).with_context(|| format!("ERROR_IO write {}", tmp.display()))?;
    fs::rename(&tmp, &p).with_context(|| format!("ERROR_IO rename {}", p.display()))?;
    Ok(())
}

pub fn get_bytes(runid: &str) -> Result<Vec<u8>> {
    let p = path_for(runid)?;
    if !p.exists() {
        bail!("ERROR_MISSING_FACT missing registry bytes {}", runid);
    }
    let b = fs::read(&p).with_context(|| format!("ERROR_IO read {}", p.display()))?;
    Ok(b)
}

// Optional: expose where things are stored (useful for debugging)
pub fn get_path(runid: &str) -> Result<PathBuf> {
    path_for(runid)
}
