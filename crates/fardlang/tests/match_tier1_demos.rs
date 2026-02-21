use std::process::Command;

fn fard_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../target/debug/fard");
    p
}

fn run(src: &str) -> String {
    let out = Command::new(fard_bin())
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
    std::fs::write(&tmp, "module main\n\npub fn main() : Value {\n  match \"hello\" { x => x }\n}\n").unwrap();
    let got = run(tmp.to_str().unwrap());
    assert_eq!(got.trim(), r#"{"t":"text","v":"hello"}"#);
}

#[test]
fn match_wild_fallback() {
    let tmp = std::env::temp_dir().join("match_wild.fard");
    std::fs::write(&tmp, "module main\n\npub fn main() : Value {\n  match 7 { 0 => \"zero\", _ => \"nonzero\" }\n}\n").unwrap();
    let got = run(tmp.to_str().unwrap());
    assert_eq!(got.trim(), r#"{"t":"text","v":"nonzero"}"#);
}
