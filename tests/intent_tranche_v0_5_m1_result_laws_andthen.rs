/// M1 invariant: result.err(...) is a value (process exit remains success).
/// Nonzero exit is reserved for runtime/parse/export/lock/badarg class failures.

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

fn read_result_bytes(outdir: &str) -> Vec<u8> {
  fs::read(format!("{}/result.json", outdir)).expect("READ_RESULT_FAIL")
}

#[test]
fn m1_andthen_left_identity_ok() {
  let out_a = "out/m1_andthen_left_identity_a";
  let out_b = "out/m1_andthen_left_identity_b";
  let p_a = "spec/tmp/m1_andthen_left_identity_a.fard";
  let p_b = "spec/tmp/m1_andthen_left_identity_b.fard";

  let st_a = run(r#"
import("std/result") as result
let f = fn(x) { result.ok(x + 1) }
result.andThen(result.ok(10), f)
"#, out_a, p_a);
  assert!(st_a.success(), "RUNNER_NONZERO_A");

  let st_b = run(r#"
import("std/result") as result
let f = fn(x) { result.ok(x + 1) }
f(10)
"#, out_b, p_b);
  assert!(st_b.success(), "RUNNER_NONZERO_B");

  assert_eq!(read_result_bytes(out_a), read_result_bytes(out_b), "LEFT_IDENTITY_FAIL");
}

#[test]
fn m1_andthen_err_absorption() {
  let out_a = "out/m1_andthen_err_absorb_a";
  let out_b = "out/m1_andthen_err_absorb_b";
  let p_a = "spec/tmp/m1_andthen_err_absorb_a.fard";
  let p_b = "spec/tmp/m1_andthen_err_absorb_b.fard";

  let st_a = run(r#"
import("std/result") as result
let f = fn(x) { result.ok(x + 1) }
result.andThen(result.err({code:"E", msg:"x"}), f)
"#, out_a, p_a);
  assert!(st_a.success(), "RUNNER_NONZERO_A");

  let st_b = run(r#"
import("std/result") as result
result.err({code:"E", msg:"x"})
"#, out_b, p_b);
  assert!(st_b.success(), "RUNNER_NONZERO_B");

  assert_eq!(
    read_result_bytes(out_a),
    read_result_bytes(out_b),
    "ERR_ABSORPTION_FAIL"
  );
}
