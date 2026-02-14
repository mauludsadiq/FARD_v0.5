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
fn m6_b8_match_scrutinee_can_be_pipe_expr_with_parens() {
    let p = "out/_m6_b8_1.fard";
    let o = "out/_m6_b8_1_out";

    let src = r#"
fn inc(x) { x + 1 }

match (1 |> inc) {
  2 => 10,
  _ => 20,
}
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(10), "B8_EXPECTED_10");
}

#[test]
fn m6_b8_match_arm_expr_can_pipe_and_commas_delimit_arms() {
    let p = "out/_m6_b8_2.fard";
    let o = "out/_m6_b8_2_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

match 1 {
  1 => 1 |> inc |> dbl,
  _ => 0,
}
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // 1 -> inc = 2 -> dbl = 4
    assert_eq!(v, J::from(4), "B8_EXPECTED_4");
}

#[test]
fn m6_b8_match_guard_can_be_pipe_expr_and_must_be_bool() {
    let p = "out/_m6_b8_3.fard";
    let o = "out/_m6_b8_3_out";

    let src = r#"
fn is_two(x) { x == 2 }
fn inc(x) { x + 1 }

match 1 {
  x if (x |> inc |> is_two) => 10,
  _ => 20,
}
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(10), "B8_EXPECTED_10");
}

#[test]
fn m6_b8_pipe_rhs_can_be_lambda_returning_bool_used_in_guard() {
    let p = "out/_m6_b8_4.fard";
    let o = "out/_m6_b8_4_out";

    // guard is: (x |> ((z)=> z == 3))
    let src = r#"
fn inc(x) { x + 1 }

match 2 {
  x if (x |> inc |> ((z)=> z == 3)) => 10,
  _ => 20,
}
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(10), "B8_EXPECTED_10");
}

#[test]
fn m6_b8_pipe_expression_in_arm_does_not_capture_following_arm() {
    let p = "out/_m6_b8_5.fard";
    let o = "out/_m6_b8_5_out";

    // If arm delimiting was wrong, parser would swallow `_ => ...` or mis-associate it.
    let src = r#"
fn inc(x) { x + 1 }

match 1 {
  1 => 1 |> inc,
  _ => 99,
}
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "B8_EXPECTED_2");
}

#[test]
fn m6_b8_pipe_can_appear_inside_pattern_bound_expression_then_match_on_value() {
    let p = "out/_m6_b8_6.fard";
    let o = "out/_m6_b8_6_out";

    let src = r#"
fn inc(x) { x + 1 }

let y = 1 |> inc in
match y {
  2 => 10,
  _ => 20,
}
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(10), "B8_EXPECTED_10");
}
