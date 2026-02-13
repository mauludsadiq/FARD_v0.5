use std::fs;
use std::process::Command;

use std::sync::atomic::{AtomicUsize, Ordering};

static RUN_ID: AtomicUsize = AtomicUsize::new(0);

fn run_ok(src: &str) -> serde_json::Value {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g66_g70_{}", id);
    let program = format!("spec/tmp/g66_g70_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(&program, src.as_bytes()).expect("WRITE_SRC_FAIL");

    let status = Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardrun",
            "--",
            "run",
            "--program",
            &program,
            "--out",
            &outdir,
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL");

    assert!(status.success(), "RUNNER_NONZERO");

    let bytes = fs::read(format!("{}/result.json", outdir)).expect("READ_RESULT_FAIL");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("RESULT_JSON_PARSE_FAIL");
    v.get("result").cloned().expect("RESULT_MISSING_RESULT_KEY")
}





fn assert_contains(hay: &str, needle: &str) {
    assert!(hay.contains(needle), "missing needle: {}\nHAY:\n{}", needle, hay);
}

fn assert_any_contains(hay: &str, needles: &[&str]) {
    for n in needles {
        if hay.contains(n) {
            return;
        }
    }
    panic!("missing any needle {:?}\nHAY:\n{}", needles, hay);
}


#[test]
fn g66_andthen_left_identity() {
    let v = run_ok(
        r#"
import("std/result") as result
fn f(x){ result.ok(x + 2) }
let lhs = result.andThen(result.ok(5), f)
let rhs = f(5)
{lhs: lhs, rhs: rhs}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("lhs"), o.get("rhs"), "LEFT_IDENTITY_FAIL");
}

#[test]
fn g67_andthen_right_identity_ok_and_err() {
    let v = run_ok(
        r#"
import("std/result") as result
fn id(x){ result.ok(x) }

let a = result.andThen(result.ok(7), id)
let b = result.ok(7)

let c = result.andThen(result.err({code:"E", msg:"m"}), id)
let d = result.err({code:"E", msg:"m"})

{a:a, b:b, c:c, d:d}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("a"), o.get("b"), "RIGHT_IDENTITY_OK_FAIL");
    assert_eq!(o.get("c"), o.get("d"), "RIGHT_IDENTITY_ERR_FAIL");
}

#[test]
fn g68_andthen_associativity_ok_path() {
    let v = run_ok(
        r#"
import("std/result") as result
fn f(x){ result.ok(x + 1) }
fn g(x){ result.ok(x * 2) }

let r = result.ok(10)

let lhs = result.andThen(result.andThen(r, f), g)
let rhs = result.andThen(r, fn(x){ result.andThen(f(x), g) })

{lhs: lhs, rhs: rhs}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("lhs"), o.get("rhs"), "ASSOCIATIVITY_FAIL");
}

#[test]
fn g69_qmark_unwind_equals_explicit_err() {
    let v = run_ok(
        r#"
import("std/result") as result

fn via_qmark() {
  let _ = result.err({code:"E_Q", msg:"q"})?
  result.ok(0)
}

let a = via_qmark()
let b = result.err({code:"E_Q", msg:"q"})
{a:a, b:b}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("a"), o.get("b"), "QMARK_EQUIV_FAIL");
}

#[test]
fn g70_andthen_callback_result_shape_is_checked() {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g66_g70_g70_{}", id);
    let program = format!("spec/tmp/g66_g70_g70_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(&program, r#"
import("std/result") as result
let r = result.ok(5)
result.andThen(r, fn(x){ {t:"ok"} })
"#.as_bytes()).expect("WRITE_SRC_FAIL");

    let status = Command::new("cargo")
        .args([
            "run","-q","--bin","fardrun","--","run","--program",&program,"--out",&outdir
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL");

    assert!(!status.success(), "EXPECTED_NONZERO");
    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");
    assert_contains(&trace, r#""t":"error""#);
    assert_any_contains(&trace, &["QMARK_EXPECT_RESULT","ERROR_BADARG"]);
}
