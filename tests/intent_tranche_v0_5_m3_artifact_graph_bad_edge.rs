use std::fs;
use std::process::Command;

fn run_runner(program_src: &str, outdir: &str, program_path: &str) -> std::process::ExitStatus {
    let _ = fs::remove_dir_all(outdir);
    fs::create_dir_all("spec/tmp").expect("MKDIR_TMP_FAIL");
    fs::write(program_path, program_src.as_bytes()).expect("WRITE_SRC_FAIL");
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardrun",
            "--",
            "run",
            "--program",
            program_path,
            "--out",
            outdir,
        ])
        .status()
        .expect("RUNNER_SPAWN_FAIL")
}

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
fn m3_artifact_graph_bad_edge_rejected() {
    let out = "out/m3_graph_bad_edge";
    let p = "spec/tmp/m3_graph_bad_edge.fard";
    let st = run_runner(
        r#"
import("std/result") as result
result.ok(1)
"#,
        out,
        p,
    );
    assert!(st.success(), "RUNNER_NONZERO");

    fs::create_dir_all(format!("{}/artifacts", out)).expect("MKDIR_ARTIFACTS_FAIL");
    let cid = "sha256:6666666666666666666666666666666666666666666666666666666666666666";
    fs::write(format!("{}/artifacts/{}.bin", out, &cid[7..]), b"x").expect("WRITE_ARTIFACT_FAIL");

    let bad = format!(
    "{{\"edges\":[{{\"from\":\"{}\",\"kind\":\"used_by\",\"to\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\"}}],\"nodes\":[{{\"cid\":\"{}\",\"name\":\"x\",\"role\":\"out\"}}],\"v\":\"0.1.0\"}}\n",
    cid, cid
  );
    fs::write(format!("{}/artifact_graph.json", out), bad.as_bytes()).expect("WRITE_GRAPH_FAIL");

    let dig_p = format!("{}/digests.json", out);
    let dig = fs::read_to_string(&dig_p).expect("READ_DIGESTS_FAIL");
    let patched = dig.replacen("{\"files\":{", "{\"files\":{\"artifact_graph.json\":\"sha256:7777777777777777777777777777777777777777777777777777777777777777\",", 1);
    fs::write(&dig_p, patched.as_bytes()).expect("WRITE_DIGESTS_FAIL");

    let vst = run_verify(out);
    assert!(!vst.success(), "EXPECTED_VERIFY_NONZERO");
}
