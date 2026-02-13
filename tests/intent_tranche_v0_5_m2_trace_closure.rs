use std::fs;
use std::process::Command;

fn write_file(path: &str, s: &str) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, s.as_bytes()).expect("write file");
}

fn run_probe(name: &str, src: &str, expect_ok: bool) {
    let program = format!("spec/tmp/{}.fard", name);
    let outdir = format!("out/{}", name);

    let _ = fs::remove_dir_all(&outdir);

    write_file(&program, src);

    let exe = env!("CARGO_BIN_EXE_fardrun");

    let status = Command::new(exe)
        .args(["run", "--program", &program, "--out", &outdir])
        .status()
        .expect("spawn fardrun");

    if expect_ok {
        assert!(status.success(), "probe runner nonzero: {}", name);
    } else {
        assert!(!status.success(), "probe runner unexpectedly ok: {}", name);
    }
}

#[allow(dead_code)]
fn read_lines(p: &str) -> Vec<String> {
    let s = fs::read_to_string(p).expect("read trace");
    s.lines()
        .map(|x| x.to_string())
        .filter(|x| !x.trim().is_empty())
        .collect()
}

fn assert_m2_event_shape(line: &str) {
    let v: serde_json::Value = serde_json::from_str(line).expect("trace line must be json");
    let obj = v.as_object().expect("trace line must be object");
    let t = obj
        .get("t")
        .and_then(|x| x.as_str())
        .expect("event.t string");

    match t {
        "emit" => {
            assert!(obj.contains_key("v"), "emit requires v");
        }
        "module_resolve" => {
            assert!(
                obj.get("name").and_then(|x| x.as_str()).is_some(),
                "module_resolve requires name:string"
            );
            assert!(
                obj.get("kind").and_then(|x| x.as_str()).is_some(),
                "module_resolve requires kind:string"
            );
            assert!(
                obj.get("cid").and_then(|x| x.as_str()).is_some(),
                "module_resolve requires cid:string"
            );
        }
        "artifact_in" => {
            assert!(
                obj.get("path").and_then(|x| x.as_str()).is_some(),
                "artifact_in requires path:string"
            );
            assert!(
                obj.get("cid").and_then(|x| x.as_str()).is_some(),
                "artifact_in requires cid:string"
            );
        }
        "artifact_out" => {
            assert!(
                obj.get("name").and_then(|x| x.as_str()).is_some(),
                "artifact_out requires name:string"
            );
            assert!(
                obj.get("cid").and_then(|x| x.as_str()).is_some(),
                "artifact_out requires cid:string"
            );
        }
        "error" => {
            assert!(
                obj.get("code").and_then(|x| x.as_str()).is_some(),
                "error requires code:string"
            );
            assert!(
                obj.get("message").and_then(|x| x.as_str()).is_some(),
                "error requires message:string"
            );
        }
        "module_graph" => {
            assert!(
                obj.get("cid").and_then(|x| x.as_str()).is_some(),
                "module_graph requires cid:string"
            );
        }
        _ => panic!("M2: unknown event kind: {t}"),
    }
}

fn check_trace_dir(out_dir: &str) {
    let p0 = format!("{out_dir}/trace.ndjson");
    let p1 = format!("{out_dir}/out/trace.ndjson");

    let mut lines: Vec<String> = Vec::new();

    if let Ok(s) = fs::read_to_string(&p0) {
        let v: Vec<String> = s
            .lines()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty())
            .collect();
        if !v.is_empty() {
            lines = v;
        }
    }

    if lines.is_empty() {
        if let Ok(s) = fs::read_to_string(&p1) {
            let v: Vec<String> = s
                .lines()
                .map(|x| x.to_string())
                .filter(|x| !x.trim().is_empty())
                .collect();
            if !v.is_empty() {
                lines = v;
            }
        }
    }

    assert!(
        !lines.is_empty(),
        "trace must be non-empty at either path: {} OR {}",
        p0,
        p1
    );

    for line in lines {
        assert_m2_event_shape(&line);
    }
}

#[test]
fn m2_trace_closure_across_probes() {
    run_probe(
        "m2_p0",
        r#"
import("std/result") as Result
emit({k:"p0"})
0
"#,
        true,
    );

    run_probe(
        "m2_p1",
        r#"
emit({k:"p1"})
let _bs = emit_artifact("p1.bin", {a:1}) in
0
"#,
        true,
    );

    write_file("spec/tmp/m2_in.bin", "x");
    run_probe(
        "m2_p2",
        r#"
emit({k:"p2"})
let _x = import_artifact("spec/tmp/m2_in.bin") in
0
"#,
        true,
    );

    run_probe(
        "m2_p3",
        r#"
import("std/result") as Result
let _x = (Result.err({k:"e"}))? in
0
"#,
        false,
    );

    run_probe(
        "m2_p4",
        r#"
emit({k:"n"})
0
"#,
        true,
    );

    check_trace_dir("out/m2_p0");
    check_trace_dir("out/m2_p1");
    check_trace_dir("out/m2_p2");
    check_trace_dir("out/m2_p3");
    check_trace_dir("out/m2_p4");
}
