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
fn base64url_and_json_roundtrip() {
    let p = tmpfile("encoding", r#"
module main

pub fn main() : Value {
  let b = bytes_from_text("hello FARD")
  let encoded = base64url_encode(b)
  let decoded = base64url_decode(encoded)
  let eq = decoded.bytes_eq(b)

  let json_in = "{\"x\":1,\"y\":true}"
  let parsed = json_parse(json_in)
  let emitted = json_emit(parsed)

  {
    encoded: encoded,
    roundtrip_eq: eq,
    emitted: emitted
  }
}
"#);
    let got = run_fard(&p);
    assert_eq!(
        got,
        r#"{"t":"map","v":[["emitted",{"t":"text","v":"{\"x\":1,\"y\":true}"}],["encoded",{"t":"text","v":"aGVsbG8gRkFSRA"}],["roundtrip_eq",{"t":"bool","v":true}]]}"#
    );
}
