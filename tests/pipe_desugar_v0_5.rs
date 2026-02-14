use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TMPCTR: AtomicUsize = AtomicUsize::new(0);

fn tmpdir(prefix: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let c = TMPCTR.fetch_add(1, Ordering::SeqCst);
    d.push(format!("{}_{}_{}_{}", prefix, std::process::id(), t, c));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn run_prog(src: &str) -> (i32, String, String, PathBuf) {
    let d = tmpdir("fard_pipe");
    let prog = d.join("main.fard");
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    fs::write(&prog, src.as_bytes()).unwrap();

    // DO NOT spawn `cargo run` during tests unless we must.
    // Cargo provides CARGO_BIN_EXE_fardrun for integration tests; use it to avoid target-dir lock contention.
    let exe = std::env::var("CARGO_BIN_EXE_fardrun").ok();

    let mut cmd = if let Some(exe) = exe {
        Command::new(exe)
    } else {
        let mut c = Command::new("cargo");
        c.args(["run", "-q", "--bin", "fardrun", "--"]);
        c
    };

    cmd.args([
        "run",
        "--program",
        prog.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);

    let outp = cmd.output().unwrap();
    let code = outp.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&outp.stdout).to_string();
    let stderr = String::from_utf8_lossy(&outp.stderr).to_string();
    (code, stdout, stderr, out)
}

fn read_json(path: &PathBuf) -> serde_json::Value {
    let b = fs::read(path).unwrap();
    serde_json::from_slice(&b).unwrap()
}

#[test]
fn pipe_desugar_a_value_to_callable() {
    let (code, _stdout, _stderr, out) = run_prog("fn id(x) { x }\n1 |> id\n");
    assert_eq!(code, 0);

    let err = out.join("error.json");
    assert!(!err.exists(), "error.json must not exist");

    let res = read_json(&out.join("result.json"));
    assert_eq!(res["result"], serde_json::json!(1));
    assert!(out.join("trace.ndjson").exists());
}

#[test]
fn pipe_desugar_b_value_to_call_with_args() {
    let (code, _stdout, _stderr, out) = run_prog("fn pair(a, b) { [a, b] }\n1 |> pair(9)\n");
    assert_eq!(code, 0);

    let err = out.join("error.json");
    assert!(!err.exists(), "error.json must not exist");

    let res = read_json(&out.join("result.json"));
    assert_eq!(res["result"], serde_json::json!([1, 9]));
    assert!(out.join("trace.ndjson").exists());
}
