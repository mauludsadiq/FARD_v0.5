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
fn m2_trace_vocabulary_gate_ok_run() {
  let out = "out/m2_vocab_ok";
  let p = "spec/tmp/m2_vocab_ok.fard";
  let st = run_runner(r#"
import("std/result") as result
result.ok(1)
"#, out, p);
  assert!(st.success(), "RUNNER_NONZERO");

  let vst = run_verify(out);
  assert!(vst.success(), "VERIFY_NONZERO");
  let pass = fs::read(format!("{}/PASS_TRACE.txt", out)).expect("MISSING_PASS_TRACE");
  assert!(pass.starts_with(b"PASS"), "BAD_PASS_TRACE");
}
