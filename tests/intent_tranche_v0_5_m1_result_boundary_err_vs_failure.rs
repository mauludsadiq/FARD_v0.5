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

fn must_exist(path: &str) {
  assert!(std::path::Path::new(path).exists(), "MISSING_FILE");
}

fn must_not_exist(path: &str) {
  assert!(!std::path::Path::new(path).exists(), "UNEXPECTED_FILE");
}

#[test]
fn m1_boundary_err_value_vs_runner_failure() {
  let out_err = "out/m1_boundary_err_value";
  let out_fail = "out/m1_boundary_parse_fail";
  let p_err = "spec/tmp/m1_boundary_err_value.fard";
  let p_fail = "spec/tmp/m1_boundary_parse_fail.fard";

  let st_err = run(r#"
import("std/result") as result
result.err({code:"E", msg:"x"})
"#, out_err, p_err);
  assert!(st_err.success(), "ERR_VALUE_MUST_EXIT_SUCCESS");
  must_exist(&format!("{}/result.json", out_err));
  must_not_exist(&format!("{}/error.json", out_err));

  let st_fail = run(r#"
import("std/result") as result
match {t:"ok", v:1} {
  {t:"ok", v:v} => v,
  {t:"err", e:e} => e,
"#, out_fail, p_fail);
  assert!(!st_fail.success(), "PARSE_FAILURE_MUST_EXIT_NONZERO");
  must_exist(&format!("{}/error.json", out_fail));
  must_not_exist(&format!("{}/result.json", out_fail));
}
