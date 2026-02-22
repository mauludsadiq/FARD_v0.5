use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn tmpfile(name: &str, contents: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    p.push(format!("fard_test_{}_{}_{}.fard", name, std::process::id(), nanos));
    fs::write(&p, contents).unwrap();
    p
}

fn run_fard(path: &PathBuf) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_fard"))
        .arg("run")
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn method_call_text_len() {
    let p = tmpfile("text_len", r#"
module main
pub fn main() : Value { "hello world".text_len() }
"#);
    let got = run_fard(&p);
    assert_eq!(got, r#"{"t":"int","v":11}"#);
}

#[test]
fn method_chain_inc() {
    let p = tmpfile("chain", r#"
module main
pub fn inc(x: Value) : Value { add(x, 1) }
pub fn main() : Value { 1.inc().inc().inc() }
"#);
    let got = run_fard(&p);
    assert_eq!(got, r#"{"t":"int","v":4}"#);
}

#[test]
fn closure_in_record_field() {
    let p = tmpfile("field_call", r#"
module main
pub fn main() : Value {
  let f = fn(x) { add(x, 10) }
  f(1)
}
"#);
    let got = run_fard(&p);
    assert_eq!(got, r#"{"t":"int","v":11}"#);
}
