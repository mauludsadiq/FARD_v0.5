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

fn run_verify(outdir: &str) -> std::process::ExitStatus {
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardverify",
            "--",
            "trace",
            "--out",
            outdir,
        ])
        .status()
        .expect("VERIFY_SPAWN_FAIL")
}

#[test]
fn m2_trace_boundary_ok_false_requires_error_event_and_no_result() {
    let out = "out/m2_boundary_parse_fail";
    let p = "spec/tmp/m2_boundary_parse_fail.fard";
    let st = run_runner(
        r#"
import("std/result") as result
match 1 {
  1 => result.ok(1),

"#,
        out,
        p,
    );
    assert!(!st.success(), "EXPECTED_RUNNER_NONZERO");

    let vst = run_verify(out);
    assert!(vst.success(), "VERIFY_NONZERO_ON_REAL_FAILURE_OUTDIR");
}

#[test]
fn m2_trace_boundary_rejects_ok_false_with_result_json_present() {
    let out = "out/m2_boundary_tamper";
    let p = "spec/tmp/m2_boundary_tamper.fard";
    let st = run_runner(
        r#"
import("std/result") as result
match 1 {
  1 => result.ok(1),

"#,
        out,
        p,
    );
    assert!(!st.success(), "EXPECTED_RUNNER_NONZERO");

    fs::write(format!("{}/result.json", out), b"{\"result\":0}\n")
        .expect("TAMPER_WRITE_RESULT_FAIL");

    let vst = run_verify(out);
    assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
