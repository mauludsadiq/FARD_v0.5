use pretty_assertions::assert_eq;
use std::process::Command;

use valuecore::{dec, vdig};

#[test]
fn gate5_compiler_emits_bundle_and_runner_produces_frozen_witness_runid() {
    // crates/fardc
    let crate_root = env!("CARGO_MANIFEST_DIR");
    let src = format!("{}/tests/fixtures/main_unit.fard", crate_root);

    let outdir = std::env::temp_dir().join("fard_gate5_bundle");
    let _ = std::fs::remove_dir_all(&outdir);
    std::fs::create_dir_all(&outdir).unwrap();

    // 1) compile -> bundle ; compiler prints CID(source_bytes) for main module
    let c = Command::new(env!("CARGO_BIN_EXE_fardc"))
        .args(["--src", &src, "--out"])
        .arg(&outdir)
        .output()
        .expect("run fardc");

    assert!(
        c.status.success(),
        "fardc failed status={:?} stderr={}",
        c.status.code(),
        String::from_utf8_lossy(&c.stderr)
    );

    let source_cid_compiler = String::from_utf8(c.stdout).unwrap();
    let source_cid_compiler = source_cid_compiler.trim().to_string();

    let src_bytes = std::fs::read(&src).unwrap();
    let expected_source_cid = format!("sha256:{}", hex_lower(&valuecore::valuecore::Sha256::digest(&src_bytes)));

    assert_eq!(source_cid_compiler, expected_source_cid);

    // 2) runner -> witness bytes ; RunID := VDIG(dec(ENC(W*)))
    // We invoke abirunner via cargo to avoid guessing paths.
    let r = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--manifest-path",
            "../../crates/abirunner/Cargo.toml",
            "--bin",
            "abirun",
            "--",
        ])
        .arg(&outdir)
        .output()
        .expect("run abirun");

    assert!(
        r.status.success(),
        "abirun failed status={:?} stderr={}",
        r.status.code(),
        String::from_utf8_lossy(&r.stderr)
    );

    let witness_bytes = r.stdout;
    let w = dec(&witness_bytes).expect("decode witness");
    let runid_runner = vdig(&w);

    // Gate5 freeze: witness RunID for this compiled bundle
    const FROZEN_RUNID: &str =
        "sha256:ab7ebe0282b3bce23992bdb672a547d9eb152bef5434cc726284ac4301c63478";
    assert_eq!(runid_runner, FROZEN_RUNID);

    // Sanity: source CID != witness RunID (different objects)
    assert!(runid_runner != source_cid_compiler);
}
