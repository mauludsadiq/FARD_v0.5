use std::fs;
use std::process::Command;

fn run(program_src: &str, outdir: &str, program_path: &str) -> std::process::ExitStatus {
  let _ = fs::remove_dir_all(outdir);
  fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
  fs::write(program_path, program_src.as_bytes()).expect("WRITE_SRC_FAIL");

  Command::new("cargo")
    .args(["run","-q","--bin","fardrun","--","run","--program",program_path,"--out",outdir])
    .status()
    .expect("RUNNER_SPAWN_FAIL")
}

fn must_obj(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
  v.as_object().expect("TYPE_FAIL obj")
}

fn must_str(v: &serde_json::Value, k: &str) -> String {
  v.get(k).and_then(|x| x.as_str()).unwrap_or_else(|| panic!("MISSING_STR {}", k)).to_string()
}

fn must_bool(v: &serde_json::Value, k: &str) -> bool {
  v.get(k).and_then(|x| x.as_bool()).unwrap_or_else(|| panic!("MISSING_BOOL {}", k))
}

fn must_obj_key(v: &serde_json::Value, k: &str) -> serde_json::Map<String, serde_json::Value> {
  v.get(k).and_then(|x| x.as_object()).unwrap_or_else(|| panic!("MISSING_OBJ {}", k)).clone()
}

fn assert_sha256_prefixed(s: &str) {
  assert!(s.starts_with("sha256:"), "BAD_HASH_PREFIX {}", s);
  let h = &s["sha256:".len()..];
  assert_eq!(h.len(), 64, "BAD_HASH_LEN {}", s);
  assert!(h.chars().all(|c| c.is_ascii_hexdigit() && c.is_ascii_lowercase() || c.is_ascii_digit()), "BAD_HASH_HEX {}", s);
}

fn check(outdir: &str, expect_ok: bool) {
  let dig_b = fs::read(format!("{}/digests.json", outdir)).expect("READ_DIGESTS_FAIL");
  let dig: serde_json::Value = serde_json::from_slice(&dig_b).expect("DIGESTS_JSON_PARSE_FAIL");

  let o = must_obj(&dig);

  // exact top-level keys
  let mut keys: Vec<&String> = o.keys().collect();
  keys.sort();
  let want = vec![
    "files".to_string(),
    "ok".to_string(),
    "preimage_sha256".to_string(),
    "runtime_version".to_string(),
    "stdlib_root_digest".to_string(),
    "trace_format_version".to_string(),
  ];
  let mut want_refs: Vec<&String> = want.iter().collect();
  want_refs.sort();
  assert_eq!(keys, want_refs, "DIGESTS_KEYS_MISMATCH");

  let _rv = must_str(&dig, "runtime_version");
  let _tf = must_str(&dig, "trace_format_version");
  let _sd = must_str(&dig, "stdlib_root_digest");
  let ok = must_bool(&dig, "ok");
  assert_eq!(ok, expect_ok, "OK_FLAG_MISMATCH");

  let pre = must_str(&dig, "preimage_sha256");
  assert_sha256_prefixed(&pre);

  let files = must_obj_key(&dig, "files");
  assert!(files.contains_key("trace.ndjson"), "FILES_MISSING_trace.ndjson");
  assert!(files.contains_key("module_graph.json"), "FILES_MISSING_module_graph.json");
  if ok {
    assert!(files.contains_key("result.json"), "FILES_MISSING_result.json");
    assert!(!files.contains_key("error.json"), "FILES_SHOULD_NOT_HAVE_error.json");
  } else {
    assert!(files.contains_key("error.json"), "FILES_MISSING_error.json");
    assert!(!files.contains_key("result.json"), "FILES_SHOULD_NOT_HAVE_result.json");
  }

  for (k,v) in files.iter() {
    let s = v.as_str().unwrap_or_else(|| panic!("FILES_VALUE_NOT_STR {}", k));
    assert_sha256_prefixed(s);
  }

  // exact file set size: 3
  assert_eq!(files.len(), 3, "FILES_LEN_MUST_BE_3");
}

#[test]
fn m5_digests_schema_ok() {
  let outdir = "out/m5_digests_schema_ok";
  let program = "spec/tmp/m5_digests_schema_ok.fard";
  let st = run(r#"
import("std/result") as result
result.ok(1)
"#, outdir, program);
  assert!(st.success(), "RUNNER_NONZERO");
  check(outdir, true);
}

#[test]
fn m5_digests_schema_err() {
  let outdir = "out/m5_digests_schema_err";
  let program = "spec/tmp/m5_digests_schema_err.fard";
  let st = run(r#"
import("std/result") as result
let _ = result.err({code:"E", msg:"x"})?
result.ok(0)
"#, outdir, program);
  assert!(!st.success(), "EXPECTED_NONZERO");
  check(outdir, false);
}
