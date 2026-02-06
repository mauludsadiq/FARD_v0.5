use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TMPCTR: AtomicUsize = AtomicUsize::new(0);

fn tmpdir(prefix: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let c = TMPCTR.fetch_add(1, Ordering::SeqCst);
    d.push(format!("{}_{}_{}_{}", prefix, std::process::id(), t, c));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(p: &Path, bytes: &[u8]) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, bytes).unwrap();
}

fn read_json(path: &Path) -> serde_json::Value {
    let b = fs::read(path).unwrap();
    serde_json::from_slice(&b).unwrap()
}

fn run_prog(src: &str) -> (i32, String, String, PathBuf) {
    let d = tmpdir("fard_intent");
    let prog = d.join("main.fard");
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    write_file(&prog, src.as_bytes());

    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-q",
        "--bin",
        "fardrun",
        "--",
        "run",
        "--program",
        prog.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);

    let outp = cmd.output().unwrap();
    let code = outp.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&outp.stdout).to_string();
    let stderr = String::from_utf8_lossy(&outp.stderr).to_string();
    (code, stdout, stderr, out)
}

fn assert_ok_run(src: &str) -> (serde_json::Value, PathBuf) {
    let (code, _stdout, stderr, out) = run_prog(src);
    assert_eq!(code, 0, "expected exit 0; stderr:\n{stderr}");
    let err = out.join("error.json");
    assert!(!err.exists(), "error.json must not exist");
    assert!(out.join("trace.ndjson").exists(), "trace.ndjson must exist");
    let res = read_json(&out.join("result.json"));
    (res, out)
}

fn assert_err_run(src: &str) -> (serde_json::Value, String, PathBuf) {
    let (code, _stdout, stderr, out) = run_prog(src);
    assert_ne!(code, 0, "expected nonzero exit; stderr:\n{stderr}");
    let errp = out.join("error.json");
    assert!(errp.exists(), "error.json must exist");
    assert!(out.join("trace.ndjson").exists(), "trace.ndjson must exist");
    let err = read_json(&errp);
    (err, stderr, out)
}

fn j(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

fn assert_result_eq(res: &serde_json::Value, expected: serde_json::Value) {
    assert_eq!(res["result"], expected);
}

fn assert_err_code(err: &serde_json::Value, expected_code: &str) {
    assert_eq!(err["code"], serde_json::json!(expected_code));
}

fn assert_err_msg_contains(err: &serde_json::Value, needle: &str) {
    let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        msg.contains(needle),
        "expected error.message to contain {needle:?}; got: {msg:?}"
    );
}

#[test]
fn g48_pipe_value_to_callable() {
    let (res, _out) = assert_ok_run("fn id(x) { x }\n1 | id\n");
    assert_result_eq(&res, serde_json::json!(1));
}

#[test]
fn g48_pipe_value_to_call_with_args() {
    let (res, _out) = assert_ok_run("fn pair(a, b) { [a, b] }\n1 | pair(9)\n");
    assert_result_eq(&res, serde_json::json!([1, 9]));
}

#[test]
fn g48_pipe_chain_left_assoc() {
    let (res, _out) = assert_ok_run("fn inc(x) { x + 1 }\nfn dbl(x) { x * 2 }\n3 | inc | dbl\n");
    assert_result_eq(&res, serde_json::json!(8));
}

#[test]
fn g49_qmark_ok_unwrap() {
    let (res, _out) = assert_ok_run("let r = {t: \"ok\", v: 5} in r?\n");
    assert_result_eq(&res, serde_json::json!(5));
}

#[test]
fn g49_qmark_err_propagates_to_fn_boundary() {
    let (res, _out) = assert_ok_run("fn f(x) { x? }\nf({t: \"err\", e: \"boom\"})\n");
    assert_result_eq(&res, j("{\"t\":\"err\",\"e\":\"boom\"}"));
}

#[test]
fn g49_qmark_expect_result_error_code() {
    let (err, _stderr, _out) = assert_err_run("123?\n");
    assert_err_code(&err, "QMARK_EXPECT_RESULT");
}

#[test]
fn g50_pat_let_list_rest_smoke() {
    let (res, _out) = assert_ok_run("let [1, 2, ...r] = [1, 2, 3] in r\n");
    assert_result_eq(&res, serde_json::json!([3]));
}

#[test]
fn g50_pat_fn_param_destructure_smoke() {
    let (res, _out) = assert_ok_run("fn tail([1, 2, ...r]) { r }\ntail([1, 2, 9])\n");
    assert_result_eq(&res, serde_json::json!([9]));
}

#[test]
fn g50_pat_fn_param_mismatch_is_error() {
    let (err, _stderr, _out) = assert_err_run("fn t([1, 2, ...r]) { r }\nt([9, 2, 3])\n");
    assert_err_code(&err, "ERROR_PAT_MISMATCH");
}

#[test]
fn g51_match_ordering_first_hit_wins() {
    let (res, _out) = assert_ok_run("match 2 { 1 => 10, 2 => 20, _ => 30 }\n");
    assert_result_eq(&res, serde_json::json!(20));
}

#[test]
fn g51_match_guard_false_falls_through() {
    let (res, _out) = assert_ok_run("match 5 { x if x > 6 => 1, _ => 2 }\n");
    assert_result_eq(&res, serde_json::json!(2));
}

#[test]
fn g51_match_guard_true_selects_arm() {
    let (res, _out) = assert_ok_run("match 7 { x if x > 6 => 1, _ => 2 }\n");
    assert_result_eq(&res, serde_json::json!(1));
}

