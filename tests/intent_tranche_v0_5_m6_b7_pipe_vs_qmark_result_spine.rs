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

fn run_err(prog_path: &str, outdir: &str) {
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
    assert!(!st.success(), "FARDRUN_EXPECTED_ERR");
}

#[test]
fn m6_b7_pipe_produces_result_then_qmark_unwraps_ok() {
    let p = "out/_m6_b7_1.fard";
    let o = "out/_m6_b7_1_out";

    let src = r#"
fn ok_inc(x) { {t:"ok", v: x + 1} }

(1 |> ok_inc)?
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "B7_EXPECTED_2");
}

#[test]
fn m6_b7_qmark_unwraps_then_pipe_feeds_plain_value() {
    let p = "out/_m6_b7_2.fard";
    let o = "out/_m6_b7_2_out";

    let src = r#"
fn dbl(x) { x * 2 }

({t:"ok", v: 3})? |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(6), "B7_EXPECTED_6");
}

#[test]
fn m6_b7_missing_parens_turns_qmark_into_postfix_on_rhs_callable_and_should_error() {
    let p = "out/_m6_b7_3.fard";
    let o = "out/_m6_b7_3_out";

    // `?` binds tighter than `|>` only when it can attach as postfix.
    // Here, `ok_inc?` applies `?` to a function value (not a Result), which must error.
    let src = r#"
fn ok_inc(x) { {t:"ok", v: x + 1} }

1 |> ok_inc?
"#;

    write_prog(p, src);
    run_err(p, o);
}

#[test]
fn m6_b7_qmark_rejects_non_result_even_if_it_came_from_pipe() {
    let p = "out/_m6_b7_4.fard";
    let o = "out/_m6_b7_4_out";

    let src = r#"
fn inc(x) { x + 1 }

(1 |> inc)?
"#;

    write_prog(p, src);
    run_err(p, o);
}

#[test]
fn m6_b7_pipe_produces_err_then_qmark_propagates_and_should_error() {
    let p = "out/_m6_b7_5.fard";
    let o = "out/_m6_b7_5_out";

    let src = r#"
fn bad(x) { {t:"err", e: x} }

(1 |> bad)?
"#;

    write_prog(p, src);
    run_err(p, o);
}

#[test]
fn m6_b7_qmark_inside_rhs_call_args_evaluates_before_pipe_insertion_then_outer_qmark_unwraps() {
    let p = "out/_m6_b7_6.fard";
    let o = "out/_m6_b7_6_out";

    // pipe inserts LHS as first arg: 2 |> sum3(1, ({ok 2})?) == sum3(2,1,2) => ok(5)
    // then outer ? unwraps to 5
    let src = r#"
fn sum3(z, a, b) { {t:"ok", v: z + a + b} }

(2 |> sum3(1, ({t:"ok", v: 2})?))?
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(5), "B7_EXPECTED_5");
}
