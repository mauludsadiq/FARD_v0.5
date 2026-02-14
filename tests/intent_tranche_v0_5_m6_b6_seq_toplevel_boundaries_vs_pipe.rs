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
fn m6_b6_let_in_body_can_pipe_across_newlines() {
    let p = "out/_m6_b6_1.fard";
    let o = "out/_m6_b6_1_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }

let x = 1 in
  x |> inc
    |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "B6_EXPECTED_4");
}

#[test]
fn m6_b6_pipe_rhs_call_args_can_span_lines() {
    let p = "out/_m6_b6_2.fard";
    let o = "out/_m6_b6_2_out";

    // 2 |> add(1+2) == 5, with args split across lines.
    let src = r#"
fn add(x, y, z) { y + z }

2 |> add(
  1,
  2
)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(3), "B6_EXPECTED_3");
}

#[test]
fn m6_b6_pipe_inside_record_literal_value_expr() {
    let p = "out/_m6_b6_3.fard";
    let o = "out/_m6_b6_3_out";

    let src = r#"
fn inc(x) { x + 1 }
let r = { v: 1 |> inc }
r.v
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "B6_EXPECTED_2");
}

#[test]
fn m6_b6_pipe_inside_list_literal_element_expr() {
    let p = "out/_m6_b6_4.fard";
    let o = "out/_m6_b6_4_out";

    let src = r#"
fn inc(x) { x + 1 }
let xs = [1 |> inc, 2 |> inc]
xs
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(vec![2, 3]), "B6_EXPECTED_[2,3]");
}

#[test]
fn m6_b6_pipe_in_match_scrutinee_parens_required() {
    let p = "out/_m6_b6_5.fard";
    let o = "out/_m6_b6_5_out";

    // Explicit parens: match (1 |> inc) { ... }
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
    assert_eq!(v, J::from(10), "B6_EXPECTED_10");
}

#[test]
fn m6_b6_top_level_items_do_not_swallow_next_def_after_pipe() {
    let p = "out/_m6_b6_6.fard";
    let o = "out/_m6_b6_6_out";

    // Ensures parser treats `fn` as a new top-level item, not part of a dangling pipe RHS.
    let src = r#"
fn inc(x) { x + 1 }

let x = 1 |> inc

fn dbl(y) { y * 2 }

dbl(x)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "B6_EXPECTED_4");
}

#[test]
fn m6_b6_pipe_chain_can_be_final_expr_after_multiple_top_level_defs() {
    let p = "out/_m6_b6_7.fard";
    let o = "out/_m6_b6_7_out";

    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }
fn sq(x) { x * x }

1 |> inc |> dbl |> sq
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // 1->2->4->16
    assert_eq!(v, J::from(16), "B6_EXPECTED_16");
}
