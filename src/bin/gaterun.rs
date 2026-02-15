use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use fard_v0_5_language_gate::{read_bytes, run_fard, sha256_hex};

#[derive(Debug, Clone, Deserialize)]
struct Gate {
    id: String,
    name: String,
    kind: String,
    program: String,

    #[serde(rename = "expect_result")]
    expected_result: Option<Value>,

    #[serde(rename = "expect_stderr_regex")]
    stderr_regexes: Option<Vec<String>>,

    #[serde(rename = "expect_exit_nonzero")]
    expect_exit_nonzero: Option<bool>,

    #[serde(rename = "lockfile")]
    lockfile_relpath: Option<String>,
}

fn main() -> Result<()> {
    let gates_path = PathBuf::from("tests/gate/gates.json");
    let gates_bytes = fs::read(&gates_path).with_context(|| format!("read {gates_path:?}"))?;

    let v: serde_json::Value =
        serde_json::from_slice(&gates_bytes).with_context(|| "parse tests/gate/gates.json")?;

    let gates: Vec<Gate> = match v {
        serde_json::Value::Object(mut obj) => {
            if let Some(sv) = obj.remove("schema") {
                match sv {
                    serde_json::Value::String(s) => {
                        if s != "fard_language_gate_v0_5" {
                            anyhow::bail!("unexpected gates schema: {}", s);
                        }
                    }
                    _ => anyhow::bail!("gates schema must be a string"),
                }
            }
            if let Some(gv) = obj.remove("gates") {
                serde_json::from_value(gv).context("decode gates.gates")?
            } else {
                anyhow::bail!("gates.json object missing \"gates\" array");
            }
        }
        serde_json::Value::Array(_) => {
            serde_json::from_value(v).context("decode gates as array")?
        }
        other => anyhow::bail!("gates.json must be an object or array, got {:?}", other),
    };

    let cfg = fard_v0_5_language_gate::load_config(std::path::Path::new("fard_gate.toml"))?;

    let mut passed = 0usize;
    let mut failed = 0usize;

    for gate in &gates {
        println!();
        println!("=== {}_{} ===", gate.id, gate.name);

        let ok = run_one_gate(&cfg, gate).unwrap_or_else(|e| {
            println!("FAIL: {e:#}");
            false
        });

        if ok {
            passed += 1;
        } else {
            failed += 1;
        }
    }

    println!();
    println!("GATE_SUMMARY passed={} failed={}", passed, failed);

    if failed > 0 {
        return Err(anyhow!("LANGUAGE_GATE_FAILED"));
    }
    Ok(())
}

fn run_one_gate(cfg: &fard_v0_5_language_gate::Config, gate: &Gate) -> Result<bool> {
    let program_path = PathBuf::from(&gate.program);
    let out_root = PathBuf::from("out/gates").join(format!("{}_{}", gate.id, gate.name));

    if out_root.exists() {
        fs::remove_dir_all(&out_root).with_context(|| format!("rm -rf {out_root:?}"))?;
    }
    fs::create_dir_all(&out_root).with_context(|| format!("mkdir -p {out_root:?}"))?;

    let kind = gate.kind.as_str();

    match kind {
        "run_expect_result" => {
            let out = run_once(cfg, &program_path, &out_root, gate)?;
            if out.status_code != 0 {
                print_run_failure(&out);
                return Ok(false);
            }
            let got = read_result(cfg, &out_root)?;
            let exp = gate
                .expected_result
                .clone()
                .ok_or_else(|| anyhow!("gate missing expected_result"))?;
            if got == exp {
                println!("PASS");
                Ok(true)
            } else {
                println!("FAIL: result mismatch");
                println!("expected: {}", exp);
                println!("got:      {}", got);
                Ok(false)
            }
        }

        "run_expect_trace_parseable" | "trace_parseable" => {
            let out = run_once(cfg, &program_path, &out_root, gate)?;
            if out.status_code != 0 {
                print_run_failure(&out);
                return Ok(false);
            }
            if trace_parseable(cfg, &out_root)? {
                println!("PASS");
                Ok(true)
            } else {
                println!("FAIL: trace not parseable");
                Ok(false)
            }
        }

        "run_twice_expect_same_trace_sha256" | "determinism_trace" => {
            let a_dir = out_root.join("run_a");
            let b_dir = out_root.join("run_b");
            fs::create_dir_all(&a_dir)?;
            fs::create_dir_all(&b_dir)?;

            let out_a = run_once(cfg, &program_path, &a_dir, gate)?;
            if out_a.status_code != 0 {
                print_run_failure(&out_a);
                return Ok(false);
            }
            let out_b = run_once(cfg, &program_path, &b_dir, gate)?;
            if out_b.status_code != 0 {
                print_run_failure(&out_b);
                return Ok(false);
            }

            let a_trace = read_trace_bytes(cfg, &a_dir)?;
            let b_trace = read_trace_bytes(cfg, &b_dir)?;

            let ha = sha256_hex(&a_trace);
            let hb = sha256_hex(&b_trace);

            if ha == hb {
                println!("PASS trace_sha256={}", ha);
                Ok(true)
            } else {
                println!("FAIL: trace bytes differ");
                println!("A sha256={}", ha);
                println!("B sha256={}", hb);
                Ok(false)
            }
        }

        "run_expect_error" | "run_expect_error_with_lock" => {
            let out = run_once(cfg, &program_path, &out_root, gate)?;
            let want_nonzero = gate.expect_exit_nonzero.unwrap_or(true);
            let code_ok = if want_nonzero {
                out.status_code != 0
            } else {
                out.status_code == 0
            };

            if !code_ok {
                if want_nonzero {
                    println!("FAIL: expected nonzero exit");
                } else {
                    println!("FAIL: expected zero exit");
                }
                Ok(false)
            } else {
                let ok = stderr_matches(gate, &out.stderr)?;
                if ok {
                    println!("PASS");
                    Ok(true)
                } else {
                    println!("FAIL: stderr did not match expected regexes");
                    print_run_failure(&out);
                    Ok(false)
                }
            }
        }

        "run_expect_failure_regex" => {
            let out = run_once(cfg, &program_path, &out_root, gate)?;

            let want_nonzero = gate.expect_exit_nonzero.unwrap_or(true);
            let code_ok = if want_nonzero {
                out.status_code != 0
            } else {
                out.status_code == 0
            };

            if !code_ok {
                if want_nonzero {
                    println!("FAIL: expected nonzero exit");
                } else {
                    println!("FAIL: expected zero exit");
                }
                print_run_failure(&out);
                return Ok(false);
            }

            if !stderr_matches(gate, &out.stderr)? {
                println!("FAIL: stderr did not match expected regexes");
                print_run_failure(&out);
                return Ok(false);
            }

            println!("PASS");
            Ok(true)
        }
        
        "cg1_color_geometry" => {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            match fard_v0_5_language_gate::gates::cg1_color_geometry::run(cfg, root) {
                Ok(()) => {
                    if cfg.gates.cg1_color_geometry_missing_std_color {
                        // When the flag is true, CG1 is expected to FAIL (missing std/color).
                        println!("XPASS: CG1 unexpectedly passed (std_color may have landed or behavior changed)");
                        Ok(false)
                    } else {
                        // When the flag is false, CG1 is expected to PASS.
                        println!("PASS");
                        Ok(true)
                    }
                }
                Err(e) => {
                    if cfg.gates.cg1_color_geometry_missing_std_color {
                        // When the flag is true, failure is expected.
                        println!("XFAIL: {}", e);
                        Ok(true)
                    } else {
                        // When the flag is false, failure is an actual failure.
                        println!("FAIL: {}", e);
                        Ok(false)
                    }
                }
            }
        }

        other => {
            println!("FAIL: unknown gate kind: {other}");
            Ok(false)
        }
    }
}

