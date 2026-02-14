use std::fs;
use std::process::Command;

#[test]
fn m6_debug_result_json_shape() {
    let _ = fs::create_dir_all("out");
    let prog = "out/_m6_debug_shape.fard";
    fs::write(prog, b"1+2*3\n").expect("WRITE_PROG");

    let outdir = "out/_m6_debug_shape_out";
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
            prog,
            "--out",
            outdir,
        ])
        .status()
        .expect("SPAWN_FARDRUN");
    assert!(st.success(), "FARDRUN_EXPECTED_OK");

    let bytes = fs::read(format!("{}/result.json", outdir)).expect("READ_RESULT_JSON");
    let s = String::from_utf8_lossy(&bytes);
    eprintln!("=== result.json bytes ===\n{}\n=== end ===", s);

    let j: serde_json::Value = serde_json::from_slice(&bytes).expect("PARSE_JSON");
    eprintln!("=== result.json parsed ===\n{}\n=== end ===", j);
}
