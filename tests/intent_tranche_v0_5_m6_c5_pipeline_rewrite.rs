use std::fs;
use std::process::Command;

use serde_json::Value as J;

include!("_m6_json.rs.inc");

fn write_prog(path: &str, src: &str) {
    let _ = fs::create_dir_all("out");
    fs::write(path, src.as_bytes()).expect("WRITE_PROG");
}

fn run_ok(prog_path: &str, outdir: &str) {
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
            prog_path,
            "--out",
            outdir,
        ])
        .status()
        .expect("SPAWN_FARDRUN");
    assert!(st.success(), "FARDRUN_EXPECTED_OK");
}

#[test]
fn m6_c5_pipe_inserts_value_as_first_arg() {
    let p = "out/_m6_c5_pipe_1.fard";
    let o = "out/_m6_c5_pipe_out_1";

    let src = r#"
fn inc(x) { x + 1 }
1 |> inc
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "C5_EXPECTED_2");
}

#[test]
fn m6_c5_pipe_chains_left_to_right() {
    let p = "out/_m6_c5_pipe_2.fard";
    let o = "out/_m6_c5_pipe_out_2";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }
1 |> inc |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "C5_EXPECTED_4");
}
