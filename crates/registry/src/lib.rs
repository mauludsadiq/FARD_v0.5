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
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
    {
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

// ── Inherit-Cert CRDT integration ─────────────────────────────────────────────

use inherit_cert_crdt::{InheritCertState, InheritCertDelta, EffectKey, RunID};

fn crdt_state_path() -> Result<PathBuf> {
    let mut p = store_root()?;
    p.push("inherit_cert_state.json");
    Ok(p)
}

/// Load the current CRDT state from disk.
pub fn crdt_load() -> Result<InheritCertState> {
    let p = crdt_state_path()?;
    if !p.exists() {
        return Ok(InheritCertState::new());
    }
    let bytes = fs::read(&p).context("crdt_load read")?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).context("crdt_load parse")?;
    InheritCertState::from_json(&v).map_err(|e| anyhow!("crdt_load: {}", e))
}

/// Save the CRDT state to disk atomically.
pub fn crdt_save(state: &InheritCertState) -> Result<()> {
    let p = crdt_state_path()?;
    fs::create_dir_all(p.parent().unwrap())?;
    let json = serde_json::to_string_pretty(&state.to_json())?;
    // Atomic write via temp file
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, json.as_bytes())?;
    fs::rename(&tmp, &p)?;
    Ok(())
}

/// Propose a RunID for an effect — merges into persistent CRDT state.
pub fn crdt_propose(effect_key: EffectKey, run_id: RunID) -> Result<()> {
    let mut state = crdt_load()?;
    state.propose(effect_key, run_id);
    crdt_save(&state)
}

/// Merge a delta into the persistent CRDT state.
/// Returns the number of updates applied.
pub fn crdt_merge_delta(delta: &InheritCertDelta) -> Result<usize> {
    let mut state = crdt_load()?;
    let before = state.len();
    delta.apply_to(&mut state);
    let after = state.len();
    crdt_save(&state)?;
    Ok(after - before + delta.updates.values()
        .filter(|run_id| state.get(&delta.updates.keys()
            .find(|k| state.certs.get(*k).map(|r| &r.value) == Some(run_id))
            .unwrap_or(&EffectKey("".to_string()))).is_some())
        .count())
}

/// Merge another full state into the persistent CRDT state.
pub fn crdt_merge_state(other: &InheritCertState) -> Result<InheritCertState> {
    let mut state = crdt_load()?;
    state.merge_into(other);
    crdt_save(&state)?;
    Ok(state)
}

/// Get the canonical RunID for an effect, if any.
pub fn crdt_get(effect_key: &EffectKey) -> Result<Option<RunID>> {
    let state = crdt_load()?;
    Ok(state.get(effect_key).cloned())
}

/// Compute the delta between our state and another replica's state.
/// Returns what the other replica needs to converge.
pub fn crdt_delta_for(their_state: &InheritCertState) -> Result<InheritCertDelta> {
    let our_state = crdt_load()?;
    Ok(InheritCertDelta::compute(their_state, &our_state))
}
