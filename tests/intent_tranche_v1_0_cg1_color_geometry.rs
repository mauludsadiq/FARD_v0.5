use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_bytes(rel: &str) -> Vec<u8> {
    let p = repo_root().join(rel);
    fs::read(&p).unwrap_or_else(|e| panic!("read failed: {}: {}", p.display(), e))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    hex::encode(d)
}

#[test]
fn cg1_color_geometry_hue_report_matches_golden_bytes() {
    let golden = read_bytes("tests/data/color_quant/K3_K5_generated.golden.md");

    let program_path = repo_root().join("tests/programs/cg1_hue_report.fard");

    let out_dir = repo_root().join("out").join("cg1_color_geometry");
    let _ = fs::create_dir_all(&out_dir);
    let out_path = out_dir.join("K3_K5_generated.md");

    let fardrun = env!("CARGO_BIN_EXE_fardrun");

    let run = Command::new(fardrun)
        .arg("run")
        .arg("--program")
        .arg(&program_path)
        .arg("--out")
        .arg(&out_dir)
        .output()
        .expect("failed to invoke fardrun");

    if !run.status.success() {
        let stderr = String::from_utf8_lossy(&run.stderr);

        // Hard requirement: CG1 must be parse-valid today.
        if stderr.contains("ERROR_PARSE") || stderr.contains("unexpected char") || stderr.contains("unexpected token") {
            panic!(
                "CG1 gate invalid: program must be parse-valid today (fail only due to missing std/color symbol)\nstatus={}\n\nstderr:\n{}",
                run.status, stderr
            );
        }

        // Until std/color exists, the only acceptable failure is missing symbol.
        if !stderr.contains("unbound var: std_color_hue_report_multi") {
            panic!(
                "CG1 gate failed for unexpected reason (expected missing std/color symbol)\nstatus={}\n\nstderr:\n{}",
                run.status, stderr
            );
        }

        panic!(
            "CG1 gate: std/color not implemented yet (expected)\nstatus={}\n\nstderr:\n{}",
            run.status, stderr
        );
    }

    let produced = fs::read(&out_path).unwrap_or_else(|e| {
        panic!(
            "CG1 gate: fardrun succeeded but did not create output file\nout_path={}\nerr={}",
            out_path.display(),
            e
        )
    });

    if produced != golden {
        let got_len = produced.len();
        let exp_len = golden.len();

        let got_sha = sha256_hex(&produced);
        let exp_sha = sha256_hex(&golden);

        let got_preview = String::from_utf8_lossy(&produced);
        let exp_preview = String::from_utf8_lossy(&golden);

        panic!(
            "CG1 gate: output bytes do not match golden\nexpected_len={} expected_sha256={}\n     got_len={}      got_sha256={}\n\n--- expected (first 600 chars) ---\n{}\n\n--- got (first 600 chars) ---\n{}\n",
            exp_len,
            exp_sha,
            got_len,
            got_sha,
            &exp_preview[..exp_preview.len().min(600)],
            &got_preview[..got_preview.len().min(600)],
        );
    }
}
