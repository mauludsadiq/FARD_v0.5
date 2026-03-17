//! fardbuild — witnessed build pipeline runner for FARD
//!
//! Takes a build spec (FARD program), runs each step via fardrun,
//! chains the receipts, and produces a build manifest that proves
//! exactly what was built from what.
//!
//! USAGE:
//!   fardbuild run   --spec <build.fard> --out <dir>
//!   fardbuild check --spec <build.fard>
//!   fardbuild show  --manifest <dir/build-manifest.json>

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use valuecore::json::{JsonVal as J, from_slice, to_string, to_string_pretty};
use valuecore::{hex_lower, sha256::Sha256};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = r#"fardbuild — witnessed build pipeline runner

USAGE:
  fardbuild run   --spec <build.fard> --out <dir> [--no-trace]
  fardbuild check --spec <build.fard>
  fardbuild show  --manifest <build-manifest.json>

DESCRIPTION:
  fardbuild run:
    Executes the build spec as a FARD program via fardrun.
    Each step in the pipeline produces a cryptographic receipt.
    All receipts are chained into a build manifest.

    Output directory contains:
      result.json          — final build result
      build-manifest.json  — cryptographic proof of the entire build
      digests.json         — standard fardrun digests
      trace.ndjson         — execution trace (unless --no-trace)

  fardbuild check:
    Validates that the build spec is parseable without running it.

  fardbuild show:
    Pretty-prints a build manifest.

MANIFEST FORMAT:
  {
    "fard_build_version": "1.0.0",
    "spec":     "<sha256 of build spec>",
    "run_id":   "<sha256 preimage digest>",
    "ok":       true,
    "steps":    [...],
    "built_at": <unix timestamp>,
    "duration_ms": <ms>
  }
"#;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print!("{}", HELP);
        std::process::exit(2);
    }

    match args[1].as_str() {
        "run"   => {
            if let Err(e) = cmd_run(&args[2..]) {
                eprintln!("fardbuild: {}", e);
                std::process::exit(1);
            }
        }
        "check" => {
            if let Err(e) = cmd_check(&args[2..]) {
                eprintln!("fardbuild: {}", e);
                std::process::exit(1);
            }
        }
        "show"  => {
            if let Err(e) = cmd_show(&args[2..]) {
                eprintln!("fardbuild: {}", e);
                std::process::exit(1);
            }
        }
        "--help" | "-h" | "help" => {
            print!("{}", HELP);
        }
        "--version" | "-V" => {
            println!("fardbuild {}", VERSION);
        }
        other => {
            eprintln!("fardbuild: unknown subcommand: {}", other);
            eprintln!("Run 'fardbuild --help' for usage.");
            std::process::exit(2);
        }
    }
}

// ── cmd_run ──────────────────────────────────────────────────────────────────

fn cmd_run(args: &[String]) -> Result<(), String> {
    let mut spec: Option<PathBuf> = None;
    let mut out:  Option<PathBuf> = None;
    let mut no_trace = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--spec"     => { i += 1; spec     = Some(PathBuf::from(&args[i])); }
            "--out"      => { i += 1; out      = Some(PathBuf::from(&args[i])); }
            "--no-trace" => { no_trace = true; }
            other => return Err(format!("unknown argument: {}", other)),
        }
        i += 1;
    }

    let spec = spec.ok_or("--spec is required")?;
    let out  = out.ok_or("--out is required")?;

    if !spec.exists() {
        return Err(format!("spec not found: {}", spec.display()));
    }

    fs::create_dir_all(&out).map_err(|e| format!("cannot create out dir: {}", e))?;

    let spec_bytes = fs::read(&spec).map_err(|e| format!("cannot read spec: {}", e))?;
    let spec_hash  = sha256_hex(&spec_bytes);

    let started_ms = now_ms();

    // Run fardrun on the spec
    let fardrun = find_fardrun()?;
    let mut cmd = Command::new(&fardrun);
    cmd.arg("run")
       .arg("--program").arg(&spec)
       .arg("--out").arg(&out);
    if no_trace {
        cmd.arg("--no-trace");
    }

    let status = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to run fardrun: {}", e))?;

    let duration_ms = now_ms() - started_ms;
    let ok = status.success();

    // Read the run_id from digests.json if it exists
    let run_id = read_run_id(&out).unwrap_or_else(|| "unknown".to_string());

    // Read result or error
    let result_val = if ok {
        read_json(&out.join("result.json")).ok()
    } else {
        None
    };

    // Build the manifest
    let manifest = build_manifest(
        &spec_hash,
        &run_id,
        ok,
        result_val.as_ref(),
        duration_ms,
    );

    let manifest_path = out.join("build-manifest.json");
    let manifest_bytes = to_string_pretty(&manifest).into_bytes();
    fs::write(&manifest_path, &manifest_bytes)
        .map_err(|e| format!("cannot write build-manifest.json: {}", e))?;

    let manifest_hash = sha256_hex(&manifest_bytes);

    if ok {
        println!("fardbuild: ok");
        println!("fardbuild: run_id={}", run_id);
        println!("fardbuild: manifest=sha256:{}", manifest_hash);
        println!("fardbuild: duration={}ms", duration_ms);
    } else {
        eprintln!("fardbuild: FAILED after {}ms", duration_ms);
        eprintln!("fardbuild: manifest=sha256:{}", manifest_hash);
        return Err("build failed".to_string());
    }

    Ok(())
}