struct RunOut {
    status_code: i32,
    stdout: String,
    stderr: String,
}

fn run_once(
    cfg: &fard_v0_5_language_gate::Config,
    program_path: &Path,
    out_dir: &Path,
    gate: &Gate,
) -> Result<RunOut> {
    let mut extra: Vec<String> = Vec::new();

    if let Some(lock_rel) = gate.lockfile_relpath.as_deref() {
        let lock_path = PathBuf::from(lock_rel);
        extra.push("--lock".to_string());
        extra.push(lock_path.to_string_lossy().to_string());
    }

    let out = run_fard(cfg, program_path, out_dir, extra.as_slice())?;

    Ok(RunOut {
        status_code: out.output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.output.stderr).to_string(),
    })
}

fn stderr_matches(gate: &Gate, stderr: &str) -> Result<bool> {
    let pats = match gate.stderr_regexes.as_deref() {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(true),
    };

    for p in pats {
        let re = Regex::new(p).with_context(|| format!("bad regex: {p}"))?;
        if !re.is_match(stderr) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn read_trace_bytes(cfg: &fard_v0_5_language_gate::Config, out_dir: &Path) -> Result<Vec<u8>> {
    let trace_path = out_dir.join(&cfg.artifacts.trace_relpath);
    read_bytes(&trace_path).with_context(|| format!("read trace {trace_path:?}"))
}

fn trace_parseable(cfg: &fard_v0_5_language_gate::Config, out_dir: &Path) -> Result<bool> {
    let bytes = read_trace_bytes(cfg, out_dir)?;
    let s = String::from_utf8(bytes).with_context(|| "trace not utf8")?;
    for (i, line) in s.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        serde_json::from_str::<Value>(line)
            .with_context(|| format!("trace line {} not json", i + 1))?;
    }
    Ok(true)
}

fn read_result(cfg: &fard_v0_5_language_gate::Config, out_dir: &Path) -> Result<Value> {
    let result_path = out_dir.join(&cfg.artifacts.result_relpath);
    if result_path.exists() {
        let bytes = read_bytes(&result_path)?;
        let v: Value = serde_json::from_slice(&bytes).with_context(|| "parse result.json")?;
        return Ok(extract_result_value(&v));
    }

    let trace_path = out_dir.join(&cfg.artifacts.trace_relpath);
    let bytes = read_bytes(&trace_path)?;
    let lines = String::from_utf8(bytes).with_context(|| "trace not utf8")?;
    for line in lines.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).with_context(|| "parse trace line")?;
        if let Some(r) = extract_result_from_trace_event(&v) {
            return Ok(r);
        }
    }

    Err(anyhow!("no result.json and no result event in trace"))
}

fn extract_result_value(v: &Value) -> Value {
    if let Some(r) = v.get("result") {
        r.clone()
    } else {
        v.clone()
    }
}

fn extract_result_from_trace_event(ev: &Value) -> Option<Value> {
    let t = ev.get("type")?.as_str()?;
    if t != "result" {
        return None;
    }
    ev.get("value").cloned()
}

fn print_run_failure(out: &RunOut) {
    println!("FAIL: exit code {}", out.status_code);
    if !out.stdout.trim().is_empty() {
        println!("stdout:");
        println!("{}", out.stdout);
    }
    if !out.stderr.trim().is_empty() {
        println!("stderr:");
        println!("{}", out.stderr);
    }
}
