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

fn sha256_bytes_hex(bytes: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  let mut h = Sha256::new();
  h.update(bytes);
  format!("{:x}", h.finalize())
}

fn extract_module_graph_cid(trace: &str) -> String {
  for line in trace.lines() {
    if !line.contains(r#""t":"module_graph""#) { continue; }
    let v: serde_json::Value = serde_json::from_str(line).expect("TRACE_LINE_JSON_PARSE_FAIL");
    let cid = v.get("cid").and_then(|x| x.as_str()).expect("MODULE_GRAPH_EVENT_MISSING_CID");
    return cid.to_string();
  }
  panic!("MODULE_GRAPH_EVENT_MISSING");
}

fn check(outdir: &str) {
  let trace = fs::read_to_string(format!("{}/trace.ndjson", outdir)).expect("READ_TRACE_FAIL");
  let got_cid = extract_module_graph_cid(&trace);

  let mg_bytes = fs::read(format!("{}/module_graph.json", outdir)).expect("READ_MODULE_GRAPH_FAIL");
  let want_cid = format!("sha256:{}", sha256_bytes_hex(&mg_bytes));

  assert_eq!(got_cid, want_cid, "MODULE_GRAPH_CID_MISMATCH");
}

#[test]
fn m5_module_graph_cid_binding_ok() {
  let outdir = "out/m5_mg_cid_ok";
  let program = "spec/tmp/m5_mg_cid_ok.fard";
  let st = run(r#"
import("std/result") as result
result.ok(1)
"#, outdir, program);
  assert!(st.success(), "RUNNER_NONZERO");
  check(outdir);
}

#[test]
fn m5_module_graph_cid_binding_err() {
  let outdir = "out/m5_mg_cid_err";
  let program = "spec/tmp/m5_mg_cid_err.fard";
  let st = run(r#"
import("std/result") as result
let _ = result.err({code:"E", msg:"x"})?
result.ok(0)
"#, outdir, program);
  assert!(!st.success(), "EXPECTED_NONZERO");
  check(outdir);
}
