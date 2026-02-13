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
fn m5_mutated_trace_fails() {
    let out = "out/m5_mut_trace";
    let p = "spec/tmp/m5_mut_trace.fard";
    let st = run_runner(
        r#"
import("std/result") as result
result.ok(1)
"#,
        out,
        p,
    );
    assert!(st.success(), "RUNNER_NONZERO");

    let tp = format!("{}/trace.ndjson", out);
    let mut t = fs::read_to_string(&tp).expect("READ_TRACE_FAIL");
    t.push(' ');
    fs::write(&tp, t.as_bytes()).expect("WRITE_TRACE_FAIL");

    let vst = run_verify_bundle(out);
    assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
