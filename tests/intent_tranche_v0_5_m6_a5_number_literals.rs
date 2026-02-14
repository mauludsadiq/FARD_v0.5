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

fn run_err_capture(prog_path: &str, outdir: &str) -> String {
    let _ = fs::remove_dir_all(outdir);
    let out = Command::new("cargo")
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
        .output()
        .expect("SPAWN_FARDRUN");
    assert!(!out.status.success(), "FARDRUN_EXPECTED_ERR");
    let mut s = String::new();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

#[test]
fn m6_a5_accepts_plain_ints_and_negatives() {
    let p = "out/_m6_a5_ok_1.fard";
    let o = "out/_m6_a5_ok_out_1";

    write_prog(p, "(-12) + 5");
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);

    assert_eq!(v, J::from(-7), "A5_EXPECTED_-7");
}

#[test]
fn m6_a5_rejects_leading_zero_int() {
    let p = "out/_m6_a5_bad_leading0.fard";
    let o = "out/_m6_a5_bad_leading0_out";
    write_prog(p, "01");
    let s = run_err_capture(p, o);
    assert!(s.contains("Error:"), "A5_EXPECTED_ERROR_PREFIX");
}

#[test]
fn m6_a5_rejects_float_form() {
    let p = "out/_m6_a5_bad_float.fard";
    let o = "out/_m6_a5_bad_float_out";
    write_prog(p, "1.0");
    let s = run_err_capture(p, o);
    assert!(s.contains("Error:"), "A5_EXPECTED_ERROR_PREFIX");
}

#[test]
fn m6_a5_rejects_scientific_notation() {
    let p = "out/_m6_a5_bad_sci.fard";
    let o = "out/_m6_a5_bad_sci_out";
    write_prog(p, "1e3");
    let s = run_err_capture(p, o);
    assert!(s.contains("Error:"), "A5_EXPECTED_ERROR_PREFIX");
}

#[test]
fn m6_a5_rejects_plus_prefix() {
    let p = "out/_m6_a5_bad_plus.fard";
    let o = "out/_m6_a5_bad_plus_out";
    write_prog(p, "+1");
    let s = run_err_capture(p, o);
    assert!(s.contains("Error:"), "A5_EXPECTED_ERROR_PREFIX");
}
