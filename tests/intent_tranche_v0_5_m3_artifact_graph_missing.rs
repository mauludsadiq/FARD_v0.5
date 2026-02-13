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
    .args(["run","-q","--bin","fardverify","--","artifact","--out",outdir])
    .status()
    .expect("VERIFY_SPAWN_FAIL")
}

#[test]
fn m3_artifact_graph_missing_rejected() {
  let out = "out/m3_graph_missing";
  let p = "spec/tmp/m3_graph_missing.fard";
  let st = run_runner(r#"
import("std/result") as result
result.ok(1)
"#, out, p);
  assert!(st.success(), "RUNNER_NONZERO");

  let dig_p = format!("{}/digests.json", out);
  let dig = fs::read_to_string(&dig_p).expect("READ_DIGESTS_FAIL");
  let patched = dig.replacen("{\"files\":{", "{\"files\":{\"artifact_graph.json\":\"sha256:3333333333333333333333333333333333333333333333333333333333333333\",", 1);
  fs::write(&dig_p, patched.as_bytes()).expect("WRITE_DIGESTS_FAIL");

  let vst = run_verify(out);
  assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
