use std::fs;
use std::process::Command;

fn run(program_src: &str, outdir: &str, program_path: &str) -> std::process::ExitStatus {
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

fn count_substr(hay: &str, needle: &str) -> usize {
    hay.match_indices(needle).count()
}

#[test]
fn m5_module_graph_event_exactly_once_ok() {
    let outdir = "out/m5_mg_once_ok";
    let program = "spec/tmp/m5_mg_once_ok.fard";
    let st = run(
        r#"
import("std/result") as result
result.ok(1)
"#,
        outdir,
        program,
    );
    assert!(st.success(), "RUNNER_NONZERO");

    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");

    let needle = r#""t":"module_graph""#;
    let n = count_substr(&trace, needle);
    assert_eq!(n, 1, "MODULE_GRAPH_EVENT_COUNT_NOT_1");
}

#[test]
fn m5_module_graph_event_exactly_once_err() {
    let outdir = "out/m5_mg_once_err";
    let program = "spec/tmp/m5_mg_once_err.fard";
    let st = run(
        r#"
import("std/result") as result
let _ = result.err({code:"E", msg:"x"})?
result.ok(0)
"#,
        outdir,
        program,
    );
    assert!(!st.success(), "EXPECTED_NONZERO");

    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");

    let needle = r#""t":"module_graph""#;
    let n = count_substr(&trace, needle);
    assert_eq!(n, 1, "MODULE_GRAPH_EVENT_COUNT_NOT_1");
}