#[test]
fn g51_match_guard_not_bool_is_runtime_error() {
    let (err, _stderr, _out) = assert_err_run("match 5 { x if x + 6 => 1, _ => 2 }\n");
    assert_err_code(&err, "ERROR_RUNTIME");
    assert_err_msg_contains(&err, "match guard not bool");
    assert_err_span_text(&err, "x + 6");
    assert_err_span_offsets(&err, 1, 15, 20);
    assert_err_span_col(&err, 16);
}

#[test]
fn g51_match_nonexhaustive_is_error_match_no_arm() {
    let (err, _stderr, _out) = assert_err_run("match 5 { 1 => 1 }\n");
    assert_err_code(&err, "ERROR_MATCH_NO_ARM");
}

#[test]
fn g52_using_is_bind_scope_smoke() {
    let (res, _out) = assert_ok_run("using x = 5 in x + 1\n");
    assert_result_eq(&res, serde_json::json!(6));
}

#[test]
fn g52_using_pattern_bind_smoke() {
    let (res, _out) = assert_ok_run("using [a, b] = [2, 3] in a * b\n");
    assert_result_eq(&res, serde_json::json!(6));
}

#[test]
fn g52_using_mismatch_is_error() {
    let (err, _stderr, _out) = assert_err_run("using [a, b] = [1] in 0\n");
    assert_err_code(&err, "ERROR_PAT_MISMATCH");
}

#[test]
fn g53_hist_int_emits_k_records() {
    let src = r#"import("std/list") as list
list.hist_int([2,2,1])"#;
    let (res, _out) = assert_ok_run(src);
    let arr = res["result"].as_array().unwrap();
    assert!(arr.len() >= 1);
    for it in arr {
        assert!(it.get("k").is_some(), "hist_int element missing k: {it}");
        assert!(it.get("count").is_some(), "hist_int element missing count: {it}");
        assert!(it["k"].is_i64(), "hist_int k must be int: {it}");
        assert!(it["count"].is_i64(), "hist_int count must be int: {it}");
    }
}
#[test]
fn g53_lambda_single_param_eval() {
    let (res, _out) = assert_ok_run("let f = (x => x + 1) in f(4)\n");
    assert_result_eq(&res, serde_json::json!(5));
}

#[test]
fn g53_lambda_multi_param_eval() {
    let (res, _out) = assert_ok_run("let f = ((a, b) => a * b) in f(3, 4)\n");
    assert_result_eq(&res, serde_json::json!(12));
}

#[test]
fn g53_lambda_closure_capture_eval() {
    let (res, _out) = assert_ok_run("let k = 10 in let f = (x => x + k) in f(5)\n");
    assert_result_eq(&res, serde_json::json!(15));
}

fn assert_err_span_offsets(
    err: &serde_json::Value,
    line: usize,
    byte_start: usize,
    byte_end: usize,
) {
    let sp = err.get("span").expect("error.span must exist");
    let ln = sp.get("line").and_then(|v| v.as_u64()).unwrap() as usize;
    let bs = sp.get("byte_start").and_then(|v| v.as_u64()).unwrap() as usize;
    let be = sp.get("byte_end").and_then(|v| v.as_u64()).unwrap() as usize;
    assert_eq!(ln, line, "span.line mismatch: got {ln} expected {line}");
    assert_eq!(
        bs, byte_start,
        "span.byte_start mismatch: got {bs} expected {byte_start}"
    );
    assert_eq!(
        be, byte_end,
        "span.byte_end mismatch: got {be} expected {byte_end}"
    );
}

fn assert_err_span_col(err: &serde_json::Value, col: usize) {
    let sp = err.get("span").expect("error.span must exist");
    let cl = sp.get("col").and_then(|v| v.as_u64()).unwrap() as usize;
    assert_eq!(cl, col, "span.col mismatch: got {cl} expected {col}");
}

fn assert_err_span_text(err: &serde_json::Value, expected: &str) {
    let sp = err.get("span").expect("error.span must exist");
    let file = sp.get("file").and_then(|v| v.as_str()).unwrap();
    let line = sp.get("line").and_then(|v| v.as_u64()).unwrap() as usize;
    let bs = sp.get("byte_start").and_then(|v| v.as_u64()).unwrap() as usize;
    let be = sp.get("byte_end").and_then(|v| v.as_u64()).unwrap() as usize;

    let src = fs::read_to_string(file).unwrap();
    let line_txt = src.lines().nth(line - 1).unwrap_or("");
    let b = line_txt.as_bytes();
    let s = std::str::from_utf8(&b[bs.min(b.len())..be.min(b.len())]).unwrap();
    assert_eq!(
        s, expected,
        "span text mismatch: got {s:?} expected {expected:?}"
    );
}

#[test]
fn g53_kitchen_sink_program_smoke() {
    let prog = r#"
fn pair(a,b){[a,b]}
fn id(x){x}

fn safe(x){
  if x > 0 then {t:"ok", v:x} else {t:"err", e:"NEG"}
}

let xs = [5,1,7,2]

let a =
  xs
  | id
  | pair(9)

let b =
  match 7 {
    x if x > 6 => "big",
    _ => "small"
  }

fn main(){
  let y = safe(3)?
  {pipe:a, match:b, y:y}
}

main()
"#;

    let (res, _out) = assert_ok_run(prog);
    assert_result_eq(
        &res,
        serde_json::json!({"pipe":[[5,1,7,2], 9], "match":"big", "y":3}),
    );
}
