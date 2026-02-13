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

fn assert_exists(p: &str) { assert!(fs::metadata(p).is_ok(), "MISSING {}", p); }

#[test]
fn m5_bundle_exists_on_ok() {
  let outdir = "out/m5_bundle_ok";
  let program = "spec/tmp/m5_bundle_ok.fard";
  let st = run(r#"
import("std/result") as result
result.ok(1)
"#, outdir, program);
  assert!(st.success(), "RUNNER_NONZERO");

  assert_exists("out/m5_bundle_ok/trace.ndjson");
  assert_exists("out/m5_bundle_ok/module_graph.json");
  assert_exists("out/m5_bundle_ok/digests.json");
  assert_exists("out/m5_bundle_ok/result.json");
  assert!(fs::metadata("out/m5_bundle_ok/error.json").is_err(), "ERROR_JSON_SHOULD_NOT_EXIST");
}

#[test]
fn m5_bundle_exists_on_err() {
  let outdir = "out/m5_bundle_err";
  let program = "spec/tmp/m5_bundle_err.fard";
  let st = run(r#"
import("std/result") as result
let _ = result.err({code:"E", msg:"x"})?
result.ok(0)
"#, outdir, program);
  assert!(!st.success(), "EXPECTED_NONZERO");

  assert_exists("out/m5_bundle_err/trace.ndjson");
  assert_exists("out/m5_bundle_err/module_graph.json");
  assert_exists("out/m5_bundle_err/digests.json");
  assert_exists("out/m5_bundle_err/error.json");
  assert!(fs::metadata("out/m5_bundle_err/result.json").is_err(), "RESULT_JSON_SHOULD_NOT_EXIST");
}
