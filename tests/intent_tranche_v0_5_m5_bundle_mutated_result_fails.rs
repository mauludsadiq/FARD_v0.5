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

fn run_verify_bundle(outdir: &str) -> std::process::ExitStatus {
  Command::new("cargo")
    .args(["run","-q","--bin","fardverify","--","bundle","--out",outdir])
    .status()
    .expect("VERIFY_SPAWN_FAIL")
}

#[test]
fn m5_mutated_result_fails() {
  let out = "out/m5_mut_result";
  let p = "spec/tmp/m5_mut_result.fard";
  let st = run_runner(r#"
import("std/result") as result
result.ok(2)
"#, out, p);
  assert!(st.success(), "RUNNER_NONZERO");

  let rp = format!("{}/result.json", out);
  let mut r = fs::read_to_string(&rp).expect("READ_RESULT_FAIL");
  r.push('\n');
  fs::write(&rp, r.as_bytes()).expect("WRITE_RESULT_FAIL");

  let vst = run_verify_bundle(out);
  assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
