use std::fs;
use std::io::Write;
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

fn shasum256_bytes(bytes: &[u8]) -> String {
  let mut child = Command::new("shasum")
    .args(["-a","256"])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()
    .expect("SHASUM_SPAWN_FAIL");
  child.stdin.as_mut().unwrap().write_all(bytes).expect("SHASUM_STDIN_FAIL");
  let out = child.wait_with_output().expect("SHASUM_WAIT_FAIL");
  let s = String::from_utf8(out.stdout).expect("SHASUM_UTF8_FAIL");
  s.split_whitespace().next().unwrap().to_string()
}

fn install_min_artifact_bundle(outdir: &str) {
  fs::create_dir_all(format!("{}/artifacts", outdir)).expect("MKDIR_ARTIFACTS_FAIL");

  let bytes = b"m3_extra_newline_bytes_v0";
  let hex = shasum256_bytes(bytes);
  let cid = format!("sha256:{}", hex);

  fs::write(format!("{}/artifacts/{}.bin", outdir, hex), bytes).expect("WRITE_ARTIFACT_BYTES_FAIL");

  let graph = format!(
    "{{\"edges\":[],\"nodes\":[{{\"cid\":\"{}\",\"name\":\"x\",\"role\":\"out\"}}],\"v\":\"0.1.0\"}}\n",
    cid
  );
  fs::write(format!("{}/artifact_graph.json", outdir), graph.as_bytes()).expect("WRITE_GRAPH_FAIL");

  let trace_p = format!("{}/trace.ndjson", outdir);
  let mut trace = fs::read_to_string(&trace_p).expect("READ_TRACE_FAIL");
  if !trace.ends_with('\n') { trace.push('\n'); }
  trace.push_str(&format!("{{\"cid\":\"{}\",\"name\":\"x\",\"t\":\"artifact_out\"}}\n", cid));
  fs::write(&trace_p, trace.as_bytes()).expect("WRITE_TRACE_FAIL");

  let graph_bytes = fs::read(format!("{}/artifact_graph.json", outdir)).expect("READ_GRAPH_FAIL");
  let graph_hex = shasum256_bytes(&graph_bytes);
  let graph_cid = format!("sha256:{}", graph_hex);

  let dig_p = format!("{}/digests.json", outdir);
  let dig = fs::read_to_string(&dig_p).expect("READ_DIGESTS_FAIL");
  let patched = dig.replacen("{\"files\":{", &format!("{{\"files\":{{\"artifact_graph.json\":\"{}\",", graph_cid), 1);
  fs::write(&dig_p, patched.as_bytes()).expect("WRITE_DIGESTS_FAIL");
}

#[test]
fn m3_artifact_graph_extra_newline_rejected() {
  let out = "out/m3_graph_extra_newline";
  let p = "spec/tmp/m3_graph_extra_newline.fard";

  let st = run_runner(r#"
import("std/result") as result
result.ok(1)
"#, out, p);
  assert!(st.success(), "RUNNER_NONZERO");

  install_min_artifact_bundle(out);

  let g = fs::read_to_string(format!("{}/artifact_graph.json", out)).expect("READ_GRAPH_FAIL");
  fs::write(format!("{}/artifact_graph.json", out), format!("{}\n", g)).expect("WRITE_GRAPH_FAIL");

  let vst = run_verify(out);
  assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
