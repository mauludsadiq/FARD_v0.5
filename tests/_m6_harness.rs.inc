use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn tmpdir(name: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!(
        "fard_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

pub fn write_file(p: &PathBuf, s: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, s.as_bytes()).unwrap();
}

pub fn run_fard_ok(prog_src: &str) -> serde_json::Value {
    let d = tmpdir("m6_ok");
    let prog = d.join("main.fard");
    let outdir = d.join("out");
    fs::create_dir_all(&outdir).unwrap();
    write_file(&prog, prog_src);

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let out = Command::new(exe)
        .arg("run")
        .arg("--program")
        .arg(prog.to_string_lossy().to_string())
        .arg("--out")
        .arg(outdir.to_string_lossy().to_string())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "fardrun failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let result_path = outdir.join("result.json");
    let bs = fs::read(&result_path).unwrap();
    serde_json::from_slice(&bs).unwrap()
}

pub fn run_fard_err(prog_src: &str) -> String {
    let d = tmpdir("m6_err");
    let prog = d.join("main.fard");
    let outdir = d.join("out");
    fs::create_dir_all(&outdir).unwrap();
    write_file(&prog, prog_src);

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let out = Command::new(exe)
        .arg("run")
        .arg("--program")
        .arg(prog.to_string_lossy().to_string())
        .arg("--out")
        .arg(outdir.to_string_lossy().to_string())
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "expected failure but succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    String::from_utf8_lossy(&out.stderr).to_string()
}

pub fn unwrap_runner_envelope(top: &serde_json::Value) -> &serde_json::Value {
    let obj = top.as_object().expect("result.json must be object");
    assert!(
        obj.len() == 1 && obj.contains_key("result"),
        "runner envelope must be {{result: ...}}; full = {}",
        serde_json::to_string_pretty(top).unwrap()
    );
    obj.get("result").unwrap()
}

pub fn get_obj(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("expected object")
}

pub fn assert_int(v: &serde_json::Value, n: i64, msg: &str) {
    assert_eq!(
        v.as_i64().unwrap(),
        n,
        "{} (full={})",
        msg,
        serde_json::to_string_pretty(v).unwrap()
    );
}

pub fn assert_str(v: &serde_json::Value, s: &str, msg: &str) {
    assert_eq!(
        v.as_str().unwrap(),
        s,
        "{} (full={})",
        msg,
        serde_json::to_string_pretty(v).unwrap()
    );
}
