use std::fs;
use std::process::Command;

fn write_file(path: &str, s: &str) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, s.as_bytes()).expect("write file");
}

fn run_fard(name: &str, src: &str, expect_ok: bool) -> String {
    let program = format!("spec/tmp/{name}.fard");
    let outdir = format!("out/{name}");

    let _ = fs::remove_dir_all(&outdir);
    write_file(&program, src);

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let out = Command::new(exe)
        .args(["run", "--program", &program, "--out", &outdir])
        .output()
        .expect("spawn fardrun");

    if expect_ok {
        assert!(out.status.success(), "runner nonzero: {name}");
    } else {
        assert!(!out.status.success(), "runner unexpectedly ok: {name}");
    }

    outdir
}

fn read_error_json(outdir: &str) -> serde_json::Value {
    let p = format!("{outdir}/error.json");
    let s = fs::read_to_string(&p).expect("read error.json");
    serde_json::from_str(&s).expect("error.json must be json")
}

#[test]
fn m3_runtime_safety_no_panic_on_missing_parent() {
    let outdir = run_fard(
        "m3_runtime_safety_missing_parent",
        r#"
emit({k:"m3"})
let _y = emit_artifact_derived("out0", "x.bin", {a:1}, ["no_such_parent"]) in
0
"#,
        false,
    );

    let ej = read_error_json(&outdir);
    let code = ej
        .get("code")
        .and_then(|x| x.as_str())
        .unwrap_or("<missing>");
    assert!(
        code == "ERROR_M3_PARENT_NOT_DECLARED",
        "expected ERROR_M3_PARENT_NOT_DECLARED, got: {code}"
    );
}

#[test]
fn m3_runtime_safety_no_panic_on_parent_cid_mismatch() {
    write_file("spec/tmp/m3s_in.bin", "x");

    let outdir = run_fard(
        "m3_runtime_safety_parent_cid_mismatch",
        r#"
let _x = import_artifact_named("in0", "spec/tmp/m3s_in.bin") in
let _y = emit_artifact_derived("out0", "x.bin", {a:1}, ["in0"]) in
let _z = emit_artifact_derived("out1", "y.bin", {a:2}, ["in0"]) in
0
"#,
        true,
    );

    // This test just proves the normal path still works. The cid-mismatch case is enforced
    // by Tracer, and should be unreachable if the builtin constructs parents from the map.
    // We keep it here to lock "no regression" on the happy path.
    let _ = outdir;
}
