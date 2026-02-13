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
fn m5_extra_file_rejected() {
  let out = "out/m5_extra_file";
  let p = "spec/tmp/m5_extra_file.fard";
  let st = run_runner(r#"
import("std/result") as result
result.ok(4)
"#, out, p);
  assert!(st.success(), "RUNNER_NONZERO");

  let dp = format!("{}/digests.json", out);
  let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&dp).expect("READ_DIGESTS_FAIL"))
    .expect("DIGESTS_PARSE_FAIL");

  let files = v.get_mut("files").and_then(|x| x.as_object_mut()).expect("DIGESTS_FILES_NOT_OBJECT");
  files.insert("extra.json".to_string(), serde_json::Value::String("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()));

  let out_s = serde_json::to_string(&v).expect("DIGESTS_STRINGIFY_FAIL");
  fs::write(&dp, out_s.as_bytes()).expect("WRITE_DIGESTS_FAIL");

  let vst = run_verify_bundle(out);
  assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
