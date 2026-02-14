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
fn m6_b3_lambda_body_is_pipe_expression() {
    let p = "out/_m6_b3_lambda_1.fard";
    let o = "out/_m6_b3_lambda_1_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

let f = (x) => x |> inc |> dbl
f(1)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "B3_EXPECTED_4");
}

#[test]
fn m6_b3_if_branches_can_pipe() {
    let p = "out/_m6_b3_if_1.fard";
    let o = "out/_m6_b3_if_1_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

if true then 1 |> inc else 1 |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "B3_EXPECTED_2");
}

#[test]
fn m6_b3_match_arm_expr_can_pipe() {
    let p = "out/_m6_b3_match_1.fard";
    let o = "out/_m6_b3_match_1_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

match 1 {
  1 => 1 |> inc,
  _ => 1 |> dbl,
}
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "B3_EXPECTED_2");
}

#[test]
fn m6_b3_let_body_can_pipe() {
    let p = "out/_m6_b3_let_1.fard";
    let o = "out/_m6_b3_let_1_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

let x = 1 in x |> inc |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "B3_EXPECTED_4");
}

#[test]
fn m6_b3_pipe_can_feed_into_if_expression() {
    let p = "out/_m6_b3_pipe_to_if_1.fard";
    let o = "out/_m6_b3_pipe_to_if_1_out";

    // LHS pipes into a function that returns a plain value.
    // Ensure the parse groups as: (1 |> inc) |> ((x)=> if ...)
    let src = r#"
fn inc(x) { x + 1 }
let chooser = (x) => if x == 2 then 10 else 20

1 |> inc |> chooser
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(10), "B3_EXPECTED_10");
}
