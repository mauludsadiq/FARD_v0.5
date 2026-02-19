use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json;

use crate::Config;

fn read_rel(root: &Path, rel: &str) -> Vec<u8> {
    let p = root.join(rel);
    fs::read(&p).unwrap_or_else(|e| panic!("read failed: {}: {}", p.display(), e))
}

pub fn run(cfg: &Config, root: &Path) -> Result<(), String> {
    let golden = read_rel(root, "tests/data/color_quant/K3_K5_generated.golden.md");
    let program_path = root.join("tests/programs/cg1_hue_report.fard");

    let out_dir = root.join("out").join("cg1_color_geometry");
    let _ = fs::create_dir_all(&out_dir);

    let run_dir = out_dir.join("run");
    let _ = fs::create_dir_all(&run_dir);

    let out_path = out_dir.join("K3_K5_generated.md");

    if cfg.runner.cmd.is_empty() {
        return Err("CG1: cfg.runner.cmd is empty".to_string());
    }

    let mut cmd = Command::new(&cfg.runner.cmd[0]);
    if cfg.runner.cmd.len() > 1 {
        cmd.args(&cfg.runner.cmd[1..]);
    }
    if !cfg.runner.args.is_empty() {
        cmd.args(&cfg.runner.args);
    }

    cmd.current_dir(root);
    cmd.arg("run")
        .arg("--program")
        .arg(&program_path)
        .arg("--out")
        .arg(&run_dir);

    let out = cmd
        .output()
        .map_err(|e| format!("CG1 invoke failed: {}", e))?;

    let status = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr);

    if status != 0 {
        if stderr.contains("ERROR_PARSE")
            || stderr.contains("unexpected char")
            || stderr.contains("unexpected token")
        {
            return Err(format!(
                "CG1 invalid: program must be parse-valid today\nstatus={}\n\nstderr:\n{}",
                status, stderr
            ));
        }

        return Err(format!(
            "CG1 failing (expected until std/color exists)\nstatus={}\n\nstderr:\n{}",
            status, stderr
        ));
    }

    let result_json_path = run_dir.join("result.json");
    let result_bytes = fs::read(&result_json_path).map_err(|e| {
        format!(
            "CG1: missing result.json: {}: {}",
            result_json_path.display(),
            e
        )
    })?;

    let v: serde_json::Value = serde_json::from_slice(&result_bytes)
        .map_err(|e| format!("CG1: invalid result.json: {}", e))?;

    let md = v
        .get("result")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "CG1: result not a string".to_string())?;

    fs::write(&out_path, md.as_bytes()).map_err(|e| {
        format!(
            "CG1: failed writing output file: {}: {}",
            out_path.display(),
            e
        )
    })?;

    let produced = fs::read(&out_path)
        .map_err(|e| format!("CG1: output not created: {}: {}", out_path.display(), e))?;

    if produced != golden {
        let got_len = produced.len();
        let exp_len = golden.len();
        let got_sha = crate::sha256_hex(&produced);
        let exp_sha = crate::sha256_hex(&golden);

        let got_preview = String::from_utf8_lossy(&produced);
        let exp_preview = String::from_utf8_lossy(&golden);

        return Err(format!(
            "CG1 mismatch\nexpected_len={} expected_sha256={}\n     got_len={}      got_sha256={}\n\n--- expected (first 600 chars) ---\n{}\n\n--- got (first 600 chars) ---\n{}\n",
            exp_len,
            exp_sha,
            got_len,
            got_sha,
            &exp_preview[..exp_preview.len().min(600)],
            &got_preview[..got_preview.len().min(600)],
        ));
    }

    Ok(())
}
