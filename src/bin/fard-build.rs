//! fard-build — Verifiable build system binary
//!
//! Reads fard.build.toml, runs each step as a witnessed FARD run,
//! chains receipts cryptographically, produces build.receipt.json.
//!
//! Usage:
//!   fard-build [--config fard.build.toml] [--out build/] [--step <name>] [--verify]
//!
//! fard.build.toml format:
//!   [build]
//!   name = "my-project"
//!   version = "1.0.0"
//!
//!   [[step]]
//!   name = "compile"
//!   program = "steps/compile.fard"
//!   out = "build/compile/"
//!
//!   [[step]]
//!   name = "test"
//!   program = "steps/test.fard"
//!   out = "build/test/"
//!   depends_on = ["compile"]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

// ── Build config ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct BuildStep {
    name: String,
    program: PathBuf,
    out: PathBuf,
    depends_on: Vec<String>,
    env: BTreeMap<String, String>,
    args: Vec<String>,
    no_trace: bool,
}

#[derive(Debug)]
struct BuildConfig {
    name: String,
    version: String,
    steps: Vec<BuildStep>,
}

fn parse_build_toml(src: &str, base: &Path) -> Result<BuildConfig> {
    let mut name = String::from("unnamed");
    let mut version = String::from("0.0.0");
    let mut steps: Vec<BuildStep> = Vec::new();
    let mut current_step: Option<BuildStep> = None;
    let mut in_build = false;
    let mut in_step = false;

    for raw_line in src.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        if line == "[build]" {
            in_build = true; in_step = false;
            if let Some(s) = current_step.take() { steps.push(s); }
            continue;
        }
        if line == "[[step]]" {
            if let Some(s) = current_step.take() { steps.push(s); }
            in_step = true; in_build = false;
            current_step = Some(BuildStep {
                name: String::new(),
                program: PathBuf::new(),
                out: PathBuf::new(),
                depends_on: Vec::new(),
                env: BTreeMap::new(),
                args: Vec::new(),
                no_trace: false,
            });
            continue;
        }
        if line.starts_with('[') { in_build = false; in_step = false; continue; }

        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"').to_string();

            if in_build {
                match k {
                    "name" => name = v,
                    "version" => version = v,
                    _ => {}
                }
            } else if in_step {
                if let Some(ref mut s) = current_step {
                    match k {
                        "name" => s.name = v,
                        "program" => s.program = base.join(&v),
                        "out" => s.out = if v.starts_with('/') { PathBuf::from(&v) } else { base.join(&v) },
                        "no_trace" => s.no_trace = v == "true",
                        "depends_on" => {
                            // depends_on = ["a", "b"]
                            s.depends_on = v.trim_matches(|c| c == '[' || c == ']')
                                .split(',')
                                .map(|x| x.trim().trim_matches('"').to_string())
                                .filter(|x| !x.is_empty())
                                .collect();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    if let Some(s) = current_step { steps.push(s); }

    // Validate
    for (i, s) in steps.iter().enumerate() {
        if s.name.is_empty() { bail!("step {} has no name", i); }
        if s.program == PathBuf::new() { bail!("step {:?} has no program", s.name); }
        if s.out == PathBuf::new() { bail!("step {:?} has no out dir", s.name); }
    }

    Ok(BuildConfig { name, version, steps })
}

// ── Step execution ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct StepResult {
    name: String,
    ok: bool,
    run_digest: Option<String>,
    out_dir: PathBuf,
    duration_ms: u128,
    error: Option<String>,
}

fn run_step(step: &BuildStep, fardrun: &Path, prior_digest: Option<&str>) -> Result<StepResult> {
    std::fs::create_dir_all(&step.out)?;

    let start = std::time::Instant::now();

    let mut cmd = Command::new(fardrun);
    cmd.arg("run")
        .arg("--program").arg(&step.program)
        .arg("--out").arg(&step.out);

    if step.no_trace {
        cmd.arg("--no-trace");
    }

    // Pass prior digest as env var for chaining
    if let Some(digest) = prior_digest {
        cmd.env("FARD_PRIOR_DIGEST", digest);
    }

    // Pass step name
    cmd.env("FARD_BUILD_STEP", &step.name);

    for (k, v) in &step.env {
        cmd.env(k, v);
    }

    let output = cmd.output()
        .with_context(|| format!("failed to run fardrun for step {:?}", step.name))?;

    let duration_ms = start.elapsed().as_millis();
    let ok = output.status.success();

    // Extract run digest from stdout
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let run_digest = stderr.lines()
        .chain(stdout.lines())
        .find_map(|l| {
            l.strip_prefix("fard_run_digest=").map(|d| d.trim().to_string())
        });

    let error = if ok { None } else {
        Some(stderr.lines().last().unwrap_or("unknown error").to_string())
    };

    if ok {
        eprintln!("  [{}] ✓  {}  ({}ms)", step.name,
            run_digest.as_deref().unwrap_or("no-digest"), duration_ms);
    } else {
        eprintln!("  [{}] ✗  FAILED  ({}ms)", step.name, duration_ms);
        if let Some(ref e) = error {
            eprintln!("       {}", e);
        }
    }

    Ok(StepResult {
        name: step.name.clone(),
        ok,
        run_digest,
        out_dir: step.out.clone(),
        duration_ms,
        error,
    })
}

// ── Receipt chaining ──────────────────────────────────────────────────────────

fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(data);
    format!("sha256:{}", hex::encode(h.finalize()))
}

fn build_receipt(config: &BuildConfig, results: &[StepResult], total_ms: u128) -> Value {
    let steps: Vec<Value> = results.iter().map(|r| {
        json!({
            "name": r.name,
            "ok": r.ok,
            "run_digest": r.run_digest,
            "duration_ms": r.duration_ms,
            "error": r.error,
        })
    }).collect();

    // Chain digest: sha256 of all step digests in order
    let chain_input = results.iter()
        .filter_map(|r| r.run_digest.as_deref())
        .collect::<Vec<_>>()
        .join(":");
    let chain_digest = sha256_hex(chain_input.as_bytes());

    let ok = results.iter().all(|r| r.ok);

    json!({
        "kind": "fard/build_receipt/v0.1",
        "name": config.name,
        "version": config.version,
        "ok": ok,
        "chain_digest": chain_digest,
        "total_duration_ms": total_ms,
        "steps": steps,
        "step_count": results.len(),
        "passed": results.iter().filter(|r| r.ok).count(),
        "failed": results.iter().filter(|r| !r.ok).count(),
    })
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let config_path = args.windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or_else(|| PathBuf::from("fard.build.toml"));

    let out_dir = args.windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or_else(|| PathBuf::from("build"));

    let only_step = args.windows(2)
        .find(|w| w[0] == "--step")
        .map(|w| w[1].clone());

    let verify_only = args.iter().any(|a| a == "--verify");

    // Find fardrun binary
    let fardrun = args.windows(2)
        .find(|w| w[0] == "--fardrun")
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or_else(|| {
            // Look next to fard-build binary
            std::env::current_exe().ok()
                .and_then(|p| p.parent().map(|d| d.join("fardrun")))
                .unwrap_or_else(|| PathBuf::from("fardrun"))
        });

    // Verify mode
    if verify_only {
        let receipt_path = out_dir.join("build.receipt.json");
        let receipt_bytes = std::fs::read(&receipt_path)
            .with_context(|| format!("cannot read {}", receipt_path.display()))?;
        let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
        let ok = receipt.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let chain = receipt.get("chain_digest").and_then(|v| v.as_str()).unwrap_or("");
        if ok {
            println!("build receipt ok");
            println!("chain_digest: {}", chain);
        } else {
            eprintln!("build receipt FAILED");
            std::process::exit(1);
        }
        return Ok(());
    }

    // Load config
    let config_base = config_path.parent().unwrap_or(Path::new("."));
    let config_src = std::fs::read_to_string(&config_path)
        .with_context(|| format!("cannot read {}", config_path.display()))?;
    let config = parse_build_toml(&config_src, config_base)?;

    std::fs::create_dir_all(&out_dir)?;

    eprintln!("fard-build: {} v{}", config.name, config.version);
    eprintln!("  {} step(s)", config.steps.len());
    eprintln!();

    let total_start = std::time::Instant::now();
    let mut results: Vec<StepResult> = Vec::new();
    let mut last_digest: Option<String> = None;

    // Build dependency order map
    let steps_to_run: Vec<&BuildStep> = if let Some(ref only) = only_step {
        config.steps.iter().filter(|s| &s.name == only).collect()
    } else {
        config.steps.iter().collect()
    };

    for step in &steps_to_run {
        // Resolve prior digest from depends_on
        let prior = if step.depends_on.is_empty() {
            last_digest.as_deref()
        } else {
            results.iter()
                .filter(|r| step.depends_on.contains(&r.name))
                .last()
                .and_then(|r| r.run_digest.as_deref())
        };

        let result = run_step(step, &fardrun, prior)?;
        let ok = result.ok;
        last_digest = result.run_digest.clone();
        results.push(result);

        if !ok {
            eprintln!("\nbuild FAILED at step {:?}", step.name);
            break;
        }
    }

    let total_ms = total_start.elapsed().as_millis();
    let all_ok = results.iter().all(|r| r.ok);

    // Write receipt
    let receipt = build_receipt(&config, &results, total_ms);
    let receipt_path = out_dir.join("build.receipt.json");
    std::fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?)?;

    eprintln!();
    if all_ok {
        eprintln!("build ok — {} step(s) in {}ms", results.len(), total_ms);
    } else {
        eprintln!("build FAILED — {} passed, {} failed",
            results.iter().filter(|r| r.ok).count(),
            results.iter().filter(|r| !r.ok).count());
    }
    eprintln!("receipt: {}", receipt_path.display());
    eprintln!("chain:   {}", receipt.get("chain_digest")
        .and_then(|v| v.as_str()).unwrap_or(""));

    if !all_ok { std::process::exit(1); }
    Ok(())
}
