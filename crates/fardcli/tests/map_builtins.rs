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
fn map_builtins_basic() {
    let p = tmpfile("map_basic", r#"
module main

pub fn main() : Value {
  let m = map_new()
  let m = map_set(m, "x", 1)
  let m = map_set(m, "y", 2)
  let has_x = map_has(m, "x")
  let has_z = map_has(m, "z")
  let keys = map_keys(m)
  let m2 = map_delete(m, "x")
  let has_x_after = map_has(m2, "x")
  {
    has_x: has_x,
    has_z: has_z,
    keys: keys,
    has_x_after: has_x_after
  }
}
"#);
    let got = run_fard(&p);
    assert_eq!(
        got,
        r#"{"t":"map","v":[["has_x",{"t":"bool","v":true}],["has_x_after",{"t":"bool","v":false}],["has_z",{"t":"bool","v":false}],["keys",{"t":"list","v":[{"t":"text","v":"x"},{"t":"text","v":"y"}]}]]}"#
    );
}

#[test]
fn map_set_overwrites_and_keys_sorted() {
    let p = tmpfile("map_overwrite", r#"
module main

pub fn main() : Value {
  let m = map_new()
  let m = map_set(m, "b", 2)
  let m = map_set(m, "a", 1)
  let m = map_set(m, "b", 99)
  {
    keys: map_keys(m),
    has_a: map_has(m, "a"),
    has_b: map_has(m, "b"),
    after_del: map_keys(map_delete(m, "a"))
  }
}
"#);
    let got = run_fard(&p);
    assert_eq!(
        got,
        r#"{"t":"map","v":[["after_del",{"t":"list","v":[{"t":"text","v":"b"}]}],["has_a",{"t":"bool","v":true}],["has_b",{"t":"bool","v":true}],["keys",{"t":"list","v":[{"t":"text","v":"a"},{"t":"text","v":"b"}]}]]}"#
    );
}
