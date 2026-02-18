use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap()
        .to_path_buf()
}

#[test]
fn gate6_v1_canon_bytes_stable() {
    let root = repo_root();
    let tmp = root.join("_out_gate6");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let src = tmp.join("main.fard");
    fs::write(&src, r#"
// comment
module   main
effect read_file(path: text): bytes
fn   main()  : int   uses [read_file]   {   unit   }
"#).unwrap();

    let out = tmp.join("bundle");

    let exe = root.join("target").join("debug").join("fardc");
    if !exe.exists() {
        Command::new("cargo")
            .args(["build","--manifest-path","crates/fardc/Cargo.toml"])
            .current_dir(&root)
            .status().unwrap();
    }

    let o = Command::new(&exe)
        .args(["--src", src.to_str().unwrap(), "--out", out.to_str().unwrap()])
        .current_dir(&root)
        .output().unwrap();

    assert!(o.status.success(), "fardc failed: {}", String::from_utf8_lossy(&o.stderr));

    let cid = String::from_utf8_lossy(&o.stdout).trim().to_string();
    assert!(cid.starts_with("sha256:"));

    let hex = cid.strip_prefix("sha256:").unwrap();
    let src_path = out.join("sources").join(format!("{}.src", hex));
    let got = fs::read(&src_path).unwrap();
    let got_s = String::from_utf8(got).unwrap();

    // expected canonical (sorted, normalized)
    assert!(got_s.starts_with("module main\n"));
    assert!(got_s.contains("effect read_file(path: text): bytes\n"));
    assert!(got_s.contains("fn main(): int uses [read_file] { "));
}
