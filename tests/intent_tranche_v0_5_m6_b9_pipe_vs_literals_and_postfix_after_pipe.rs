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
fn m6_b9_pipe_can_produce_record_then_field_access_reads_value() {
    let p = "out/_m6_b9_1.fard";
    let o = "out/_m6_b9_1_out";

    let src = r#"
fn mk(x) { {v: x + 1} }
(1 |> mk).v
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "B9_EXPECTED_2");
}

#[test]
fn m6_b9_pipe_can_produce_list_then_index_like_access_is_not_assumed_but_list_is_jsonable() {
    let p = "out/_m6_b9_2.fard";
    let o = "out/_m6_b9_2_out";

    let src = r#"
fn mk(x) { [x, x + 1] }
1 |> mk
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(vec![1, 2]), "B9_EXPECTED_[1,2]");
}

#[test]
fn m6_b9_pipe_rhs_can_be_record_literal_value_expr() {
    let p = "out/_m6_b9_3.fard";
    let o = "out/_m6_b9_3_out";

    // 7 |> {v:(x)=>x+1}.v  ==> ( (z)=>z+1 )(7) == 8
    let src = r#"
7 |> {v: (x)=> x + 1}.v
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(8), "B9_EXPECTED_8");
}

#[test]
fn m6_b9_pipe_rhs_can_be_paren_record_then_field_then_call_args_follow() {
    let p = "out/_m6_b9_4.fard";
    let o = "out/_m6_b9_4_out";

    // 3 |> ({f:(x,y)=>x+y}.f)(10) ==> f(3,10)=13
    let src = r#"
3 |> ({f: (x, y)=> x + y}.f)(10)
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(13), "B9_EXPECTED_13");
}

#[test]
fn m6_b9_pipe_inside_record_value_does_not_escape_record_literal() {
    let p = "out/_m6_b9_5.fard";
    let o = "out/_m6_b9_5_out";

    let src = r#"
fn inc(x) { x + 1 }
let r = {a: 1, b: 1 |> inc, c: 3}
r.b
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(2), "B9_EXPECTED_2");
}

#[test]
fn m6_b9_pipe_inside_list_elements_does_not_escape_list_literal() {
    let p = "out/_m6_b9_6.fard";
    let o = "out/_m6_b9_6_out";

    let src = r#"
fn inc(x) { x + 1 }
[1 |> inc, 2, 3 |> inc]
"#;

    write_prog(p, src);
    run_ok(p, o);

    let j = read_json(&format!("{}/result.json", o));
    let v = unwrap_ok_value_from_result_json(&j);
    assert_eq!(v, J::from(vec![2, 2, 4]), "B9_EXPECTED_[2,2,4]");
}
