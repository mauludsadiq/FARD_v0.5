use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn tmpfile(name: &str, contents: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    p.push(format!("fard_test_{}_{}_{}.fard", name, std::process::id(), nanos));
    std::fs::write(&p, contents).unwrap();
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
fn sha256_hkdf_xchacha20poly1305_roundtrip() {
    let p = tmpfile("crypto", r#"
module main

pub fn main() : Value {
  let msg = bytes_from_text("hello FARD")
  let hash = sha256(msg)
  let key = hkdf_sha256(msg, bytes_from_text("salt"), bytes_from_text("info"), 32)
  let nonce = hkdf_sha256(msg, bytes_from_text("salt"), bytes_from_text("nonce"), 24)
  let aad = bytes_from_text("context")
  let ct = xchacha20poly1305_seal(key, nonce, aad, msg)
  let pt = xchacha20poly1305_open(key, nonce, aad, ct)
  let roundtrip = pt.bytes_eq(msg)
  {
    hash_len: hash.bytes_len(),
    key_len: key.bytes_len(),
    roundtrip: roundtrip
  }
}
"#);
    let got = run_fard(&p);
    assert_eq!(
        got,
        r#"{"t":"map","v":[["hash_len",{"t":"int","v":32}],["key_len",{"t":"int","v":32}],["roundtrip",{"t":"bool","v":true}]]}"#
    );
}