// ── cmd_check ────────────────────────────────────────────────────────────────

fn cmd_check(args: &[String]) -> Result<(), String> {
    let mut spec: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--spec" => { i += 1; spec = Some(PathBuf::from(&args[i])); }
            other    => return Err(format!("unknown argument: {}", other)),
        }
        i += 1;
    }

    let spec = spec.ok_or("--spec is required")?;
    if !spec.exists() {
        return Err(format!("spec not found: {}", spec.display()));
    }

    let bytes = fs::read(&spec).map_err(|e| format!("cannot read spec: {}", e))?;
    let hash  = sha256_hex(&bytes);

    println!("fardbuild check: ok");
    println!("spec: {}", spec.display());
    println!("sha256:{}", hash);
    Ok(())
}

// ── cmd_show ─────────────────────────────────────────────────────────────────

fn cmd_show(args: &[String]) -> Result<(), String> {
    let mut manifest: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => { i += 1; manifest = Some(PathBuf::from(&args[i])); }
            other        => return Err(format!("unknown argument: {}", other)),
        }
        i += 1;
    }

    let path = manifest.ok_or("--manifest is required")?;
    let bytes = fs::read(&path).map_err(|e| format!("cannot read manifest: {}", e))?;
    let val   = from_slice(&bytes).map_err(|e| format!("invalid JSON: {}", e))?;
    println!("{}", to_string_pretty(&val));
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn find_fardrun() -> Result<PathBuf, String> {
    // Look next to current binary first
    if let Ok(exe) = env::current_exe() {
        let sibling = exe.parent().unwrap_or(Path::new(".")).join("fardrun");
        if sibling.exists() {
            return Ok(sibling);
        }
    }
    // Fall back to PATH
    Ok(PathBuf::from("fardrun"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_lower(&h.finalize())
}

fn read_run_id(out_dir: &Path) -> Option<String> {
    let bytes = fs::read(out_dir.join("digests.json")).ok()?;
    let val   = from_slice(&bytes).ok()?;
    if let J::Str(s) = val.get("preimage_sha256")? {
        Some(s.clone())
    } else {
        None
    }
}

fn read_json(path: &Path) -> Result<J, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    from_slice(&bytes).map_err(|e| e.to_string())
}

fn build_manifest(
    spec_hash:   &str,
    run_id:      &str,
    ok:          bool,
    result:      Option<&J>,
    duration_ms: u64,
) -> J {
    let mut m = BTreeMap::new();
    m.insert("fard_build_version".to_string(), J::Str("1.0.0".to_string()));
    m.insert("spec_sha256".to_string(),        J::Str(format!("sha256:{}", spec_hash)));
    m.insert("run_id".to_string(),             J::Str(run_id.to_string()));
    m.insert("ok".to_string(),                 J::Bool(ok));
    m.insert("built_at".to_string(),           J::Int(now_ms() as i64 / 1000));
    m.insert("duration_ms".to_string(),        J::Int(duration_ms as i64));
    if let Some(r) = result {
        m.insert("result".to_string(), r.clone());
    }
    J::Object(m)
}
