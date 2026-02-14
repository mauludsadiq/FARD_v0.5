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
fn m6_b10_pipe_value_can_be_bound_by_let_in_and_used_downstream() {
    let p = "out/_m6_b10_1.fard";
    let o = "out/_m6_b10_1_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

let y = 1 |> inc in y |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "B10_EXPECTED_4");
}

#[test]
fn m6_b10_let_pattern_record_destructure_can_bind_pipe_produced_record() {
    let p = "out/_m6_b10_2.fard";
    let o = "out/_m6_b10_2_out";

    let src = r#"
fn mk(x) { {a: x, b: x + 2} }

let {a: a, b: b} = (1 |> mk) in a + b
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // a=1, b=3 => 4
    assert_eq!(v, J::from(4), "B10_EXPECTED_4");
}

#[test]
fn m6_b10_let_pattern_list_destructure_can_bind_pipe_produced_list() {
    let p = "out/_m6_b10_3.fard";
    let o = "out/_m6_b10_3_out";

    let src = r#"
fn mk(x) { [x, x + 1, x + 2] }

let [a, b, c] = (1 |> mk) in a + b + c
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // 1+2+3=6
    assert_eq!(v, J::from(6), "B10_EXPECTED_6");
}

#[test]
fn m6_b10_let_shadowing_does_not_break_pipe_chain_value_flow() {
    let p = "out/_m6_b10_4.fard";
    let o = "out/_m6_b10_4_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

let x = 1 in
  let x = x |> inc in
    x |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // inner x = 2; dbl => 4
    assert_eq!(v, J::from(4), "B10_EXPECTED_4");
}

#[test]
fn m6_b10_pipe_rhs_can_reference_let_bound_function_value() {
    let p = "out/_m6_b10_5.fard";
    let o = "out/_m6_b10_5_out";

    let src = r#"
let f = (x) => x + 10 in
  1 |> f
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(11), "B10_EXPECTED_11");
}

#[test]
fn m6_b10_pipe_in_let_bound_expr_does_not_require_parens_and_binds_as_expr() {
    let p = "out/_m6_b10_6.fard";
    let o = "out/_m6_b10_6_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

let x = 1 |> inc |> dbl in x
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // 1->2->4
    assert_eq!(v, J::from(4), "B10_EXPECTED_4");
}
