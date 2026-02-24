pub mod builtin_pipe_v1;
pub mod builtin_sig_table_v1;

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub runner: RunnerCfg,
    pub artifacts: ArtifactsCfg,
    #[serde(default)]
    pub gates: GatesCfg,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunnerCfg {
    /// Command to invoke. If this is a list, the first element is the executable.
    pub cmd: Vec<String>,
    /// Base args to pass before per-run args.
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArtifactsCfg {
    pub trace_relpath: String,
    pub result_relpath: String,
    pub lock_relpath: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GatesCfg {
    #[serde(default = "default_true")]
    pub require_trace_file: bool,
    #[serde(default = "default_true")]
    pub require_result_file: bool,
    #[serde(default = "default_true")]
    pub cg1_color_geometry_missing_std_color: bool,
}

fn default_true() -> bool {
    true
}

pub fn load_config(path: &Path) -> Result<Config> {
    let s = fs::read_to_string(path).with_context(|| format!("read config {path:?}"))?;
    let cfg: Config = toml::from_str(&s).with_context(|| format!("parse TOML {path:?}"))?;
    if cfg.runner.cmd.is_empty() {
        bail!("runner.cmd must be non-empty");
    }
    Ok(cfg)
}

#[derive(Debug, Clone)]
pub struct RunPaths {
    pub out_dir: PathBuf,
    pub trace_path: PathBuf,
    pub result_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub output: Output,
    pub paths: RunPaths,
}

fn sha256_hex_native(b: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(b.len() * 2);
    for &byte in b {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    sha256_hex_native(&h.finalize())
}

pub fn read_bytes(p: &Path) -> Result<Vec<u8>> {
    fs::read(p).with_context(|| format!("read bytes {p:?}"))
}

pub fn run_fard(
    cfg: &Config,
    program_path: &Path,
    out_dir: &Path,
    extra_args: &[String],
) -> Result<RunResult> {
    fs::create_dir_all(out_dir).with_context(|| format!("create out dir {out_dir:?}"))?;

    let exe = &cfg.runner.cmd[0];
    let mut cmd = Command::new(exe);
    if cfg.runner.cmd.len() > 1 {
        cmd.args(&cfg.runner.cmd[1..]);
    }
    cmd.args(&cfg.runner.args);
    cmd.args(extra_args);

    // Common flag pattern used in your repos; adjust in fard_gate.toml if different.
    cmd.arg("--program").arg(program_path);
    cmd.arg("--out").arg(out_dir);

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd
        .output()
        .with_context(|| format!("execute runner: {exe} (program={program_path:?})"))?;

    let trace_path = out_dir.join(&cfg.artifacts.trace_relpath);
    let result_path = out_dir.join(&cfg.artifacts.result_relpath);

    Ok(RunResult {
        output,
        paths: RunPaths {
            out_dir: out_dir.to_path_buf(),
            trace_path,
            result_path,
        },
    })
}

pub fn assert_success(r: &RunResult) -> Result<()> {
    if !r.output.status.success() {
        let out = String::from_utf8_lossy(&r.output.stdout);
        let err = String::from_utf8_lossy(&r.output.stderr);
        bail!(
            "runner failed (exit={}):\n--- stdout ---\n{}\n--- stderr ---\n{}\n",
            r.output.status,
            out,
            err
        );
    }
    Ok(())
}

pub fn assert_artifacts_exist(cfg: &Config, r: &RunResult) -> Result<()> {
    if cfg.gates.require_trace_file && !r.paths.trace_path.exists() {
        bail!("missing trace file: {:?}", r.paths.trace_path);
    }
    if cfg.gates.require_result_file && !r.paths.result_path.exists() {
        bail!("missing result file: {:?}", r.paths.result_path);
    }
    Ok(())
}

pub fn parse_ndjson_bytes(trace_bytes: &[u8]) -> Result<Vec<serde_json::Value>> {
    let s = std::str::from_utf8(trace_bytes).context("trace.ndjson is not UTF-8")?;
    let mut out = Vec::new();
    for (idx, line) in s.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("invalid JSON on trace line {}: {}", idx + 1, line))?;
        out.push(v);
    }
    Ok(out)
}

pub fn find_events<'a>(trace: &'a [serde_json::Value], t: &str) -> Vec<&'a serde_json::Value> {
    trace
        .iter()
        .filter(|v| v.get("t").and_then(|x| x.as_str()) == Some(t))
        .collect()
}

