use std::fs;
use std::process::Command;

fn run_verify(outdir: &str) -> std::process::ExitStatus {
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardverify",
            "--",
            "artifact",
            "--out",
            outdir,
        ])
        .status()
        .expect("VERIFY_SPAWN_FAIL")
}

#[test]
fn m3_artifact_missing_file_rejected() {
    let out = "out/m3_missing_file";
    fs::remove_dir_all(out).ok();
    fs::create_dir_all(format!("{}/artifacts", out)).expect("MKDIR_ARTIFACTS_FAIL");

    let cid = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let graph = format!(
    "{{\"edges\":[],\"nodes\":[{{\"cid\":\"{}\",\"name\":\"x\",\"role\":\"out\"}}],\"v\":\"0.1.0\"}}\n",
    cid
  );
    fs::write(format!("{}/artifact_graph.json", out), graph.as_bytes()).expect("WRITE_GRAPH_FAIL");

    fs::write(
    format!("{}/trace.ndjson", out),
    b"{\"cid\":\"sha256:1111111111111111111111111111111111111111111111111111111111111111\",\"t\":\"module_graph\"}\n"
  ).expect("WRITE_TRACE_FAIL");

    fs::write(
        format!("{}/result.json", out),
        b"{\"result\":{\"t\":\"ok\",\"v\":1}}\n",
    )
    .expect("WRITE_RESULT_FAIL");

    fs::write(
    format!("{}/digests.json", out),
    b"{\"files\":{\"artifact_graph.json\":\"sha256:2222222222222222222222222222222222222222222222222222222222222222\"},\"ok\":true,\"preimage_sha256\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\",\"runtime_version\":\"0.5.0\",\"stdlib_root_digest\":\"sha256:c8627f9d8447f9b9781111e6a3698b2ead3686378caee89ffdfda6a6fba85f2c\",\"trace_format_version\":\"0.1.0\"}\n"
  ).expect("WRITE_DIGESTS_FAIL");

    let vst = run_verify(out);
    assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
