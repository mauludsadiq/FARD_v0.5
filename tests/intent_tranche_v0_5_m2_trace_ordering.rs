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
fn m2_trace_ordering_gate_module_resolve_must_be_prefix() {
    let out = "out/m2_ordering_bad_prefix";
    let p = "spec/tmp/m2_ordering_bad_prefix.fard";
    let st = run_runner(
        r#"
import("std/result") as result
result.ok(1)
"#,
        out,
        p,
    );
    assert!(st.success(), "RUNNER_NONZERO");

    let trace_p = format!("{}/trace.ndjson", out);
    let mut bytes = fs::read(&trace_p).expect("READ_TRACE_FAIL");
    let late_module_resolve = b"{\"cid\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\",\"kind\":\"std\",\"name\":\"std/result\",\"t\":\"module_resolve\"}\n";
    bytes.extend_from_slice(late_module_resolve);
    fs::write(&trace_p, &bytes).expect("WRITE_TRACE_FAIL");

    let vst = run_verify(out);
    assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
