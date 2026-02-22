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
fn list_builtins_via_methods() {
    let p = tmpfile("list_builtins", r#"
module main
pub fn main() : Value {
  let xs = [1, 2, 3]
  let appended  = xs.list_append(4)
  let concatted = xs.list_concat([4, 5])
  let reversed  = xs.list_reverse()
  let has_two   = xs.list_contains(2)
  let sliced    = xs.list_slice(1, 3)
  {
    appended: appended,
    concatted: concatted,
    reversed: reversed,
    has_two: has_two,
    sliced: sliced
  }
}
"#);
    let got = run_fard(&p);
    assert_eq!(got, r#"{"t":"map","v":[["appended",{"t":"list","v":[{"t":"int","v":1},{"t":"int","v":2},{"t":"int","v":3},{"t":"int","v":4}]}],["concatted",{"t":"list","v":[{"t":"int","v":1},{"t":"int","v":2},{"t":"int","v":3},{"t":"int","v":4},{"t":"int","v":5}]}],["has_two",{"t":"bool","v":true}],["reversed",{"t":"list","v":[{"t":"int","v":3},{"t":"int","v":2},{"t":"int","v":1}]}],["sliced",{"t":"list","v":[{"t":"int","v":2},{"t":"int","v":3}]}]]}"#);
}
