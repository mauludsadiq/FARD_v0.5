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


#[test]
fn g54_smoke_tranche_file_loads() {
    let v = run_ok(
        r#"
import("std/json") as json

let x = {a: 1, b: 2}
emit({tag: "g54", x: x})
x
"#,
    );
    assert!(v.is_object(), "RESULT_NOT_OBJECT");
}

#[test]
fn g55_str_trim_lower_split_lines() {
    let v = run_ok(
        r#"
import("std/str") as str

let s = "  AbC \nDeF  \n"
let t = str.trim(s)
let u = str.lower(t)
let xs = str.split_lines(u)
emit({tag: "g55", t: t, u: u, xs: xs})
{t: t, u: u, xs: xs}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(
        o.get("t").and_then(|x| x.as_str()).expect("TYPE_FAIL t"),
        "AbC \nDeF"
    );
    assert_eq!(
        o.get("u").and_then(|x| x.as_str()).expect("TYPE_FAIL u"),
        "abc \ndef"
    );
    let xs = o
        .get("xs")
        .and_then(|x| x.as_array())
        .expect("TYPE_FAIL xs");
    assert_eq!(xs.len(), 2);
    assert_eq!(xs[0].as_str().unwrap(), "abc ");
    assert_eq!(xs[1].as_str().unwrap(), "def");
}

#[test]
fn g56_list_range_repeat_concat() {
    let v = run_ok(
        r#"
import("std/list") as list

let a = list.range(0, 5)        
let b = list.repeat("x", 3)     
let c = list.concat([a, [9], [10]])
emit({tag: "g56", a: a, b: b, c: c})
{a: a, b: b, c: c}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    let a = o.get("a").and_then(|x| x.as_array()).expect("TYPE_FAIL a");
    assert_eq!(a.len(), 5);
    assert_eq!(a[0].as_i64().unwrap(), 0);
    assert_eq!(a[4].as_i64().unwrap(), 4);

    let b = o.get("b").and_then(|x| x.as_array()).expect("TYPE_FAIL b");
    assert_eq!(b.len(), 3);

    let c = o.get("c").and_then(|x| x.as_array()).expect("TYPE_FAIL c");
    assert_eq!(c.len(), 7);
    assert_eq!(c[5].as_i64().unwrap(), 9);
    assert_eq!(c[6].as_i64().unwrap(), 10);
}

#[test]
fn g57_list_map_filter_fold() {
    let v = run_ok(
        r#"
import("std/list") as list

let xs = list.range(0, 10)
let evens = list.filter(xs, fn(x){ 1 - (x - ((x / 2) * 2)) })
let sq = list.map(evens, fn(x){ x * x })
let sum = list.fold(sq, 0, fn(acc, x){ acc + x })
emit({tag: "g57", sum: sum})
{sum: sum}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(
        o.get("sum")
            .and_then(|x| x.as_i64())
            .expect("TYPE_FAIL sum"),
        120
    );
}

#[test]
fn g58_int_parse_pow() {
    let v = run_ok(
        r#"
import("std/int") as int

let a = int.parse("42")?
let b = int.pow(2, 10)
emit({tag: "g58", a: a, b: b})
{a: a, b: b}
"#,
    );

    let o = v.as_object().expect("TYPE_FAIL result.obj");
    assert_eq!(o.get("a").and_then(|x| x.as_i64()).unwrap(), 42);
    assert_eq!(o.get("b").and_then(|x| x.as_i64()).unwrap(), 1024);
}

#[test]
fn g59_artifact_import_and_derived_emit_artifact() {
    let outdir = "out/g59";
    let _ = fs::remove_dir_all(outdir);
    fs::create_dir_all(format!("{}/artifacts", outdir)).expect("MKDIR_ART_FAIL");
    fs::write(format!("{}/artifacts/in.txt", outdir), b"hello\n").expect("WRITE_ART_FAIL");

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(
        "spec/tmp/g59.fard",
        r#"
import("std/str") as str

let a = import_artifact("in.txt")?
let s = str.lower(a.text)
emit({tag: "g59", n: len(s)})
emit_artifact("out.txt", {text: s})?
{n: len(s)}
        "#,
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
            "spec/tmp/g59.fard",
            "--out",
            outdir,
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL");

    assert!(status.success(), "RUNNER_NONZERO");

    let out_txt =
        fs::read_to_string(format!("{}/artifacts/out.txt", outdir)).expect("READ_OUT_ART_FAIL");
    assert_eq!(out_txt, "hello\n");
}

#[test]
fn g60_trace_has_emit_events() {
    let outdir = "out/g60";
    let _ = fs::remove_dir_all(outdir);

    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(
        "spec/tmp/g60.fard",
        r#"
emit({tag: "g60", a: 1})
emit({tag: "g60", a: 2})
{ok: true}
        "#,
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
            "spec/tmp/g60.fard",
            "--out",
            outdir,
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL");

    assert!(status.success(), "RUNNER_NONZERO");

    let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");
    assert!(trace.contains("\"tag\":\"g60\""), "TRACE_MISSING_G60");
}
