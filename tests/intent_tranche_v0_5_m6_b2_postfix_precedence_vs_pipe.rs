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
fn m6_b2_postfix_binds_tighter_than_pipe_call() {
    let p = "out/_m6_b2_1.fard";
    let o = "out/_m6_b2_1_out";

    // f(10).inc()? |> g
    // must parse as: (f(10).inc()? ) |> g
    // NOT as: f(10).inc()? |> g  where postfix attaches to g(...)
    let src = r#"
fn f(x) { {inc: (y)=> {t:"ok", v: y + x}} }
fn g(z) { z * 2 }
f(1).inc(10)? |> g
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    // f(1).inc(10) = 11; |> g => g(11)=22
    assert_eq!(v, J::from(22), "B2_EXPECTED_22");
}

#[test]
fn m6_b2_field_access_binds_tighter_than_pipe() {
    let p = "out/_m6_b2_2.fard";
    let o = "out/_m6_b2_2_out";

    // {v:3}.v |> inc  == 4
    let src = r#"
fn inc(x) { x + 1 }
{v: 3}.v |> inc
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "B2_EXPECTED_4");
}

#[test]
fn m6_b2_postfix_chain_on_rhs_is_callable_then_pipe_inserts_lhs() {
    let p = "out/_m6_b2_3.fard";
    let o = "out/_m6_b2_3_out";

    // rhs is a postfix chain that produces a function: obj.m(5) returns (z)=>...
    // 7 |> obj.m(5)  ==> (obj.m(5))(7) with 7 inserted as first arg
    let src = r#"
let obj = {
  m: (z, k) => z + k
}
7 |> obj.m(5)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(12), "B2_EXPECTED_12");
}

#[test]
fn m6_b2_pipe_is_left_associative_over_postfix_values() {
    let p = "out/_m6_b2_4.fard";
    let o = "out/_m6_b2_4_out";

    // (1 |> inc) |> dbl == 4
    // If it grouped wrong, you’d see other values.
    let src = r#"
fn inc(x) { x + 1 }
fn dbl(x) { x * 2 }
1 |> inc |> dbl
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(4), "B2_EXPECTED_4");
}
