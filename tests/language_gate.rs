use std::path::{Path, PathBuf};

#[test]
fn gate_spec_parses() {
    let p = Path::new("tests/gate/gates.json");
    let bytes = std::fs::read(p).expect("read gates.json");
    let _v: serde_json::Value = serde_json::from_slice(&bytes).expect("parse gates.json");
}

#[test]
fn gate_runner_smoke_optional() {
    if std::env::var("RUN_FARD_GATES").ok().as_deref() != Some("1") {
        eprintln!("SKIP: set RUN_FARD_GATES=1 to execute the real FARD language-gate suite");
        return;
    }

    let exe: PathBuf = std::env::var_os("CARGO_BIN_EXE_gaterun")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("debug")
                .join(if cfg!(windows) { "gaterun.exe" } else { "gaterun" })
        });

    assert!(exe.exists(), "gaterun missing at {}", exe.display());

    let status = std::process::Command::new(&exe)
        .arg("--spec")
        .arg("tests/gate/gates.json")
        .arg("--config")
        .arg("fard_gate.toml")
        .status()
        .expect("run gaterun");

    assert!(status.success(), "gaterun failed");
}
