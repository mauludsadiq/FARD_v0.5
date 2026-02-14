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
fn m6_b5_pipe_value_can_participate_in_add() {
    let p = "out/_m6_b5_1.fard";
    let o = "out/_m6_b5_1_out";

    let src = r#"
fn inc(x) { x + 1 }
(1 |> inc) + 10
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(12), "B5_EXPECTED_12");
}

#[test]
fn m6_b5_mul_binds_tighter_than_add_with_pipe_inputs() {
    let p = "out/_m6_b5_2.fard";
    let o = "out/_m6_b5_2_out";

    // (1|>inc) + (2|>dbl) * 3 = 2 + 4*3 = 14
    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }
(1 |> inc) + ((2 |> dbl) * 3)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(14), "B5_EXPECTED_14");
}

#[test]
fn m6_b5_parens_override_with_pipe_inside() {
    let p = "out/_m6_b5_3.fard";
    let o = "out/_m6_b5_3_out";

    // ((1|>inc) + 2) * 3 = (2+2)*3 = 12
    let src = r#"
fn inc(x) { x + 1 }
((1 |> inc) + 2) * 3
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(12), "B5_EXPECTED_12");
}

#[test]
fn m6_b5_unary_minus_binds_tighter_than_mul_with_pipe_lhs() {
    let p = "out/_m6_b5_4.fard";
    let o = "out/_m6_b5_4_out";

    // -(1|>inc) * 2 = -2 * 2 = -4
    let src = r#"
fn inc(x) { x + 1 }
(-(1 |> inc)) * 2
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(-4), "B5_EXPECTED_-4");
}

#[test]
fn m6_b5_pipe_chain_then_arith_observes_final_value() {
    let p = "out/_m6_b5_5.fard";
    let o = "out/_m6_b5_5_out";

    // (1|>inc|>dbl) + 1 = (2*2)+1 = 5
    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }
(1 |> inc |> dbl) + 1
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(5), "B5_EXPECTED_5");
}

#[test]
fn m6_b5_arith_inside_rhs_of_pipe_is_single_postfix_expr() {
    let p = "out/_m6_b5_6.fard";
    let o = "out/_m6_b5_6_out";

    // 2 |> add(1+2)  ==> add(2, 3) = 5
    // This checks that (1+2) parses as an argument expression before call completes, then pipe inserts LHS.
    let src = r#"
fn add(x, y) { x + y }
2 |> add(1 + 2)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(5), "B5_EXPECTED_5");
}

#[test]
fn m6_b5_pipe_can_feed_into_arith_via_lambda() {
    let p = "out/_m6_b5_7.fard";
    let o = "out/_m6_b5_7_out";

    // 3 |> ((x)=> x*10 + 1) == 31
    let src = r#"
let f = (x) => (x * 10) + 1
3 |> f
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(31), "B5_EXPECTED_31");
}
