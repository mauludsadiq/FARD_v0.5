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

fn must_obj(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("TYPE_FAIL obj")
}

#[test]
fn m1_ok_shape_exact_keys() {
    let outdir = "out/m1_ok_shape_exact";
    let program = "spec/tmp/m1_ok_shape_exact.fard";
    let st = run(
        r#"
import("std/result") as result
result.ok(1)
"#,
        outdir,
        program,
    );
    assert!(st.success(), "RUNNER_NONZERO");

    let b = fs::read(format!("{}/result.json", outdir)).expect("READ_RESULT_FAIL");
    let v: serde_json::Value = serde_json::from_slice(&b).expect("RESULT_JSON_PARSE_FAIL");
    let root = must_obj(&v);
    let r = root.get("result").expect("MISSING result");
    let ro = must_obj(r);

    assert_eq!(ro.get("t").and_then(|x| x.as_str()), Some("ok"), "BAD_T");
    assert!(ro.contains_key("v"), "MISSING v");
    assert_eq!(ro.len(), 2, "EXTRA_KEYS_IN_OK");
}
