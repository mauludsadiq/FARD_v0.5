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

fn run_fard(prog_src: &str) -> serde_json::Value {
    let d = tmpdir("m1_result");
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
        .output().unwrap();
    assert!(out.status.success(), "fardrun failed\nstdout:\n{}\nstderr:\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));

    let result_path = outdir.join("result.json");
    let bs = fs::read(&result_path).unwrap();
    serde_json::from_slice(&bs).unwrap()
}

fn assert_result_shape(v: &serde_json::Value) {
    let obj = v.as_object().expect("result must be object");
    assert!(obj.len() == 2, "result must have exactly 2 keys");
    let t = obj
        .get("t")
        .and_then(|x| x.as_str())
        .expect("result must have t:string");
    match t {
        "ok" => {
            assert!(obj.contains_key("v"), "ok must have v");
            assert!(!obj.contains_key("e"), "ok must not have e");
        }
        "err" => {
            assert!(obj.contains_key("e"), "err must have e");
            assert!(!obj.contains_key("v"), "err must not have v");
        }
        _ => panic!("invalid result tag"),
    }
}

fn assert_runner_result_shape(v: &serde_json::Value) {
    let obj = v.as_object().expect("result.json must be object");
    assert!(obj.len() == 1, "runner envelope must have exactly 1 key; full = {}", serde_json::to_string_pretty(v).unwrap());
    assert!(obj.contains_key("result"), "runner envelope must have key result; full = {}", serde_json::to_string_pretty(v).unwrap());
}

fn get_obj(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("expected object")
}

#[test]
fn m1_result_semantics_closure() {
    let prog = r#"
import("std/result") as result

let ok7 = result.ok(7) in
let f = fn(x) { result.ok(x + 1) } in

let a = result.andThen(ok7, f) in
let expect_a = result.ok(8) in

let err_payload = {m:"no"} in
let errv = result.err(err_payload) in
let b = result.andThen(errv, f) in

let c = (fn(_u) { let _ = result.err({k:"keep"})? in result.ok(1) })(0) in

{
  a: a,
  expect_a: expect_a,
  b: b,
  errv: errv,
  c: c
}
"#;

    let top = run_fard(prog);

    assert_runner_result_shape(&top);
    let topobj = get_obj(&top);
    let v = topobj.get("result").unwrap();
    let root = get_obj(v);

    let a = root.get("a").unwrap();
    let expect_a = root.get("expect_a").unwrap();
    let b = root.get("b").unwrap();
    let errv = root.get("errv").unwrap();
    let c = root.get("c").unwrap();

    assert_result_shape(a);
    assert_result_shape(expect_a);
    assert_result_shape(b);
    assert_result_shape(errv);
    assert_result_shape(c);

    assert!(a == expect_a, "andThen(ok(v), f) must equal f(v) (here ok(8))");
    assert!(b == errv, "andThen(err(e), f) must equal err(e)");

    let cobj = get_obj(c);
    assert_eq!(cobj.get("t").unwrap().as_str().unwrap(), "err");
    let e = cobj.get("e").unwrap().as_object().unwrap();
    assert_eq!(
        e.get("k").unwrap().as_str().unwrap(),
        "keep",
        "? must preserve e payload exactly"
    );
}
