use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn tmpdir(name: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!(
        "fard_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(p: &PathBuf, s: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, s.as_bytes()).unwrap();
}

fn run_fard_ok(prog_src: &str) -> serde_json::Value {
    let d = tmpdir("m2_pat_ok");
    let prog = d.join("main.fard");
    let outdir = d.join("out");
    fs::create_dir_all(&outdir).unwrap();
    write_file(&prog, prog_src);

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let out = Command::new(exe)
        .arg("run")
        .arg("--program")
        .arg(prog.to_string_lossy().to_string())
        .arg("--out")
        .arg(outdir.to_string_lossy().to_string())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "fardrun failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let result_path = outdir.join("result.json");
    let bs = fs::read(&result_path).unwrap();
    serde_json::from_slice(&bs).unwrap()
}

fn run_fard_err(prog_src: &str) -> String {
    let d = tmpdir("m2_pat_err");
    let prog = d.join("main.fard");
    let outdir = d.join("out");
    fs::create_dir_all(&outdir).unwrap();
    write_file(&prog, prog_src);

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let out = Command::new(exe)
        .arg("run")
        .arg("--program")
        .arg(prog.to_string_lossy().to_string())
        .arg("--out")
        .arg(outdir.to_string_lossy().to_string())
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "expected failure but succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    String::from_utf8_lossy(&out.stderr).to_string()
}

fn get_obj(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("expected object")
}

fn unwrap_runner_envelope(top: &serde_json::Value) -> &serde_json::Value {
    let obj = top.as_object().expect("result.json must be object");
    assert!(
        obj.len() == 1 && obj.contains_key("result"),
        "runner envelope must be {{result: ...}}; full = {}",
        serde_json::to_string_pretty(top).unwrap()
    );
    obj.get("result").unwrap()
}

fn assert_int(v: &serde_json::Value, n: i64, msg: &str) {
    assert_eq!(
        v.as_i64().unwrap(),
        n,
        "{} (full={})",
        msg,
        serde_json::to_string_pretty(v).unwrap()
    );
}

#[test]
fn m2_pattern_semantics_ok_paths() {
    let prog = r#"
import("std/result") as Result

let rec = {a: 1, b: 2, c: 3} in

let r_match = match rec {
  {a: x} => x,
  _ => 0
} in

let r_let = let {b: y} = rec in y in

let xs = [10, 20] in
let l_match = match xs {
  [p, q] => p + q,
  _ => 0
} in

let l_let = let [u, v] = xs in u * v in

let f = fn({a: z}) { z + 9 } in
let f_ok = f(rec) in

{
  r_match: r_match,
  r_let: r_let,
  l_match: l_match,
  l_let: l_let,
  f_ok: f_ok
}
"#;

    let top = run_fard_ok(prog);
    let v = unwrap_runner_envelope(&top);
    let root = get_obj(v);

    assert_int(root.get("r_match").unwrap(), 1, "record subset match via match");
    assert_int(root.get("r_let").unwrap(), 2, "record subset match via let");
    assert_int(root.get("l_match").unwrap(), 30, "list exact-length match via match");
    assert_int(root.get("l_let").unwrap(), 200, "list exact-length match via let");
    assert_int(root.get("f_ok").unwrap(), 10, "fn param pattern match (record subset)");
}

#[test]
fn m2_pattern_mismatch_errors_are_frozen() {
    let prog_let_mismatch = r#"
  let _ = let {a: y} = {b: 1} in y in
  0
  "#;
    let e1 = run_fard_err(prog_let_mismatch);
    assert!(
        e1.contains("ERROR_PAT_MISMATCH"),
        "let-pattern mismatch must be ERROR_PAT_MISMATCH; got:\n{}",
        e1
    );

    let prog_fn_mismatch = r#"
let f = fn({a: x}) { x } in
f({b: 1})
"#;
    let e2 = run_fard_err(prog_fn_mismatch);
    assert!(
        e2.contains("ERROR_PAT_MISMATCH"),
        "fn-param pattern mismatch must be ERROR_PAT_MISMATCH; got:\n{}",
        e2
    );

    let prog_match_no_arm = r#"
let _z = match {b: 1} {
  {a: _} => 1,
  [1,2] => 2
} in
0
"#;
    let e3 = run_fard_err(prog_match_no_arm);
    assert!(
        e3.contains("ERROR_MATCH_NO_ARM"),
        "match no-arm must be ERROR_MATCH_NO_ARM; got:\n{}",
        e3
    );
}

#[test]
fn m2_duplicate_bindings_are_rejected_deterministically() {
    let prog_dup = r#"
let _x = let {a: x, b: x} = {a: 1, b: 2} in x in
0
"#;
    let e = run_fard_err(prog_dup);

    let ok = e.contains("ERROR_PARSE") && (e.to_lowercase().contains("dup") || e.to_lowercase().contains("duplicate"));
    assert!(
        ok,
        "duplicate bindings must be rejected deterministically as parse error mentioning duplicate; got:\n{}",
        e
    );
}
