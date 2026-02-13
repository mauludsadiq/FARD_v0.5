use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static RUN_ID: AtomicUsize = AtomicUsize::new(1);

fn assert_contains(hay: &str, needle: &str) {
    if !hay.contains(needle) {
        panic!(
            "ASSERT_CONTAINS_FAIL needle={:?}\nHAYSTACK:\n{}",
            needle, hay
        );
    }
}

fn read_result_json(outdir: &str) -> String {
    fs::read_to_string(format!("{}/result.json", outdir)).expect("READ_RESULT_FAIL")
}

fn read_trace(outdir: &str) -> String {
    fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL")
}

fn run_ok(program_src: &str, tag: &str) -> (String, String) {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/{}_{}", tag, id);
    let program = format!("spec/tmp/{}_{}.fard", tag, id);
    let _ = fs::remove_dir_all(&outdir);
    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(&program, program_src.as_bytes()).expect("WRITE_SRC_FAIL");

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

    assert!(status.success(), "EXPECTED_ZERO");
    (read_result_json(&outdir), read_trace(&outdir))
}

fn run_err_trace(program_src: &str, tag: &str) -> String {
    let id = RUN_ID.fetch_add(1, Ordering::SeqCst);
    let outdir = format!("out/{}_{}", tag, id);
    let program = format!("spec/tmp/{}_{}.fard", tag, id);
    let _ = fs::remove_dir_all(&outdir);
    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(&program, program_src.as_bytes()).expect("WRITE_SRC_FAIL");

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
    read_trace(&outdir)
}

/*
M1 CLOSE (single shot)

We freeze Result semantics without relying on result.json being a Result record.

Strategy:
- Use FARD-side match as the canonical-shape oracle.
- The program returns 1 iff shape/algebra is correct.
- Host asserts result.json contains {"result":1} (runner's stable wrapper).
- Err-path asserts trace-only with frozen literals.
*/

#[test]
fn g78_result_ok_constructor_is_canonical_shape() {
    let (result_json, _trace) = run_ok(
        r#"
import("std/result") as result

let r = result.ok(123)

match r {
  {t:"ok", v:123} => 1,
  _ => 0
}
"#,
        "g78",
    );

    assert_contains(&result_json, r#""result":1"#);
}

#[test]
fn g79_result_err_constructor_is_canonical_shape() {
    let (result_json, _trace) = run_ok(
        r#"
import("std/result") as result

let r = result.err("boom")

match r {
  {t:"err", e:"boom"} => 1,
  _ => 0
}
"#,
        "g79",
    );

    assert_contains(&result_json, r#""result":1"#);
}

#[test]
fn g80_andthen_ok_calls_f_and_returns_f_result() {
    let (result_json, _trace) = run_ok(
        r#"
import("std/result") as result

let f = fn(x) { result.ok(x + 1) }

let r = result.andThen(result.ok(41), f)

match r {
  {t:"ok", v:42} => 1,
  _ => 0
}
"#,
        "g80",
    );

    assert_contains(&result_json, r#""result":1"#);
}

#[test]
fn g81_andthen_err_passthrough_preserves_e() {
    let (result_json, _trace) = run_ok(
        r#"
import("std/result") as result

let f = fn(x) { result.ok(x + 1) }

let r = result.andThen(result.err("E0"), f)

match r {
  {t:"err", e:"E0"} => 1,
  _ => 0
}
"#,
        "g81",
    );

    assert_contains(&result_json, r#""result":1"#);
}

#[test]
fn g82_andthen_ok_requires_f_return_result_shape() {
    let trace = run_err_trace(
        r#"
import("std/result") as result

let bad = fn(x) { 7 }   // not a Result

let _ = result.andThen(result.ok(1), bad)

0
"#,
        "g82",
    );

    assert_contains(&trace, r#""t":"error""#);
    assert_contains(&trace, "QMARK_EXPECT_RESULT expected result");
}

#[test]
fn g83_qmark_ok_yields_v() {
    let (result_json, _trace) = run_ok(
        r#"
import("std/result") as result

let x = result.ok(9)?

x
"#,
        "g83",
    );

    assert_contains(&result_json, r#""result":9"#);
}

#[test]
fn g84_qmark_err_is_frozen_propagation() {
    let trace = run_err_trace(
        r#"
import("std/result") as result

let _ = result.err("E_Q")?

0
"#,
        "g84",
    );

    assert_contains(&trace, r#""t":"error""#);
    // Your runtime is already emitting this (seen on stderr): freeze it in trace assertions.
    assert_contains(&trace, "QMARK_PROPAGATE_ERR");
    assert_contains(&trace, r#""e":"E_Q""#);
}
