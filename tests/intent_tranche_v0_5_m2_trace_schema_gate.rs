use std::fs;
use std::process::Command;

fn write_file(path: &str, s: &str) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, s.as_bytes()).expect("write file");
}

#[allow(dead_code)]
#[allow(dead_code)]
fn read_nonempty_lines(p: &str) -> Vec<String> {
    let s = fs::read_to_string(p).expect("read trace");
    s.lines()
        .map(|x| x.to_string())
        .filter(|x| !x.trim().is_empty())
        .collect()
}

fn trace_paths(out_dir: &str) -> (String, String) {
    let p0 = format!("{out_dir}/trace.ndjson");
    let p1 = format!("{out_dir}/out/trace.ndjson");
    (p0, p1)
}

fn read_trace_any(out_dir: &str) -> Vec<String> {
    let (p0, p1) = trace_paths(out_dir);

    if let Ok(s) = fs::read_to_string(&p0) {
        let v: Vec<String> = s
            .lines()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty())
            .collect();
        if !v.is_empty() {
            return v;
        }
    }

    if let Ok(s) = fs::read_to_string(&p1) {
        let v: Vec<String> = s
            .lines()
            .map(|x| x.to_string())
            .filter(|x| !x.trim().is_empty())
            .collect();
        if !v.is_empty() {
            return v;
        }
    }

    panic!("trace must be non-empty at either path: {} OR {}", p0, p1);
}

fn assert_req_str(obj: &serde_json::Map<String, serde_json::Value>, k: &str) {
    assert!(
        obj.get(k).and_then(|x| x.as_str()).is_some(),
        "required field missing or not string: {k}"
    );
}

fn assert_has_key(obj: &serde_json::Map<String, serde_json::Value>, k: &str) {
    assert!(obj.contains_key(k), "required field missing: {k}");
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
            assert_has_key(obj, "v");
        }
        "grow_node" => {
            assert_has_key(obj, "v");
        }
        "module_resolve" => {
            assert_req_str(obj, "name");
            assert_req_str(obj, "kind");
            assert_req_str(obj, "cid");
        }
        "artifact_in" => {
            assert_req_str(obj, "path");
            assert_req_str(obj, "cid");
        }
        "artifact_out" => {
            assert_req_str(obj, "name");
            assert_req_str(obj, "cid");
        }
        "error" => {
            assert_req_str(obj, "code");
            assert_req_str(obj, "message");
            assert_has_key(obj, "e");
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

fn run_fard(name: &str, src: &str, expect_ok: bool) -> String {
    let program = format!("spec/tmp/{name}.fard");
    let outdir = format!("out/{name}");

    let _ = fs::remove_dir_all(&outdir);
    write_file(&program, src);

    let exe = env!("CARGO_BIN_EXE_fardrun");
    let status = Command::new(exe)
        .args(["run", "--program", &program, "--out", &outdir])
        .status()
        .expect("spawn fardrun");

    if expect_ok {
        assert!(status.success(), "runner nonzero: {name}");
    } else {
        assert!(!status.success(), "runner unexpectedly ok: {name}");
    }

    outdir
}

fn check_trace(outdir: &str) {
    let lines = read_trace_any(outdir);
    for line in lines {
        assert_m2_event_shape(&line);
    }
}

#[test]
fn m2_trace_schema_gate() {
    let out0 = run_fard(
        "m2_gate_emit_module",
        r#"
import("std/result") as Result
emit({k:"hello"})
0
"#,
        true,
    );

    write_file("spec/tmp/m2_gate_in.bin", "x");
    let out1 = run_fard(
        "m2_gate_artifact_in",
        r#"
emit({k:"in"})
let _x = import_artifact("spec/tmp/m2_gate_in.bin") in
0
"#,
        true,
    );

    let out2 = run_fard(
        "m2_gate_artifact_out",
        r#"
emit({k:"out"})
let _bs = emit_artifact("m2_gate_out.bin", {a:1}) in
0
"#,
        true,
    );

    let out3 = run_fard(
        "m2_gate_error",
        r#"
import("std/result") as Result
let _x = (Result.err({k:"e"}))? in
0
"#,
        false,
    );

    let out4 = run_fard(
        "m2_gate_grow_node",
        r#"
import("std/grow") as Grow
Grow.unfold_tree({n:0}, {depth:1})
0
"#,
        true,
    );

    check_trace(&out0);
    check_trace(&out1);
    check_trace(&out2);
    check_trace(&out3);
    check_trace(&out4);
}
