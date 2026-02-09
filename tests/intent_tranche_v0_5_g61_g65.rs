use std::fs;
use std::process::Command;

use std::sync::atomic::{AtomicUsize, Ordering};

static RUN_ID: AtomicUsize = AtomicUsize::new(0);

fn run_ok(src: &str) -> serde_json::Value {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g54_g60_{}", id);
    let program = format!("spec/tmp/g54_g60_kitchen_sink_{}.fard", id);
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
fn g61_result_andthen_ok_executes_callback_and_returns_it() {
    let v = run_ok(
        r#"
import("std/result") as result
let r = result.ok(5)
let out = result.andThen(r, fn(x){ result.ok(x + 1) })
out
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("t").and_then(|x| x.as_str()).expect("TYPE_FAIL t"), "ok");
    assert_eq!(o.get("v").and_then(|x| x.as_i64()).expect("TYPE_FAIL v"), 6);
}

#[test]
fn g62_result_andthen_err_passes_through_without_calling_callback() {
    let v = run_ok(
        r#"
import("std/result") as result
let r = result.err({code:"E", msg:"no"})
let out = result.andThen(r, fn(x){ result.ok(x + 1) })
out
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("t").and_then(|x| x.as_str()).expect("TYPE_FAIL t"), "err");
    let e = o.get("e").and_then(|x| x.as_object()).expect("TYPE_FAIL e.obj");
    assert_eq!(e.get("code").and_then(|x| x.as_str()).expect("TYPE_FAIL e.code"), "E");
}

#[test]
fn g63_result_andthen_callback_must_return_canonical_result() {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g63_{}", id);
    let program = format!("spec/tmp/g63_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(&program, r#"
import("std/result") as result
let r = result.ok(5)
result.andThen(r, fn(x){ x + 1 })
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
    assert_any_contains(&trace, &["QMARK_EXPECT_RESULT","EXPORT_MISSING","ERROR_BADARG"]);
}

#[test]
fn g64_result_andthen_rejects_non_result_input() {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g64_{}", id);
    let program = format!("spec/tmp/g64_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(&program, r#"
import("std/result") as result
result.andThen(5, fn(x){ result.ok(x + 1) })
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
    assert_any_contains(&trace, &["ERROR_BADARG","QMARK_EXPECT_RESULT","EXPORT_MISSING"]);
}

#[test]
fn g65_qmark_propagates_canonical_err_record() {
    let v = run_ok(
        r#"
import("std/result") as result
fn f() {
  let x = result.err({code:"E_Q", msg:"q"})?
  result.ok(x)
}
f()
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("t").and_then(|x| x.as_str()).expect("TYPE_FAIL t"), "err");
    let e = o.get("e").and_then(|x| x.as_object()).expect("TYPE_FAIL e.obj");
    assert_eq!(e.get("code").and_then(|x| x.as_str()).expect("TYPE_FAIL e.code"), "E_Q");
}
