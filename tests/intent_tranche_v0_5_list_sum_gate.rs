use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_fard(program: &str, out_dir: &str) -> (i32, String) {
    let root = repo_root();
    let out = root.join(out_dir);
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).unwrap();

    let bin = root.join("target").join("debug").join("fardrun");

    let mut cmd = Command::new(bin);
    cmd.arg("run")
        .arg("--program")
        .arg("/dev/stdin")
        .arg("--out")
        .arg(&out);

    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    use std::io::Write;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(program.as_bytes())
        .unwrap();

    let outp = child.wait_with_output().unwrap();
    let code = outp.status.code().unwrap_or(1);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&outp.stdout),
        String::from_utf8_lossy(&outp.stderr)
    );
    (code, combined)
}

#[test]
fn g_list_sum_returns_100() {
    let prog = r#"

import("std/list") as list

fn sum_list(xs: Dynamic, i: Int, acc: Int) -> Int {
  if i == list.len(xs) then acc
  else sum_list(xs, i + 1, acc + list.get(xs, i))
}

fn main() -> Int {
  let xs = [10, 20, 30, 40]
  sum_list(xs, 0, 0)
}

main()
"#;

    let (code, logs) = run_fard(prog, "_out_g_list_sum");
    assert_eq!(code, 0, "run failed:\n{logs}");

    let root = repo_root().join("_out_g_list_sum").join("result.json");
    let s = fs::read_to_string(&root).unwrap();
    assert_eq!(s.trim(), r#"{"result":100}"#);
}
