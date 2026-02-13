use std::fs;
use std::process::Command;

fn run(program: &str, outdir: &str) {
    let _ = fs::remove_dir_all(outdir);

    let st = Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardrun",
            "--",
            "run",
            "--program",
            program,
            "--out",
            outdir,
        ])
        .status()
        .expect("SPAWN_FARDRUN");

    assert!(st.success(), "FARDRUN_FAILED");
}

fn verify_bundle(outdir: &str) {
    let st = Command::new("cargo")
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
        .expect("SPAWN_FARDVERIFY_BUNDLE");

    assert!(st.success(), "BUNDLE_VERIFY_FAILED");
}

fn assert_bytes(path: &str, golden_path: &str) {
    let a = fs::read(path).expect("READ_ACTUAL");
    let b = fs::read(golden_path).expect("READ_GOLDEN");
    assert_eq!(a, b, "BYTES_MISMATCH {}", path);
}

#[test]
fn m6_golden_program_01_bytes_match() {
    let program = "spec/m6/programs/golden_01.fard";
    let outdir = "out/m6_golden_01";

    run(program, outdir);
    verify_bundle(outdir);

    assert_bytes(
        &format!("{}/result.json", outdir),
        "spec/m6/golden/golden_01/result.json",
    );
    assert_bytes(
        &format!("{}/trace.ndjson", outdir),
        "spec/m6/golden/golden_01/trace.ndjson",
    );
    assert_bytes(
        &format!("{}/module_graph.json", outdir),
        "spec/m6/golden/golden_01/module_graph.json",
    );
    assert_bytes(
        &format!("{}/digests.json", outdir),
        "spec/m6/golden/golden_01/digests.json",
    );
}
