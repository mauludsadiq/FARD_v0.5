use std::fs;
use std::process::Command;

fn run_runner(program_src: &str, outdir: &str, program_path: &str) -> std::process::ExitStatus {
    let _ = fs::remove_dir_all(outdir);
    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(program_path, program_src.as_bytes()).expect("WRITE_SRC_FAIL");
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardrun",
            "--",
            "run",
            "--program",
            program_path,
            "--out",
            outdir,
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL")
}

fn run_verify_bundle(outdir: &str) -> std::process::ExitStatus {
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardverify",
            "--",
            "bundle",
            "--out",
            outdir,
        ])
        .status()
        .expect("VERIFY_SPAWN_FAIL")
}

#[test]
fn m5_ok_bundle_verifies() {
    let out = "out/m5_ok_bundle";
    let p = "spec/tmp/m5_ok_bundle.fard";
    let st = run_runner(
        r#"
import("std/result") as result
result.ok(7)
"#,
        out,
        p,
    );
    assert!(st.success(), "RUNNER_NONZERO");

    let vst = run_verify_bundle(out);
    assert!(vst.success(), "EXPECTED_VERIFY_OK");
}
