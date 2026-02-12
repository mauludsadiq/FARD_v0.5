use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

fn write_file(path: &str, s: &str) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, s.as_bytes()).expect("write file");
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

fn as_obj(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("event must be object")
}

fn req_str(obj: &serde_json::Map<String, serde_json::Value>, k: &str) -> String {
    obj.get(k)
        .and_then(|x| x.as_str())
        .unwrap_or_else(|| panic!("required field missing or not string: {k}"))
        .to_string()
}

fn req_arr(obj: &serde_json::Map<String, serde_json::Value>, k: &str) -> Vec<serde_json::Value> {
    obj.get(k)
        .and_then(|x| x.as_array())
        .unwrap_or_else(|| panic!("required field missing or not array: {k}"))
        .clone()
}

fn keyset(obj: &serde_json::Map<String, serde_json::Value>) -> BTreeSet<String> {
    obj.keys().cloned().collect()
}

fn assert_exact_keys(obj: &serde_json::Map<String, serde_json::Value>, expect: &[&str]) {
    let got = keyset(obj);
    let exp: BTreeSet<String> = expect.iter().map(|x| x.to_string()).collect();
    assert!(
        got == exp,
        "trace event has non-closed keyset.\nexpected: {:?}\n     got: {:?}",
        exp,
        got
    );
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

fn assert_m3_trace_closure(lines: &[String]) {
    let allowed: BTreeSet<&str> = ["emit", "module_resolve", "artifact_in", "artifact_out", "error", "grow_node"]
        .into_iter()
        .collect();

    for line in lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("trace line must be json");
        let obj = as_obj(&v);
        let t = req_str(obj, "t");

        assert!(allowed.contains(t.as_str()), "M3: unknown event kind: {t}");

        match t.as_str() {
            "emit" => {
                assert_exact_keys(obj, &["t", "v"]);
            }
            "grow_node" => {
                assert_exact_keys(obj, &["t", "v"]);
            }
            "module_resolve" => {
                let _name = req_str(obj, "name");
                let _kind = req_str(obj, "kind");
                let _cid = req_str(obj, "cid");
                assert_exact_keys(obj, &["t", "name", "kind", "cid"]);
            }
            "artifact_in" => {
                let _name = req_str(obj, "name");
                let _path = req_str(obj, "path");
                let _cid = req_str(obj, "cid");
                assert_exact_keys(obj, &["t", "name", "path", "cid"]);
            }
            "artifact_out" => {
                let _name = req_str(obj, "name");
                let _cid = req_str(obj, "cid");
                let parents = req_arr(obj, "parents");
                assert!(!parents.is_empty(), "artifact_out.parents must be non-empty for derived outputs: {_name}");

                for p in parents {
                    let pobj = p.as_object().expect("parent entry must be object");
                    let p_name = pobj
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or_else(|| panic!("parent.name missing or not string for child {_name}"))
                        .to_string();
                    let p_cid = pobj
                        .get("cid")
                        .and_then(|x| x.as_str())
                        .unwrap_or_else(|| panic!("parent.cid missing or not string for child {_name}"))
                        .to_string();
                    assert!(!p_name.is_empty(), "parent.name empty for child {_name}");
                    assert!(!p_cid.is_empty(), "parent.cid empty for child {_name}");

                    let pkeys: BTreeSet<String> = pobj.keys().cloned().collect();
                    let exp: BTreeSet<String> = ["name", "cid"].into_iter().map(|x| x.to_string()).collect();
                    assert!(pkeys == exp, "parent entry keyset must be exactly {{name,cid}} for child {_name}");
                }

                assert_exact_keys(obj, &["t", "name", "cid", "parents"]);
            }
            "error" => {
                let _code = req_str(obj, "code");
                let _message = req_str(obj, "message");
                assert!(obj.contains_key("e"), "required field missing: e");
                assert_exact_keys(obj, &["t","code","message","e"]);
            }
            _ => panic!("unreachable: allowed set mismatch"),
        }
    }
}

#[test]
fn m3_trace_closure_gate_import_then_emit_artifact() {
    write_file("spec/tmp/m3c_in.bin", "x");

    let outdir = run_fard(
        "m3c_gate_import_emit",
        r#"
emit({k:"m3c"})
let _x = import_artifact_named("in0", "spec/tmp/m3c_in.bin") in
let _y = emit_artifact_derived("out0", "m3c_out.bin", {a:1}, ["in0"]) in
0
"#,
        true,
    );

    let lines = read_trace_any(&outdir);
    assert_m3_trace_closure(&lines);
}

#[test]
fn m3_trace_closure_gate_error_shape_is_closed() {
    let outdir = run_fard(
        "m3c_gate_error",
        r#"
import("std/result") as Result
let _x = (Result.err({k:"e"}))? in
0
"#,
        false,
    );

    let lines = read_trace_any(&outdir);
    assert_m3_trace_closure(&lines);
}
