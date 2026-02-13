use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

fn run(program_src: &str, outdir: &str, program_path: &str) -> std::process::ExitStatus {
    let _ = fs::remove_dir_all(outdir);
    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(program_path, program_src.as_bytes()).expect("WRITE_SRC_FAIL");

    Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardrun",
            "--",
            "run",
            "--program",
            program_path,
            "--out",
            outdir,
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL")
}

fn assert_exists(p: &str) {
    assert!(fs::metadata(p).is_ok(), "MISSING {}", p);
}

fn sha256_bytes_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn sha256_file_hex(path: &str) -> String {
    let b = fs::read(path).expect("READ_FAIL");
    sha256_bytes_hex(&b)
}

fn must_str(v: &serde_json::Value, k: &str) -> String {
    v.get(k)
        .and_then(|x| x.as_str())
        .unwrap_or_else(|| panic!("MISSING_STR {}", k))
        .to_string()
}

fn must_obj(v: &serde_json::Value, k: &str) -> serde_json::Map<String, serde_json::Value> {
    v.get(k)
        .and_then(|x| x.as_object())
        .unwrap_or_else(|| panic!("MISSING_OBJ {}", k))
        .clone()
}

#[test]
fn m5_digests_self_verify_ok() {
    let outdir = "out/m5_digest_verify_ok";
    let program = "spec/tmp/m5_digest_verify_ok.fard";

    let st = run(
        r#"
import("std/result") as result
result.ok(1)
"#,
        outdir,
        program,
    );
    assert!(st.success(), "RUNNER_NONZERO");

    assert_exists("out/m5_digest_verify_ok/trace.ndjson");
    assert_exists("out/m5_digest_verify_ok/module_graph.json");
    assert_exists("out/m5_digest_verify_ok/digests.json");
    assert_exists("out/m5_digest_verify_ok/result.json");

    let dig_b = fs::read("out/m5_digest_verify_ok/digests.json").expect("READ_DIGESTS_FAIL");
    let dig: serde_json::Value = serde_json::from_slice(&dig_b).expect("DIGESTS_JSON_PARSE_FAIL");

    let runtime_version = must_str(&dig, "runtime_version");
    let trace_format_version = must_str(&dig, "trace_format_version");
    let stdlib_root_digest = must_str(&dig, "stdlib_root_digest");
    let ok = dig
        .get("ok")
        .and_then(|x| x.as_bool())
        .expect("MISSING_BOOL ok");
    assert!(ok, "EXPECTED_OK_TRUE");

    let files = must_obj(&dig, "files");

    // Ensure stable order for preimage reproduction (BTreeMap)
    let mut file_map: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in files.iter() {
        let s = v.as_str().unwrap_or_else(|| panic!("files[{}] not str", k));
        file_map.insert(k.clone(), s.to_string());
    }

    // recompute file hashes
    for (name, want) in file_map.iter() {
        let p = format!("{}/{}", outdir, name);
        let got = format!("sha256:{}", sha256_file_hex(&p));
        assert_eq!(&got, want, "FILE_DIGEST_MISMATCH {}", name);
    }

    // recompute preimage hash
    // preimage format must match runtime's write_m5_digests exactly.
    let mut pre = String::new();
    pre.push_str("cid_run_v0\n");
    pre.push_str(&format!("runtime_version={}\n", runtime_version));
    pre.push_str(&format!("trace_format_version={}\n", trace_format_version));
    pre.push_str(&format!("stdlib_root_digest={}\n", stdlib_root_digest));
    pre.push_str(&format!("ok={}\n", if ok { "true" } else { "false" }));
    for (name, h) in file_map.iter() {
        pre.push_str(&format!("{}={}\n", name, h));
    }
    let pre_h = format!("sha256:{}", sha256_bytes_hex(pre.as_bytes()));
    let want_pre = must_str(&dig, "preimage_sha256");
    assert_eq!(pre_h, want_pre, "PREIMAGE_HASH_MISMATCH");
}

#[test]
fn m5_digests_self_verify_err() {
    let outdir = "out/m5_digest_verify_err";
    let program = "spec/tmp/m5_digest_verify_err.fard";

    let st = run(
        r#"
import("std/result") as result
let _ = result.err({code:"E", msg:"x"})?
result.ok(0)
"#,
        outdir,
        program,
    );
    assert!(!st.success(), "EXPECTED_NONZERO");

    assert_exists("out/m5_digest_verify_err/trace.ndjson");
    assert_exists("out/m5_digest_verify_err/module_graph.json");
    assert_exists("out/m5_digest_verify_err/digests.json");
    assert_exists("out/m5_digest_verify_err/error.json");

    let dig_b = fs::read("out/m5_digest_verify_err/digests.json").expect("READ_DIGESTS_FAIL");
    let dig: serde_json::Value = serde_json::from_slice(&dig_b).expect("DIGESTS_JSON_PARSE_FAIL");

    let ok = dig
        .get("ok")
        .and_then(|x| x.as_bool())
        .expect("MISSING_BOOL ok");
    assert!(!ok, "EXPECTED_OK_FALSE");

    let files = must_obj(&dig, "files");
    assert!(files.contains_key("error.json"), "FILES_MISSING_error.json");
    assert!(
        !files.contains_key("result.json"),
        "FILES_SHOULD_NOT_INCLUDE_result.json"
    );
}
