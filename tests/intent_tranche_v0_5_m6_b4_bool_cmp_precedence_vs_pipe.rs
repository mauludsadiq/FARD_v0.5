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
fn m6_b4_pipe_binds_tighter_than_eq() {
    let p = "out/_m6_b4_1.fard";
    let o = "out/_m6_b4_1_out";

    let src = r#"
fn inc(x) { x + 1 }
(1 |> inc) == 2
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(true), "B4_EXPECTED_true");
}

#[test]
fn m6_b4_pipe_binds_tighter_than_lt_gt() {
    let p = "out/_m6_b4_2.fard";
    let o = "out/_m6_b4_2_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }
(1 |> inc) < 3 && (2 |> dbl) > 3
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(true), "B4_EXPECTED_true");
}

#[test]
fn m6_b4_eq_binds_tighter_than_and() {
    let p = "out/_m6_b4_3.fard";
    let o = "out/_m6_b4_3_out";

    // Expect grouping: ((1 |> inc) == 2) && true
    // If it grouped as (1 |> inc) == (2 && true) you'd get an error / false.
    let src = r#"
fn inc(x) { x + 1 }
((1 |> inc) == 2) && true
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(true), "B4_EXPECTED_true");
}

#[test]
fn m6_b4_and_binds_tighter_than_or() {
    let p = "out/_m6_b4_4.fard";
    let o = "out/_m6_b4_4_out";

    // true || false && false  ==> true (because && binds tighter)
    // We also weave in a pipe expression on RHS.
    let src = r#"
fn inc(x) { x + 1 }
true || ((1 |> inc) == 0) && false
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(true), "B4_EXPECTED_true");
}

#[test]
fn m6_b4_pipe_can_feed_into_predicate_used_by_if() {
    let p = "out/_m6_b4_5.fard";
    let o = "out/_m6_b4_5_out";

    // Ensure predicate parses as: ((1 |> inc) == 2) not (1 |> (inc == 2))
    let src = r#"
fn inc(x) { x + 1 }
if (1 |> inc) == 2 then 10 else 20
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(10), "B4_EXPECTED_10");
}

#[test]
fn m6_b4_pipe_with_cmp_chain_inside_let_body() {
    let p = "out/_m6_b4_6.fard";
    let o = "out/_m6_b4_6_out";

    let src = r#"
fn inc(x) { x + 1 }
let x = 1 in ((x |> inc) >= 2) && ((x |> inc) <= 2)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(true), "B4_EXPECTED_true");
}

#[test]
fn m6_b4_pipe_left_associative_then_cmp_observes_final_value() {
    let p = "out/_m6_b4_7.fard";
    let o = "out/_m6_b4_7_out";

    // (1 |> inc |> inc) == 3 should be true if left-assoc holds.
    let src = r#"
fn inc(x) { x + 1 }
((1 |> inc |> inc) == 3)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(true), "B4_EXPECTED_true");
}
