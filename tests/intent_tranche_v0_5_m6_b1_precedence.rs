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
fn m6_b1_mul_binds_tighter_than_add() {
    let p = "out/_m6_b1_prec_1.fard";
    let o = "out/_m6_b1_prec_out_1";
    write_prog(p, "1 + 2 * 3");
    run_ok(p, o);
    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(7), "B1_EXPECTED_7");
}

#[test]
fn m6_b1_parens_override_precedence() {
    let p = "out/_m6_b1_prec_2.fard";
    let o = "out/_m6_b1_prec_out_2";
    write_prog(p, "(1 + 2) * 3");
    run_ok(p, o);
    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(9), "B1_EXPECTED_9");
}

#[test]
fn m6_b1_unary_binds_tighter_than_mul() {
    let p = "out/_m6_b1_prec_3.fard";
    let o = "out/_m6_b1_prec_out_3";
    write_prog(p, "-1 * 3");
    run_ok(p, o);
    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(-3), "B1_EXPECTED_-3");
}
