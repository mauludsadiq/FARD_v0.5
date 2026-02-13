use std::fs;
use std::process::Command;

fn run_runner(program_src: &str, outdir: &str, program_path: &str) -> std::process::ExitStatus {
  let _ = fs::remove_dir_all(outdir);
  fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
  fs::write(program_path, program_src.as_bytes()).expect("WRITE_SRC_FAIL");
  Command::new("cargo")
    .args(["run","-q","--bin","fardrun","--","run","--program",program_path,"--out",outdir])
    .status()
    .expect("RUNNER_SPAWN_FAIL")
}

fn run_verify(outdir: &str) -> std::process::ExitStatus {
  Command::new("cargo")
    .args(["run","-q","--bin","fardverify","--","trace","--out",outdir])
    .status()
    .expect("VERIFY_SPAWN_FAIL")
}

#[test]
fn m2_trace_canonicalization_gate_rejects_noncanonical_key_order() {
  let out = "out/m2_canon_bad_order";
  let p = "spec/tmp/m2_canon_bad_order.fard";
  let st = run_runner(r#"
import("std/result") as result
result.ok(1)
"#, out, p);
  assert!(st.success(), "RUNNER_NONZERO");

  let trace_p = format!("{}/trace.ndjson", out);
  let mut bytes = fs::read(&trace_p).expect("READ_TRACE_FAIL");
  let noncanon = b"{\"t\":\"artifact_out\",\"cid\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\",\"name\":\"x\"}\n";
  bytes.extend_from_slice(noncanon);
  fs::write(&trace_p, &bytes).expect("WRITE_TRACE_FAIL");

  let vst = run_verify(out);
  assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
