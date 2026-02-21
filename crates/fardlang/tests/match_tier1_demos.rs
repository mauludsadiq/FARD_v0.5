use std::process::Command;

fn run(src: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_fard"))
        .args(["run", src])
        .output()
        .expect("run fard");
    assert!(
        out.status.success(),
        "fard failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    String::from_utf8(out.stdout).expect("utf8 stdout")
}

#[test]
fn match_bind_id_text() {
    let tmp = std::env::temp_dir().join("match_bind_id.fard");
    std::fs::write(
        &tmp,
        r#"module main

pub fn main() : Value {
  match "hello" { x => x }
}
"#,
    )
    .unwrap();
    let got = run(tmp.to_str().unwrap());
    assert_eq!(got.trim(), r#"{"t":"text","v":"hello"}"#);
}

#[test]
fn match_wild_fallback() {
    let tmp = std::env::temp_dir().join("match_wild.fard");
    std::fs::write(
        &tmp,
        r#"module main

pub fn main() : Value {
  match 7 { 0 => "zero", _ => "nonzero" }
}
"#,
    )
    .unwrap();
    let got = run(tmp.to_str().unwrap());
    assert_eq!(got.trim(), r#"{"t":"text","v":"nonzero"}"#);
}