pub fn require_event(trace: &[serde_json::Value], t: &str) -> Result<()> {
    if find_events(trace, t).is_empty() {
        bail!("trace missing event type t={}", t);
    }
    Ok(())
}

pub fn extract_lock_mismatch(stderr: &str) -> Option<(String, String, String)> {
    // Expected pattern:
    // ERROR: LOCK_MISMATCH
    //   logical: std/list
    //   want: sha256:...
    //   got:  sha256:...
    let re = Regex::new(
        r"logical:\s*(?P<logical>\S+)\s*\n\s*want:\s*(?P<want>\S+)\s*\n\s*got:\s*(?P<got>\S+)",
    )
    .ok()?;
    let caps = re.captures(stderr)?;
    Some((
        caps.name("logical")?.as_str().to_string(),
        caps.name("want")?.as_str().to_string(),
        caps.name("got")?.as_str().to_string(),
    ))
}

pub fn as_os_strings(xs: &[String]) -> Vec<OsString> {
    xs.iter().map(OsString::from).collect()
}

pub fn write_if_missing(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

pub fn write_always(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

pub fn die(msg: impl AsRef<str>) -> anyhow::Error {
    anyhow!(msg.as_ref().to_string())
}

pub fn sha256_file_hex(path: &Path) -> Result<String> {
    let bytes = read_bytes(path)?;
    Ok(sha256_hex(&bytes))
}

pub fn read_json_value(path: &Path) -> Result<serde_json::Value> {
    let bytes = read_bytes(path)?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parse JSON {path:?}"))?;
    Ok(v)
}

pub fn extract_result_value(result_json: &serde_json::Value) -> serde_json::Value {
    match result_json {
        serde_json::Value::Object(map) => {
            if let Some(v) = map.get("result") {
                return v.clone();
            }
            if let Some(v) = map.get("value") {
                return v.clone();
            }
            if let Some(v) = map.get("v") {
                return v.clone();
            }
            result_json.clone()
        }
        _ => result_json.clone(),
    }
}

pub fn parse_ndjson_lines(path: &Path) -> Result<Vec<serde_json::Value>> {
    let s = fs::read_to_string(path).with_context(|| format!("read {path:?}"))?;
    let mut out = Vec::new();
    for (i, line) in s.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(l)
            .with_context(|| format!("ndjson parse error at line {} in {path:?}", i + 1))?;
        out.push(v);
    }
    Ok(out)
}

pub fn extract_result_from_trace(events: &[serde_json::Value]) -> Option<serde_json::Value> {
    for ev in events.iter().rev() {
        if let serde_json::Value::Object(map) = ev {
            if map.get("t").and_then(|v| v.as_str()) == Some("result") {
                if let Some(v) = map.get("v") {
                    return Some(v.clone());
                }
                if let Some(v) = map.get("value") {
                    return Some(v.clone());
                }
            }
            if map.get("event").and_then(|v| v.as_str()) == Some("result") {
                if let Some(v) = map.get("v") {
                    return Some(v.clone());
                }
                if let Some(v) = map.get("value") {
                    return Some(v.clone());
                }
            }
        }
    }
    None
}

pub fn matches_any_regex(text: &str, patterns: &[String]) -> Result<bool> {
    for p in patterns {
        let re = Regex::new(p).with_context(|| format!("bad regex: {p}"))?;
        if re.is_match(text) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub mod cli;
pub mod digest;

pub mod gates;
