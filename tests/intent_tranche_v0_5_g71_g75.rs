use std::fs;
use std::process::Command;

use std::sync::atomic::{AtomicUsize, Ordering};

static RUN_ID: AtomicUsize = AtomicUsize::new(0);

fn run_ok(src: &str) -> serde_json::Value {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g71_g75_{}", id);
    let program = format!("spec/tmp/g71_g75_kitchen_sink_{}.fard", id);
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
    assert!(
        hay.contains(needle),
        "missing needle: {}\nHAY:\n{}",
        needle,
        hay
    );
}

#[test]
fn g71_unwrap_ok_on_ok_returns_v() {
    let v = run_ok(
        r#"
import("std/result") as result
let r = result.ok(41)
result.unwrap_ok(r)
"#,
    );
    assert_eq!(v.as_i64().expect("TYPE_FAIL int"), 41);
}

#[test]
fn g72_unwrap_err_on_err_returns_e() {
    let v = run_ok(
        r#"
import("std/result") as result
let r = result.err({code:"E", msg:"m"})
result.unwrap_err(r)
"#,
    );
    let o = v.as_object().expect("TYPE_FAIL rec");
    assert_eq!(
        o.get("code")
            .and_then(|x| x.as_str())
            .expect("TYPE_FAIL code"),
        "E"
    );
    assert_eq!(
        o.get("msg")
            .and_then(|x| x.as_str())
            .expect("TYPE_FAIL msg"),
        "m"
    );
}

#[test]
fn g73_unwrap_ok_on_err_is_deterministic_error() {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g73_{}", id);
    let program = format!("spec/tmp/g73_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(
        &program,
        r#"
import("std/result") as result
let r = result.err({code:"E", msg:"m"})
result.unwrap_ok(r)
"#
        .as_bytes(),
    )
    .expect("WRITE_SRC_FAIL");

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

    assert!(!status.success(), "EXPECTED_NONZERO");
    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");
    assert_contains(&trace, r#""t":"error""#);
    assert_contains(&trace, "QMARK_EXPECT_RESULT tried unwrap ok on err");
}

#[test]
fn g74_unwrap_err_on_ok_is_deterministic_error() {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g74_{}", id);
    let program = format!("spec/tmp/g74_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(
        &program,
        r#"
import("std/result") as result
let r = result.ok(9)
result.unwrap_err(r)
"#
        .as_bytes(),
    )
    .expect("WRITE_SRC_FAIL");

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

    assert!(!status.success(), "EXPECTED_NONZERO");
    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");
    assert_contains(&trace, r#""t":"error""#);
    assert_contains(&trace, "QMARK_EXPECT_RESULT tried unwrap err on ok");
}

#[test]
fn g75_match_on_canonical_result_tags_is_stable() {
    let v = run_ok(
        r#"
import("std/result") as result

fn tag(r) {
  match r {
    {t:"ok", v:x} => "ok",
    {t:"err", e:_} => "err",
    _ => "other"
  }
}

{a: tag(result.ok(1)), b: tag(result.err({code:"E", msg:"m"}))}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL rec");
    assert_eq!(
        o.get("a").and_then(|x| x.as_str()).expect("TYPE_FAIL a"),
        "ok"
    );
    assert_eq!(
        o.get("b").and_then(|x| x.as_str()).expect("TYPE_FAIL b"),
        "err"
    );
}

#[test]
fn g76_unwrap_ok_missing_v_is_frozen_error() {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g76_{}", id);
    let program = format!("spec/tmp/g76_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(
        &program,
        r#"
import("std/result") as result
result.unwrap_ok({t:"ok"})
"#
        .as_bytes(),
    )
    .expect("WRITE_SRC_FAIL");

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

    assert!(!status.success(), "EXPECTED_NONZERO");
    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");
    assert_contains(&trace, r#""t":"error""#);
    assert_contains(&trace, "QMARK_EXPECT_RESULT ok missing v");
}

#[test]
fn g77_unwrap_err_missing_e_is_frozen_error() {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/g77_{}", id);
    let program = format!("spec/tmp/g77_{}.fard", id);
    let _ = fs::remove_dir_all(&outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(
        &program,
        r#"
import("std/result") as result
result.unwrap_err({t:"err"})
"#
        .as_bytes(),
    )
    .expect("WRITE_SRC_FAIL");

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

    assert!(!status.success(), "EXPECTED_NONZERO");
    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");
    assert_contains(&trace, r#""t":"error""#);
    assert_contains(&trace, "QMARK_EXPECT_RESULT err missing e");
}
