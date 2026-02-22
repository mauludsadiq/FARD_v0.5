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
fn bytes_builtins_via_methods() {
    let p = tmpfile("bytes_builtins", r#"
module main

pub fn main() : Value {
  let b = bytes_from_text("hello")
  let b2 = bytes_from_text(" world")
  let cat = b.bytes_concat(b2)
  let len = cat.bytes_len()
  let sliced = cat.bytes_slice(0, 5)
  let eq = b.bytes_eq(bytes_from_text("hello"))
  {
    len: len,
    eq: eq,
    sliced_len: sliced.bytes_len()
  }
}
"#);
    let got = run_fard(&p);
    assert_eq!(got, r#"{"t":"map","v":[["eq",{"t":"bool","v":true}],["len",{"t":"int","v":11}],["sliced_len",{"t":"int","v":5}]]}"#);
}

#[test]
fn bytes_slice_clamps() {
    let p = tmpfile("bytes_slice_clamps", r#"
module main

pub fn main() : Value {
  let b = bytes_from_text("abcdef")
  let s1 = b.bytes_slice(-10, 2)   // clamp start to 0 => "ab"
  let s2 = b.bytes_slice(4, 99)    // clamp end to len => "ef"
  { a: s1.bytes_len(), e: s2.bytes_len() }
}
"#);
    let got = run_fard(&p);
    assert_eq!(got, r#"{"t":"map","v":[["a",{"t":"int","v":2}],["e",{"t":"int","v":2}]]}"#);
}
