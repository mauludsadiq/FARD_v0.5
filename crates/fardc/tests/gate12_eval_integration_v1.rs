use std::process::Command;

use valuecore::{dec, vdig};

#[test]
fn gate12_v1_eval_integration_produces_stable_runid() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    let tmp = tempfile::tempdir().unwrap();
    let bundle_dir = tmp.path().join("bundle");

    let src = root.join("fardc/tests/fixtures/gate12_eval_v1_main.fard");

    let out = Command::new(env!("CARGO_BIN_EXE_fardc"))
        .args([
            "--src",
            src.to_str().unwrap(),
            "--out",
            bundle_dir.to_str().unwrap(),
        ])
        .output()
        .expect("spawn fardc");
    assert!(
        out.status.success(),
        "fardc failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--manifest-path",
            "../../crates/abirunner/Cargo.toml",
            "--bin",
            "abirun",
            "--",
        ])
        .arg(&bundle_dir)
        .output()
        .expect("run abirun");
    assert!(
        out.status.success(),
        "abirun failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let witness_bytes = out.stdout;
    let w = dec(&witness_bytes).expect("decode witness");
    let runid = vdig(&w);

    // Freeze after first run:
    const FROZEN_RUNID: &str =
        "sha256:8ed574172a09bc38fd8dd4f70d0f1d119083a10790bee6d00340cceb545e41e2";
    std::assert_eq!(runid, FROZEN_RUNID);
    // Freeze after first run:
    // assert_eq!(stdout.trim(), "sha256:...");
}
