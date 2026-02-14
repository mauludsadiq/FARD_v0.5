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
fn m6_b11_import_alias_member_can_be_pipe_rhs_function_value() {
    let p = "out/_m6_b11_1.fard";
    let o = "out/_m6_b11_1_out";

    // stdlib access is ONLY via import aliasing.
    // We pick std/list.map, which (in the pinned ontology) should exist and be callable.
    let src = r#"
import("std/list") as L

fn inc(x) { x + 1 }

[1,2,3] |> L.map(inc)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(vec![2, 3, 4]), "B11_EXPECTED_[2,3,4]");
}

#[test]
fn m6_b11_import_alias_member_call_can_take_pipe_value_as_first_arg() {
    let p = "out/_m6_b11_2.fard";
    let o = "out/_m6_b11_2_out";

    let src = r#"
import("std/list") as L

fn dbl(x) { x * 2 }

let xs = [1,2,3] in
  xs |> L.map(dbl)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(vec![2, 4, 6]), "B11_EXPECTED_[2,4,6]");
}

#[test]
fn m6_b11_pipeline_into_imported_member_then_postfix_field_access_on_result() {
    let p = "out/_m6_b11_3.fard";
    let o = "out/_m6_b11_3_out";

    // If std/list has len, use it; otherwise this will gate-fail and we’ll adjust.
    // This test is about: (xs |> L.len) participating in arithmetic.
    let src = r#"
import("std/list") as L

([1,2,3] |> L.len) + 10
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(13), "B11_EXPECTED_13");
}

#[test]
fn m6_b11_export_list_does_not_conflict_with_pipe_expression_items() {
    let p = "out/_m6_b11_4.fard";
    let o = "out/_m6_b11_4_out";

    // export item is parsed as top-level item; final expr still evaluated.
    let src = r#"
export { marker }

let marker = 0

fn inc(x) { x + 1 }

1 |> inc
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(
        v,
        J::from(serde_json::json!({"marker": 0})),
        "B11_EXPECTED_exports_marker_0"
    );
}

#[test]
fn m6_b11_unbound_std_namespace_is_runtime_error_even_with_pipe_present() {
    let p = "out/_m6_b11_5.fard";
    let o = "out/_m6_b11_5_out";

    // Reinforces notation evidence: std is NOT ambient.
    let src = r#"
fn inc(x) { x + 1 }
1 |> inc |> std.list.map
"#;

    write_prog(p, src);
    run_err(p, o);
}

#[test]
fn m6_b11_import_alias_is_lexical_and_can_be_shadowed_by_let_without_breaking_parse() {
    let p = "out/_m6_b11_6.fard";
    let o = "out/_m6_b11_6_out";

    // Alias L exists at module scope, but can be shadowed as a value inside let.
    // This is about boundary correctness, not “recommended style”.
    let src = r#"
import("std/list") as L

fn inc(x) { x + 1 }

let L = { map: (xs, f) => xs } in
  [1,2,3] |> L.map(inc)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // shadowed L.map returns xs unchanged
    assert_eq!(v, J::from(vec![1, 2, 3]), "B11_EXPECTED_[1,2,3]");
}
